//! Connected accounts (no secrets): `config/integrations.json`.
//! Each account carries its own host allow-list, checked before every request.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use super::types::ProviderKind;
use crate::error::{Error, Result};
use crate::fsutil::atomic_write;

pub const DEFAULT_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";
pub const TRELLO_API: &str = "https://api.trello.com";
pub const SLACK_API: &str = "https://slack.com";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SavedQuery {
    pub name: String,
    pub query: String,
    /// How `query` is interpreted (JQL when absent, for older configs).
    pub mode: super::query::SearchMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub provider: ProviderKind,
    pub label: String,
    /// Site / API base (`https://acme.atlassian.net`), without trailing slash.
    pub base_url: String,
    /// Hosts this account may talk to (lowercase). Redirects elsewhere are refused.
    pub allowed_hosts: Vec<String>,
    /// Plain HTTP explicitly allowed by the user (self-hosted only, warned in the UI).
    #[serde(default)]
    pub insecure_http: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_query: Option<String>,
    #[serde(default)]
    pub saved_queries: Vec<SavedQuery>,
    /// Extra CA certificate (PEM) for self-hosted sites.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ca_cert_path: Option<String>,
    /// Client certificate + private key (PEM) for mTLS.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert_path: Option<String>,
    /// Display name of the connected user (no email).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    /// False when the secret could only be kept in memory.
    #[serde(default)]
    pub persisted: bool,
    #[serde(default)]
    pub created: String,
}

