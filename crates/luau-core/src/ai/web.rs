//! Link previews ("show thumbnail"): fetch a page's `<head>`, read Open Graph /
//! Twitter / HTML metadata and return title, description, site, favicon and a
//! thumbnail (inlined as a `data:` URL so the webview never contacts the site).
//!
//! SSRF protection (OWASP A10): only http(s) on default-ish ports, no
//! credentials in URLs, every host is resolved and **all** its addresses must
//! be public (no loopback, private, link-local, CGNAT, multicast, reserved,
//! documentation or IPv4-mapped equivalents). The connection is pinned to the
//! checked address (no DNS rebinding) and each redirect is re-checked.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::LazyLock;
use std::time::Duration;

use base64::Engine as _;
use futures_util::StreamExt;
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};

pub const MAX_HTML_BYTES: usize = 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 1536 * 1024;
const MAX_REDIRECTS: usize = 5;
const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preview {
    pub url: String,
    pub title: String,
    pub description: String,
    /// Thumbnail as a `data:image/…;base64,` URL.
    pub image: Option<String>,
    pub site: String,
    pub favicon: Option<String>,
    /// Fetched over plain http.
    pub insecure: bool,
    /// RFC 3339 fetch time (cache).
    pub fetched: String,
}

// --- address rules (pure) ---------------------------------------------------

pub fn is_public_v4(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || o[0] == 0 // "this network"
        || (o[0] == 100 && (o[1] & 0xc0) == 64) // CGNAT 100.64/10
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // IETF protocol assignments
        || (o[0] == 198 && (o[1] & 0xfe) == 18) // benchmarking 198.18/15
        || o[0] >= 240) // reserved
}

pub fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let s = ip.segments();
    // NAT64 64:ff9b::/96 embeds an IPv4 address.
    if s[0] == 0x64 && s[1] == 0xff9b && s[2..6] == [0, 0, 0, 0] {
        return is_public_v4(Ipv4Addr::new(
            (s[6] >> 8) as u8,
            s[6] as u8,
            (s[7] >> 8) as u8,
            s[7] as u8,
        ));
    }
    // IPv4-compatible ::a.b.c.d (deprecated).
    if s[..6] == [0, 0, 0, 0, 0, 0] {
        return false;
    }
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || (s[0] & 0xfe00) == 0xfc00 // unique local fc00::/7
        || (s[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        || (s[0] & 0xffc0) == 0xfec0 // site-local fec0::/10
        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        || (s[0] == 0x2002) // 6to4 (may embed private v4)
        || (s[0] == 0x2001 && s[1] == 0)) // Teredo
}

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v) => is_public_v4(v),
        IpAddr::V6(v) => is_public_v6(v),
    }
}

/// Validate a URL before any network access. Returns it normalized.
pub fn check_url(raw: &str) -> Result<Url> {
    let raw = raw.trim();
    if raw.len() > 2048 {
        return Err(Error::invalid("url too long"));
    }
    let u = Url::parse(raw).map_err(|_| Error::invalid("not a valid URL"))?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err(Error::invalid("only http(s) links have previews"));
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err(Error::invalid("links with credentials are not previewed"));
    }
    let host = u
        .host_str()
        .ok_or_else(|| Error::invalid("url has no host"))?
        .to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".home.arpa")
    {
        return Err(Error::invalid("local addresses are not previewed"));
    }
    if let Some(p) = u.port()
        && !matches!(p, 80 | 443 | 8080 | 8443)
    {
        return Err(Error::invalid("unusual port"));
    }
    if let Some(url::Host::Ipv4(ip)) = u.host()
        && !is_public_v4(ip)
    {
        return Err(Error::invalid("private addresses are not previewed"));
    }
    if let Some(url::Host::Ipv6(ip)) = u.host()
        && !is_public_v6(ip)
    {
        return Err(Error::invalid("private addresses are not previewed"));
    }
    Ok(u)
}

