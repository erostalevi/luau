//! HTTP client per account: rustls, host allow-list on every request and
//! redirect, auth headers (never URLs), `429`/`Retry-After` with exponential
//! backoff + jitter. Logs carry method, host-less path kind and status only.

use std::time::Duration;

use reqwest::{Method, StatusCode, header};
use serde_json::Value;
use url::Url;

use super::accounts::{Account, host_allowed};
use super::secrets::Secret;
use super::types::ProviderKind;
use crate::error::{Error, Result};

const MAX_ATTEMPTS: u32 = 4;
/// Largest response body we download (attachments).
pub const MAX_DOWNLOAD: usize = 20 * 1024 * 1024;

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    account: Account,
    auth: Option<header::HeaderValue>,
    base: Url,
}

pub fn remote_err(status: StatusCode) -> Error {
    match status.as_u16() {
        401 | 403 => Error::Other(format!("remote_auth:{}", status.as_u16())),
        404 => Error::NotFound("remote_404".into()),
        _ => Error::Other(format!("remote_http:{}", status.as_u16())),
    }
}

fn net_err(e: &reqwest::Error) -> Error {
    if e.is_redirect() {
        Error::invalid("redirect_not_allowed")
    } else if e.is_timeout() {
        Error::Other("remote_timeout".into())
    } else {
        Error::Other("remote_offline".into())
    }
}

/// Delay before retry `attempt` (1-based). Honours `Retry-After` seconds
/// (capped at 60 s); otherwise 0.5 s · 2^(attempt-1) plus up to 30 % jitter.
pub fn backoff_delay(attempt: u32, retry_after: Option<&str>, jitter01: f64) -> Duration {
    if let Some(secs) = retry_after.and_then(|v| v.trim().parse::<u64>().ok()) {
        return Duration::from_secs(secs.min(60));
    }
    let base = 500u64 * (1u64 << (attempt.saturating_sub(1)).min(6));
    let jitter = (base as f64 * 0.3 * jitter01.clamp(0.0, 1.0)) as u64;
    Duration::from_millis(base + jitter)
}

fn retryable(s: StatusCode) -> bool {
    matches!(s.as_u16(), 429 | 502 | 503 | 504)
}

fn jitter() -> f64 {
    let mut b = [0u8; 2];
    let _ = getrandom::fill(&mut b);
    u16::from_le_bytes(b) as f64 / u16::MAX as f64
}

fn auth_header(provider: ProviderKind, secret: &Secret) -> Result<Option<header::HeaderValue>> {
    use base64::Engine;
    let token = secret.token.trim();
    if token.is_empty() {
        return Err(Error::invalid("missing_token"));
    }
    let v = match (provider, secret.user.as_deref().map(str::trim)) {
        (ProviderKind::JiraCloud, Some(user)) | (ProviderKind::JiraServer, Some(user))
            if !user.is_empty() =>
        {
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{token}"))
            )
        }
        (ProviderKind::JiraCloud, _) => return Err(Error::invalid("missing_email")),
        (ProviderKind::JiraServer, _) | (ProviderKind::Slack, _) => format!("Bearer {token}"),
        (ProviderKind::Trello, Some(key)) if !key.is_empty() => {
            format!(r#"OAuth oauth_consumer_key="{key}", oauth_token="{token}""#)
        }
        (ProviderKind::Trello, _) => return Err(Error::invalid("missing_key")),
    };
    let mut hv = header::HeaderValue::from_str(&v).map_err(|_| Error::invalid("invalid_token"))?;
    hv.set_sensitive(true);
    Ok(Some(hv))
}

impl Http {
    pub fn new(account: &Account, secret: &Secret) -> Result<Self> {
        let base = Url::parse(&account.base_url).map_err(|_| Error::invalid("invalid_url"))?;
        if !host_allowed(account, &base) {
            return Err(Error::invalid("host_not_allowed"));
        }
        let guard = account.clone();
        let policy = reqwest::redirect::Policy::custom(move |a| {
            if a.previous().len() >= 5 {
                a.error("too many redirects")
            } else if host_allowed(&guard, a.url()) {
                a.follow()
            } else {
                a.error("redirect to a host outside the allow-list")
            }
        });
        let mut b = reqwest::Client::builder()
            .user_agent(concat!("Lull/", env!("CARGO_PKG_VERSION")))
            .redirect(policy)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .https_only(!account.insecure_http);
        if let Some(p) = &account.ca_cert_path {
            let pem = std::fs::read(p).map_err(|e| Error::io(p, e))?;
            for c in reqwest::Certificate::from_pem_bundle(&pem)
                .map_err(|_| Error::invalid("invalid_ca_cert"))?
            {
                b = b.add_root_certificate(c);
            }
        }
        if let Some(p) = &account.client_cert_path {
            let pem = std::fs::read(p).map_err(|e| Error::io(p, e))?;
            let id = reqwest::Identity::from_pem(&pem)
                .map_err(|_| Error::invalid("invalid_client_cert"))?;
            b = b.identity(id);
        }
        let client = b.build().map_err(|_| Error::Other("http_client".into()))?;
        Ok(Http {
            client,
            auth: auth_header(account.provider, secret)?,
            account: account.clone(),
            base,
        })
    }

    pub fn base(&self) -> &Url {
        &self.base
    }

