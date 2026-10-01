//! The `IssueProvider` port (SPEC §9.7): one enum, one adapter per service.
//! Everything above this layer (panel, links, mirrors, gate) is provider-agnostic.

use std::path::Path;

use super::accounts::{self, Account};
use super::http::Http;
use super::jira::Jira;
use super::secrets;
use super::slack::Slack;
use super::trello::Trello;
use super::types::*;
use crate::error::{Error, Result};

#[derive(Clone)]
pub enum Provider {
    Jira(Jira),
    Trello(Trello),
}

impl Provider {
    pub fn new(account: &Account, secret: &secrets::Secret) -> Result<Self> {
        let http = Http::new(account, secret)?;
        Ok(match account.provider {
            ProviderKind::JiraCloud => Provider::Jira(Jira { http, cloud: true }),
            ProviderKind::JiraServer => Provider::Jira(Jira { http, cloud: false }),
            ProviderKind::Trello => Provider::Trello(Trello { http }),
            ProviderKind::Slack => return Err(Error::invalid("not_an_issue_provider")),
        })
    }

    /// Load account + secret from config/keychain.
    pub fn open(config: &Path, account: &str) -> Result<(Account, Self)> {
        let a = accounts::find(config, account)?;
        let s = secrets::get(&a.id)?;
        let p = Self::new(&a, &s)?;
        Ok((a, p))
    }

    pub fn capabilities(&self) -> Capabilities {
        match self {
            Provider::Jira(_) => Capabilities {
                transitions: true,
                assign: true,
                labels: true,
                priority: true,
                comments: true,
                create: true,
                edit: true,
                sprints: true,
                jql: true,
                story_points: false,
                saved_filters: true,
            },
            Provider::Trello(_) => Capabilities {
                transitions: true,
                comments: true,
                create: true,
                edit: true,
                labels: true,
                ..Default::default()
            },
        }
    }

    pub fn http(&self) -> &Http {
        match self {
            Provider::Jira(j) => &j.http,
            Provider::Trello(t) => &t.http,
        }
    }

    pub async fn whoami(&self) -> Result<RemoteUser> {
        match self {
            Provider::Jira(j) => j.myself().await,
            Provider::Trello(t) => t.me().await,
        }
    }
    pub async fn search(&self, q: &str, next: Option<&str>) -> Result<SearchPage> {
        match self {
            Provider::Jira(j) => j.search(q, next).await,
            Provider::Trello(t) => t.search(q, next).await,
        }
    }
    pub async fn issue(&self, key: &str) -> Result<RemoteIssue> {
        match self {
            Provider::Jira(j) => j.issue(key).await,
            Provider::Trello(t) => t.card(key).await,
        }
    }
    pub async fn issues(&self, keys: &[String]) -> Result<Vec<RemoteIssue>> {
        match self {
            Provider::Jira(j) => j.issues(keys).await,
            Provider::Trello(t) => {
                let mut out = Vec::new();
                for k in keys.iter().take(100) {
                    if let Ok(i) = t.card(k).await {
                        out.push(i);
                    }
                }
                Ok(out)
            }
        }
    }
    pub async fn transitions(&self, key: &str) -> Result<Vec<Transition>> {
        match self {
            Provider::Jira(j) => j.transitions(key).await,
            Provider::Trello(t) => t.transitions(key).await,
        }
    }
    pub async fn transition(&self, key: &str, id: &str) -> Result<()> {
        match self {
            Provider::Jira(j) => j.transition(key, id).await,
            Provider::Trello(t) => t.transition(key, id).await,
        }
    }
    pub async fn comments(&self, key: &str) -> Result<Vec<RemoteComment>> {
        match self {
            Provider::Jira(j) => j.comments(key).await,
            Provider::Trello(t) => t.comments(key).await,
        }
    }
    pub async fn add_comment(&self, key: &str, md: &str) -> Result<()> {
        match self {
            Provider::Jira(j) => j.add_comment(key, md).await,
            Provider::Trello(t) => t.add_comment(key, md).await,
        }
    }
    pub async fn update(&self, key: &str, summary: Option<&str>, md: Option<&str>) -> Result<()> {
        match self {
            Provider::Jira(j) => j.update(key, summary, md).await,
            Provider::Trello(t) => t.update(key, summary, md).await,
        }
    }
    pub async fn create(&self, req: &CreateIssue) -> Result<String> {
        match self {
            Provider::Jira(j) => j.create(req).await,
            Provider::Trello(t) => t.create(req).await,
        }
    }
    pub async fn assign(&self, key: &str, user: Option<&RemoteUser>) -> Result<()> {
        match self {
            Provider::Jira(j) => j.assign(key, user).await,
            Provider::Trello(_) => Err(Error::invalid("unsupported")),
        }
    }
    pub async fn users(&self, q: &str) -> Result<Vec<RemoteUser>> {
        match self {
            Provider::Jira(j) => j.users(q).await,
            Provider::Trello(_) => Ok(vec![]),
        }
    }
    /// Projects (Jira) or boards (Trello) an issue can be created in.
    pub async fn projects(&self) -> Result<Vec<IdName>> {
        match self {
            Provider::Jira(j) => j.projects().await,
            Provider::Trello(t) => t.boards().await,
        }
    }
    /// Issue types (Jira) or lists (Trello) of a project/board.
    pub async fn issue_types(&self, project: &str) -> Result<Vec<IdName>> {
        match self {
            Provider::Jira(j) => j.issue_types(project).await,
            Provider::Trello(t) => t.lists(project).await,
        }
    }
    pub async fn boards(&self) -> Result<Vec<IdName>> {
        match self {
            Provider::Jira(j) => j.boards().await,
            Provider::Trello(t) => t.boards().await,
        }
    }
    pub async fn board(&self, id: &str) -> Result<(RemoteBoard, Vec<RemoteIssue>)> {
        match self {
            Provider::Jira(j) => j.board(id).await,
            Provider::Trello(t) => t.board(id).await,
        }
    }
    pub async fn download(&self, url: &str) -> Result<Vec<u8>> {
        self.http().bytes(url).await
    }
}

pub fn slack(config: &Path, account: &str) -> Result<(Account, Slack)> {
    let a = accounts::find(config, account)?;
    if a.provider != ProviderKind::Slack {
        return Err(Error::invalid("not_slack"));
    }
    let s = secrets::get(&a.id)?;
    Ok((
        a.clone(),
        Slack {
            http: Http::new(&a, &s)?,
        },
    ))
}