/// Resolve and require every address to be public; returns one to pin.
async fn resolve_public(u: &Url) -> Result<SocketAddr> {
    let host = u
        .host_str()
        .ok_or_else(|| Error::invalid("url has no host"))?
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    let port = u.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = tokio::task::spawn_blocking(move || {
        (host.as_str(), port).to_socket_addrs().map(|a| a.collect())
    })
    .await
    .map_err(|_| Error::Other("dns task failed".into()))?
    .map_err(|_| Error::not_found("host not found"))?;
    if addrs.is_empty() {
        return Err(Error::not_found("host not found"));
    }
    if addrs.iter().any(|a| !is_public_ip(a.ip())) {
        return Err(Error::invalid("the link points to a private address"));
    }
    Ok(addrs[0])
}

/// GET with SSRF checks, manual redirects and a body cap.
/// Returns `(final_url, content_type, body)`.
pub async fn safe_get(start: &str, accept: &str, cap: usize) -> Result<(Url, String, Vec<u8>)> {
    let mut u = check_url(start)?;
    for _ in 0..=MAX_REDIRECTS {
        let addr = resolve_public(&u).await?;
        let host = u
            .host_str()
            .unwrap_or_default()
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .resolve(&host, addr)
            .connect_timeout(Duration::from_secs(4))
            .timeout(TIMEOUT)
            .user_agent(concat!(
                "Mozilla/5.0 (compatible; Luau/",
                env!("CARGO_PKG_VERSION"),
                "; link preview)"
            ))
            .build()
            .map_err(|e| Error::Other(format!("http client: {e}")))?;
        let resp = client
            .get(u.clone())
            .header("accept", accept)
            .header("accept-language", "en;q=0.8, *;q=0.5")
            .send()
            .await
            .map_err(|e| Error::Other(format!("fetch failed: {}", e.without_url())))?;
        let status = resp.status();
        if status.is_redirection() {
            let loc = resp
                .headers()
                .get("location")
                .and_then(|l| l.to_str().ok())
                .ok_or_else(|| Error::Other("redirect without location".into()))?;
            let next = u.join(loc).map_err(|_| Error::invalid("bad redirect"))?;
            u = check_url(next.as_str())?;
            continue;
        }
        if !status.is_success() {
            return Err(Error::Other(format!("HTTP {}", status.as_u16())));
        }
        let ctype = resp
            .headers()
            .get("content-type")
            .and_then(|c| c.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut body = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let Ok(chunk) = chunk else { break };
            let room = cap.saturating_sub(body.len());
            body.extend_from_slice(&chunk[..chunk.len().min(room)]);
            if body.len() >= cap
                || (accept.starts_with("text/html") && contains_ci(&body, b"</head>"))
            {
                break;
            }
        }
        return Ok((u, ctype, body));
    }
    Err(Error::Other("too many redirects".into()))
}

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle))
}

// --- HTML metadata (pure) -----------------------------------------------------

static META_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<meta\b[^>]*>").unwrap());
static LINK_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<link\b[^>]*>").unwrap());
static TITLE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap());
static ATTR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)([a-z_:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'>]+))"#).unwrap()
});

fn attrs(tag: &str) -> Vec<(String, String)> {
    ATTR_RE
        .captures_iter(tag)
        .map(|c| {
            let v = c
                .get(2)
                .or(c.get(3))
                .or(c.get(4))
                .map(|m| m.as_str())
                .unwrap_or("");
            (c[1].to_ascii_lowercase(), v.to_string())
        })
        .collect()
}

/// Decode the handful of entities that appear in titles.
pub fn decode_entities(s: &str) -> String {
    let mut out = s
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");
    static NUM: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"&#(x?)([0-9a-fA-F]{1,6});").unwrap());
    out = NUM
        .replace_all(&out, |c: &regex::Captures| {
            let n = if &c[1] == "x" {
                u32::from_str_radix(&c[2], 16).ok()
            } else {
                c[2].parse().ok()
            };
            n.and_then(char::from_u32)
                .map(String::from)
                .unwrap_or_default()
        })
        .into_owned();
    out.replace("&amp;", "&")
}