    /// Resolve `path` (absolute URL or path relative to the base) and check the allow-list.
    pub fn url(&self, path: &str) -> Result<Url> {
        let u = if path.starts_with("https://") || path.starts_with("http://") {
            Url::parse(path).map_err(|_| Error::invalid("invalid_url"))?
        } else {
            let base = self.base.as_str().trim_end_matches('/');
            Url::parse(&format!("{base}{path}")).map_err(|_| Error::invalid("invalid_url"))?
        };
        if !host_allowed(&self.account, &u) {
            tracing::warn!("blocked request to a host outside the allow-list");
            return Err(Error::invalid("host_not_allowed"));
        }
        Ok(u)
    }

    async fn send(
        &self,
        method: Method,
        url: Url,
        body: Option<&Value>,
    ) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut rq = self
                .client
                .request(method.clone(), url.clone())
                .header(header::ACCEPT, "application/json");
            if let Some(a) = &self.auth {
                rq = rq.header(header::AUTHORIZATION, a.clone());
            }
            if let Some(b) = body {
                rq = rq.json(b);
            }
            match rq.send().await {
                Ok(r) if r.status().is_success() => return Ok(r),
                Ok(r) if retryable(r.status()) && attempt < MAX_ATTEMPTS => {
                    let ra = r
                        .headers()
                        .get(header::RETRY_AFTER)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string);
                    let d = backoff_delay(attempt, ra.as_deref(), jitter());
                    tracing::info!(
                        "remote {} {} → {}, retrying in {:?}",
                        method,
                        url.path(),
                        r.status().as_u16(),
                        d
                    );
                    tokio::time::sleep(d).await;
                }
                Ok(r) => {
                    tracing::info!("remote {} {} → {}", method, url.path(), r.status().as_u16());
                    return Err(remote_err(r.status()));
                }
                Err(e) if !e.is_redirect() && attempt < 2 => {
                    tokio::time::sleep(backoff_delay(attempt, None, jitter())).await;
                }
                Err(e) => {
                    tracing::info!(
                        "remote {} {} failed: {}",
                        method,
                        url.path(),
                        if e.is_timeout() { "timeout" } else { "network" }
                    );
                    return Err(net_err(&e));
                }
            }
        }
    }

    pub async fn json(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value> {
        let mut u = self.url(path)?;
        if !query.is_empty() {
            u.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        }
        let r = self.send(method, u, body).await?;
        if r.status() == StatusCode::NO_CONTENT {
            return Ok(Value::Null);
        }
        let bytes = r.bytes().await.map_err(|e| net_err(&e))?;
        if bytes.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::Other("remote_bad_json".into()))
    }

    pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        self.json(Method::GET, path, query, None).await
    }
    pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
        self.json(Method::POST, path, &[], Some(body)).await
    }
    pub async fn put(&self, path: &str, body: &Value) -> Result<Value> {
        self.json(Method::PUT, path, &[], Some(body)).await
    }

    /// Download a file (attachments), bounded by [`MAX_DOWNLOAD`].
    pub async fn bytes(&self, path: &str) -> Result<Vec<u8>> {
        let u = self.url(path)?;
        let r = self.send(Method::GET, u, None).await?;
        if r.content_length()
            .is_some_and(|l| l as usize > MAX_DOWNLOAD)
        {
            return Err(Error::invalid("download_too_large"));
        }
        let b = r.bytes().await.map_err(|e| net_err(&e))?;
        if b.len() > MAX_DOWNLOAD {
            return Err(Error::invalid("download_too_large"));
        }
        Ok(b.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_honours_retry_after() {
        assert_eq!(backoff_delay(1, None, 0.0), Duration::from_millis(500));
        assert_eq!(backoff_delay(2, None, 0.0), Duration::from_millis(1000));
        assert_eq!(backoff_delay(3, None, 0.0), Duration::from_millis(2000));
        assert_eq!(backoff_delay(3, None, 1.0), Duration::from_millis(2600));
        assert_eq!(backoff_delay(1, Some("7"), 0.5), Duration::from_secs(7));
        assert_eq!(backoff_delay(1, Some("9999"), 0.5), Duration::from_secs(60));
        // Unparseable Retry-After (HTTP date) falls back to exponential.
        assert_eq!(
            backoff_delay(1, Some("Wed, 21 Oct 2026"), 0.0),
            Duration::from_millis(500)
        );
        assert!(backoff_delay(40, None, 0.0) <= Duration::from_millis(500 * 64));
    }

    #[test]
    fn auth_headers_by_provider() {
        let s = |u: Option<&str>| Secret {
            user: u.map(str::to_string),
            token: "t".into(),
        };
        let h = auth_header(ProviderKind::JiraCloud, &s(Some("a@b.c")))
            .unwrap()
            .unwrap();
        assert!(h.is_sensitive() && h.to_str().unwrap().starts_with("Basic "));
        assert!(auth_header(ProviderKind::JiraCloud, &s(None)).is_err());
        assert_eq!(
            auth_header(ProviderKind::JiraServer, &s(None))
                .unwrap()
                .unwrap()
                .to_str()
                .unwrap(),
            "Bearer t"
        );
        assert!(
            auth_header(ProviderKind::Trello, &s(Some("k")))
                .unwrap()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("OAuth ")
        );
        assert!(auth_header(ProviderKind::Slack, &Secret::default()).is_err());
    }
}