impl Account {
    pub fn query(&self) -> String {
        match &self.default_query {
            Some(q) if !q.trim().is_empty() => q.clone(),
            _ if self.provider.is_jira() => DEFAULT_JQL.into(),
            _ => String::new(),
        }
    }
    pub fn host(&self) -> String {
        Url::parse(&self.base_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AccountsFile {
    schema: u32,
    accounts: Vec<Account>,
}

pub fn file(config: &Path) -> PathBuf {
    config.join("integrations.json")
}

pub fn load(config: &Path) -> Vec<Account> {
    std::fs::read_to_string(file(config))
        .ok()
        .and_then(|t| serde_json::from_str::<AccountsFile>(&t).ok())
        .map(|f| f.accounts)
        .unwrap_or_default()
}

pub fn save(config: &Path, accounts: &[Account]) -> Result<()> {
    let f = AccountsFile {
        schema: 1,
        accounts: accounts.to_vec(),
    };
    let text = serde_json::to_string_pretty(&f).map_err(|e| Error::Other(e.to_string()))?;
    atomic_write(&file(config), text.as_bytes())
}

pub fn find(config: &Path, id: &str) -> Result<Account> {
    load(config)
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| Error::not_found(format!("account {id}")))
}

/// Validate and normalize a user-entered site URL.
/// HTTPS only unless `allow_http` (explicit, warned opt-in); no credentials,
/// query or fragment; trailing slashes removed.
pub fn normalize_base_url(raw: &str, allow_http: bool) -> Result<Url> {
    let raw = raw.trim();
    let with_scheme = if raw.contains("://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    let mut u = Url::parse(&with_scheme).map_err(|_| Error::invalid("invalid_url"))?;
    match u.scheme() {
        "https" => {}
        "http" if allow_http => {}
        "http" => return Err(Error::invalid("insecure_url")),
        _ => return Err(Error::invalid("invalid_url")),
    }
    if u.host_str().is_none_or(str::is_empty) {
        return Err(Error::invalid("invalid_url"));
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err(Error::invalid("url_has_credentials"));
    }
    u.set_query(None);
    u.set_fragment(None);
    let path = u.path().trim_end_matches('/').to_string();
    u.set_path(&path);
    Ok(u)
}

pub fn base_string(u: &Url) -> String {
    u.as_str().trim_end_matches('/').to_string()
}

/// Default allow-list for a provider/site.
pub fn default_hosts(provider: ProviderKind, base: &Url) -> Vec<String> {
    let host = base.host_str().unwrap_or_default().to_ascii_lowercase();
    match provider {
        // Cloud attachment content redirects to the Atlassian media host.
        ProviderKind::JiraCloud => vec![host, "api.media.atlassian.com".into()],
        ProviderKind::JiraServer => vec![host],
        ProviderKind::Trello => vec!["api.trello.com".into(), "trello.com".into()],
        ProviderKind::Slack => vec!["slack.com".into()],
    }
}

/// Is `url` allowed for this account? Checked before every request and redirect.
pub fn host_allowed(account: &Account, url: &Url) -> bool {
    let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) else {
        return false;
    };
    let scheme_ok = match url.scheme() {
        "https" => true,
        // Plain HTTP only to the configured site itself, and only when opted in.
        "http" => account.insecure_http && host == account.host().to_ascii_lowercase(),
        _ => false,
    };
    scheme_ok
        && url.username().is_empty()
        && url.password().is_none()
        && account
            .allowed_hosts
            .iter()
            .any(|h| h.eq_ignore_ascii_case(&host))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acc(provider: ProviderKind, base: &str, insecure: bool) -> Account {
        let u = normalize_base_url(base, insecure).unwrap();
        Account {
            id: "a1".into(),
            provider,
            label: "Acme".into(),
            base_url: base_string(&u),
            allowed_hosts: default_hosts(provider, &u),
            insecure_http: insecure,
            default_query: None,
            saved_queries: vec![],
            ca_cert_path: None,
            client_cert_path: None,
            user_name: None,
            server_version: None,
            persisted: true,
            created: String::new(),
        }
    }

    #[test]
    fn normalizes_urls() {
        assert_eq!(
            base_string(&normalize_base_url("acme.atlassian.net/", false).unwrap()),
            "https://acme.atlassian.net"
        );
        assert_eq!(
            base_string(&normalize_base_url("https://jira.corp.io/jira/?x=1#y", false).unwrap()),
            "https://jira.corp.io/jira"
        );
        assert!(normalize_base_url("http://jira.corp.io", false).is_err());
        assert!(normalize_base_url("http://jira.corp.io", true).is_ok());
        assert!(normalize_base_url("ftp://jira.corp.io", true).is_err());
        assert!(normalize_base_url("https://user:pw@jira.corp.io", false).is_err());
        assert!(normalize_base_url("https://", false).is_err());
    }

    #[test]
    fn allow_list() {
        let a = acc(ProviderKind::JiraCloud, "https://acme.atlassian.net", false);
        let ok = |s: &str| host_allowed(&a, &Url::parse(s).unwrap());
        assert!(ok("https://acme.atlassian.net/rest/api/3/myself"));
        assert!(ok("https://ACME.atlassian.net/x"));
        assert!(ok("https://api.media.atlassian.com/file/1"));
        assert!(!ok("https://evil.example.com/"));
        assert!(!ok("https://acme.atlassian.net.evil.com/"));
        assert!(!ok("http://acme.atlassian.net/"));
        assert!(!ok("https://u:p@acme.atlassian.net/"));
    }

    #[test]
    fn http_only_when_opted_in_and_same_host() {
        let a = acc(ProviderKind::JiraServer, "http://jira.lan:8080", true);
        assert!(host_allowed(
            &a,
            &Url::parse("http://jira.lan:8080/rest/api/2/myself").unwrap()
        ));
        let mut b = a.clone();
        b.allowed_hosts.push("other.lan".into());
        assert!(!host_allowed(&b, &Url::parse("http://other.lan/").unwrap()));
        assert!(host_allowed(&b, &Url::parse("https://other.lan/").unwrap()));
    }

    #[test]
    fn default_query() {
        let mut a = acc(ProviderKind::JiraCloud, "https://acme.atlassian.net", false);
        assert_eq!(a.query(), DEFAULT_JQL);
        a.default_query = Some("project = X".into());
        assert_eq!(a.query(), "project = X");
        let t = acc(ProviderKind::Trello, TRELLO_API, false);
        assert_eq!(t.query(), "");
    }
}