fn clean_text(s: &str, max: usize) -> String {
    let s = decode_entities(s);
    let s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() > max {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    } else {
        s
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub title: String,
    pub description: String,
    pub image: Option<Url>,
    pub site: String,
    pub favicon: Option<Url>,
}

pub fn parse_meta(html: &str, base: &Url) -> Meta {
    let mut m = Meta::default();
    let mut og_title = None;
    let mut tw_title = None;
    let mut og_desc = None;
    let mut desc = None;
    let mut og_image = None;
    let mut tw_image = None;
    for t in META_RE.find_iter(html) {
        let a = attrs(t.as_str());
        let key = a
            .iter()
            .find(|(k, _)| k == "property" || k == "name")
            .map(|(_, v)| v.to_ascii_lowercase());
        let Some(content) = a
            .iter()
            .find(|(k, _)| k == "content")
            .map(|(_, v)| v.clone())
        else {
            continue;
        };
        match key.as_deref() {
            Some("og:title") => og_title = og_title.or(Some(content)),
            Some("twitter:title") => tw_title = tw_title.or(Some(content)),
            Some("og:description") => og_desc = og_desc.or(Some(content)),
            Some("description" | "twitter:description") => desc = desc.or(Some(content)),
            Some("og:image" | "og:image:url" | "og:image:secure_url") => {
                og_image = og_image.or(Some(content))
            }
            Some("twitter:image" | "twitter:image:src") => tw_image = tw_image.or(Some(content)),
            Some("og:site_name") if m.site.is_empty() => m.site = clean_text(&content, 80),
            _ => {}
        }
    }
    let title_tag = TITLE_RE.captures(html).map(|c| c[1].to_string());
    m.title = clean_text(
        &og_title.or(tw_title).or(title_tag).unwrap_or_default(),
        200,
    );
    m.description = clean_text(&og_desc.or(desc).unwrap_or_default(), 300);
    let abs = |s: &str| {
        base.join(decode_entities(s.trim()).as_str())
            .ok()
            .filter(|u| matches!(u.scheme(), "http" | "https"))
    };
    m.image = og_image.or(tw_image).and_then(|s| abs(&s));
    for t in LINK_RE.find_iter(html) {
        let a = attrs(t.as_str());
        let rel = a
            .iter()
            .find(|(k, _)| k == "rel")
            .map(|(_, v)| v.to_ascii_lowercase())
            .unwrap_or_default();
        if rel
            .split_whitespace()
            .any(|r| r == "icon" || r == "apple-touch-icon")
            && let Some(href) = a.iter().find(|(k, _)| k == "href").map(|(_, v)| v.clone())
        {
            m.favicon = abs(&href);
            if rel.contains("apple-touch-icon") {
                break;
            }
        }
    }
    if m.favicon.is_none() {
        m.favicon = base.join("/favicon.ico").ok();
    }
    if m.site.is_empty() {
        m.site = base
            .host_str()
            .unwrap_or_default()
            .trim_start_matches("www.")
            .to_string();
    }
    m
}

fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF8") {
        Some("image/gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else if bytes.starts_with(&[0, 0, 1, 0]) {
        Some("image/x-icon")
    } else {
        None // SVG is refused: it can carry scripts.
    }
}

/// Fetch an image and inline it (sniffed, never SVG).
pub async fn fetch_image(u: &Url, cap: usize) -> Option<String> {
    let (_, _, bytes) = safe_get(
        u.as_str(),
        "image/avif,image/webp,image/png,image/jpeg,image/*;q=0.8",
        cap + 1,
    )
    .await
    .ok()?;
    if bytes.len() > cap {
        return None;
    }
    let mime = sniff_image(&bytes)?;
    Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    ))
}

/// Build a preview for `url`.
pub async fn preview(url: &str) -> Result<Preview> {
    let (final_url, ctype, body) = safe_get(
        url,
        "text/html,application/xhtml+xml;q=0.9,*/*;q=0.1",
        MAX_HTML_BYTES,
    )
    .await?;
    let insecure = final_url.scheme() == "http";
    let mut p = Preview {
        url: final_url.to_string(),
        insecure,
        fetched: crate::history::now(),
        ..Default::default()
    };
    if ctype.starts_with("image/") {
        p.title = final_url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .unwrap_or("")
            .to_string();
        p.site = final_url.host_str().unwrap_or_default().to_string();
        p.image = fetch_image(&final_url, MAX_IMAGE_BYTES).await;
        return Ok(p);
    }
    if !(ctype.is_empty() || ctype.contains("html") || ctype.contains("xml")) {
        p.site = final_url.host_str().unwrap_or_default().to_string();
        return Ok(p);
    }
    let html = String::from_utf8_lossy(&body);
    let m = parse_meta(&html, &final_url);
    p.title = m.title;
    p.description = m.description;
    p.site = m.site;
    if let Some(img) = &m.image {
        p.image = fetch_image(img, MAX_IMAGE_BYTES).await;
    }
    if let Some(f) = &m.favicon {
        p.favicon = fetch_image(f, 64 * 1024).await;
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v4(s: &str) -> bool {
        is_public_ip(s.parse().unwrap())
    }

    #[test]
    fn ssrf_ip_rules() {
        for bad in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "198.18.0.1",
            "240.0.0.1",
            "192.0.2.1",
            "::1",
            "::",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "2001:db8::1",
            "::127.0.0.1",
            "ff02::1",
            "2002:a00:1::",
        ] {
            assert!(!v4(bad), "{bad} must be blocked");
        }
        for good in [
            "8.8.8.8",
            "1.1.1.1",
            "172.32.0.1",
            "100.128.0.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
        ] {
            assert!(v4(good), "{good} must be allowed");
        }
    }

    #[test]
    fn url_checks() {
        assert!(check_url("https://example.com/a?b=c").is_ok());
        assert!(check_url("http://example.com").is_ok());
        assert!(check_url("ftp://example.com").is_err());
        assert!(check_url("file:///etc/passwd").is_err());
        assert!(check_url("https://user:pw@example.com").is_err());
        assert!(check_url("http://localhost:8080").is_err());
        assert!(check_url("http://printer.local").is_err());
        assert!(check_url("http://127.0.0.1").is_err());
        assert!(check_url("http://[::1]/").is_err());
        assert!(check_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(check_url("https://example.com:22").is_err());
        assert!(check_url("https://example.com:8443").is_ok());
    }

    #[tokio::test]
    async fn resolution_blocks_private_names() {
        // "localhost.localdomain"-style names resolve to loopback where configured;
        // an IP literal via the resolver path must also be blocked.
        let u = Url::parse("http://127.0.0.1.nip.io.invalid").unwrap();
        assert!(resolve_public(&u).await.is_err());
    }

    #[test]
    fn parses_open_graph() {
        let base = Url::parse("https://www.example.com/post/1").unwrap();
        let html = r#"<html><head>
            <title>Fallback &amp; title</title>
            <meta property="og:title" content="A &quot;great&quot; post">
            <meta name="description" content="Plain   description">
            <meta content="/img/cover.png" property="og:image" />
            <meta property="og:site_name" content='Example Blog'>
            <link rel="shortcut icon" href="/fav.png">
        </head><body>ignored</body></html>"#;
        let m = parse_meta(html, &base);
        assert_eq!(m.title, "A \"great\" post");
        assert_eq!(m.description, "Plain description");
        assert_eq!(
            m.image.unwrap().as_str(),
            "https://www.example.com/img/cover.png"
        );
        assert_eq!(m.site, "Example Blog");
        assert_eq!(
            m.favicon.unwrap().as_str(),
            "https://www.example.com/fav.png"
        );
        let bare = parse_meta("<title>Only</title>", &base);
        assert_eq!(bare.title, "Only");
        assert_eq!(bare.site, "example.com");
        assert_eq!(
            bare.favicon.unwrap().as_str(),
            "https://www.example.com/favicon.ico"
        );
        let js = parse_meta(
            r#"<meta property="og:image" content="javascript:alert(1)">"#,
            &base,
        );
        assert!(js.image.is_none());
    }

    #[test]
    fn entities_and_sniffing() {
        assert_eq!(decode_entities("a &#233; &#x41; &amp;lt;"), "a é A &lt;");
        assert_eq!(sniff_image(b"\x89PNGxxxx"), Some("image/png"));
        assert_eq!(sniff_image(b"<svg onload=alert(1)>"), None);
    }
}
