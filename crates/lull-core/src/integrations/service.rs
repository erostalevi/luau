//! Application layer for integrations: accounts, remote search, linked copies,
//! mirrors, the WriteGate prepare/commit flow, images and the watch loop.
//! Network calls are async; board mutations go through `Core` (short locks,
//! never held across an `.await`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::accounts::{self, Account, SLACK_API, SavedQuery, TRELLO_API};
use super::gate::{self, Action, ChangeRow, Direction, Draft, Prepared};
use super::links::{self, Link, LinkInfo, MirrorSource, MirrorState, RemoteInfoDto};
use super::mirror;
use super::provider::{self, Provider};
use super::secrets::{self, Secret};
use super::types::*;
use crate::app::{Core, CoreEvent, files};
use crate::error::{Error, Result};
use crate::fsutil::sha256_hex;
use crate::history::{self, JournalEntry, Origin};
use crate::ids::{IdKind, new_id};
use crate::model::{BoardKind, Parent};
use crate::store::{BoardStore, Op};

pub const ALLOW_PUSH: &str = "integrations.allowPush";
pub const ALLOW_PULL: &str = "integrations.allowPull";
const MAX_IMAGES: usize = 20;

fn setting_bool(core: &Core, key: &str, default: bool) -> bool {
    core.settings()
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

pub fn ensure_allowed(core: &Core, dir: Direction) -> Result<()> {
    match dir {
        Direction::Push if !setting_bool(core, ALLOW_PUSH, false) => {
            Err(Error::invalid("push_disabled"))
        }
        Direction::Pull if !setting_bool(core, ALLOW_PULL, true) => {
            Err(Error::invalid("pull_disabled"))
        }
        _ => Ok(()),
    }
}

fn emit(core: &Core, name: &str, payload: Value) {
    core.sink.emit(CoreEvent::Custom {
        name: name.into(),
        payload,
    });
}

fn hash(s: &str) -> String {
    sha256_hex(s.as_bytes())[..16].to_string()
}

// --- accounts ------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConnectReq {
    pub provider: Option<ProviderKind>,
    pub label: String,
    pub base_url: String,
    /// Jira Cloud email / Trello API key / legacy server username.
    pub user: Option<String>,
    pub token: String,
    pub insecure_http: bool,
    pub ca_cert_path: Option<String>,
    pub client_cert_path: Option<String>,
}

fn check_pem_path(p: &Option<String>) -> Result<()> {
    if let Some(p) = p {
        let pb = PathBuf::from(p);
        if !pb.is_absolute() || !pb.is_file() {
            return Err(Error::invalid("invalid_cert_path"));
        }
    }
    Ok(())
}

/// Validate, test the connection, then store the secret in the keychain.
pub async fn connect(core: &Arc<Core>, req: ConnectReq) -> Result<Account> {
    let provider = req
        .provider
        .ok_or_else(|| Error::invalid("missing_provider"))?;
    if req.token.trim().is_empty() || req.token.len() > 4096 {
        return Err(Error::invalid("missing_token"));
    }
    check_pem_path(&req.ca_cert_path)?;
    check_pem_path(&req.client_cert_path)?;
    let insecure = req.insecure_http && provider == ProviderKind::JiraServer;
    let base = match provider {
        ProviderKind::Trello => accounts::normalize_base_url(TRELLO_API, false)?,
        ProviderKind::Slack => accounts::normalize_base_url(SLACK_API, false)?,
        _ => accounts::normalize_base_url(&req.base_url, insecure)?,
    };
    let mut account = Account {
        id: new_id(IdKind::Mirror, |_| false).replacen('m', "a", 1),
        provider,
        label: req.label.trim().chars().take(80).collect(),
        base_url: accounts::base_string(&base),
        allowed_hosts: accounts::default_hosts(provider, &base),
        insecure_http: insecure,
        default_query: None,
        saved_queries: vec![],
        ca_cert_path: req.ca_cert_path.clone(),
        client_cert_path: req.client_cert_path.clone(),
        user_name: None,
        server_version: None,
        persisted: false,
        created: history::now(),
    };
    let secret = Secret {
        user: req
            .user
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty()),
        token: req.token.trim().to_string(),
    };
    test_account(&mut account, &secret).await?;
    if account.label.is_empty() {
        account.label = match provider {
            ProviderKind::Trello => "Trello".into(),
            ProviderKind::Slack => account.user_name.clone().unwrap_or_else(|| "Slack".into()),
            _ => account.host(),
        };
    }
    account.persisted = secrets::set(&account.id, &secret)?;
    let mut list = accounts::load(&core.paths.config);
    list.push(account.clone());
    accounts::save(&core.paths.config, &list)?;
    tracing::info!("integration account added ({:?})", provider);
    emit(core, "integrations.accounts", json!({}));
    Ok(account)
}

async fn test_account(account: &mut Account, secret: &Secret) -> Result<()> {
    match account.provider {
        ProviderKind::Slack => {
            let s = super::slack::Slack {
                http: super::http::Http::new(account, secret)?,
            };
            let (team, _user) = s.auth_test().await?;
            account.user_name = Some(team);
        }
        ProviderKind::JiraCloud | ProviderKind::JiraServer => {
            let p = Provider::new(account, secret)?;
            if let Provider::Jira(j) = &p
                && let Ok((kind, version)) = j.server_info().await
            {
                let is_cloud = kind.eq_ignore_ascii_case("cloud");
                if is_cloud != (account.provider == ProviderKind::JiraCloud) {
                    return Err(Error::invalid(if is_cloud {
                        "flavour_is_cloud"
                    } else {
                        "flavour_is_server"
                    }));
                }
                account.server_version = Some(version).filter(|v| !v.is_empty());
            }
            account.user_name = Some(p.whoami().await?.name);
        }
        ProviderKind::Trello => {
            let p = Provider::new(account, secret)?;
            account.user_name = Some(p.whoami().await?.name);
        }
    }
    Ok(())
}

pub async fn test(core: &Arc<Core>, id: &str) -> Result<Account> {
    let mut a = accounts::find(&core.paths.config, id)?;
    let s = secrets::get(id)?;
    test_account(&mut a, &s).await?;
    Ok(a)
}

pub fn remove_account(core: &Core, id: &str) -> Result<()> {
    let mut list = accounts::load(&core.paths.config);
    list.retain(|a| a.id != id);
    accounts::save(&core.paths.config, &list)?;
    secrets::delete(id);
    emit(core, "integrations.accounts", json!({}));
    Ok(())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AccountPatch {
    pub label: Option<String>,
    pub default_query: Option<String>,
    pub saved_queries: Option<Vec<SavedQuery>>,
    pub allowed_hosts: Option<Vec<String>>,
}

fn valid_host(h: &str) -> bool {
    !h.is_empty()
        && h.len() <= 253
        && h.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && !h.starts_with('.')
        && !h.ends_with('.')
}

pub fn update_account(core: &Core, id: &str, patch: AccountPatch) -> Result<Account> {
    let mut list = accounts::load(&core.paths.config);
    let a = list
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| Error::not_found(id))?;
    if let Some(l) = patch.label {
        a.label = l.trim().chars().take(80).collect();
    }
    if let Some(q) = patch.default_query {
        a.default_query =
            Some(q.chars().take(4000).collect()).filter(|q: &String| !q.trim().is_empty());
    }
    if let Some(s) = patch.saved_queries {
        a.saved_queries = s
            .into_iter()
            .take(50)
            .filter(|q| !q.name.trim().is_empty())
            .collect();
    }
    if let Some(h) = patch.allowed_hosts {
        let mut hosts: Vec<String> = h
            .iter()
            .map(|x| x.trim().to_ascii_lowercase())
            .filter(|x| valid_host(x))
            .collect();
        let own = a.host().to_ascii_lowercase();
        if !hosts.contains(&own) {
            hosts.insert(0, own);
        }
        a.allowed_hosts = hosts;
    }
    let out = a.clone();
    accounts::save(&core.paths.config, &list)?;
    emit(core, "integrations.accounts", json!({}));
    Ok(out)
}

pub fn open(core: &Core, account: &str) -> Result<(Account, Provider)> {
    Provider::open(&core.paths.config, account)
}

// --- links -----------------------------------------------------------------------

fn root_of(core: &Core, board: &str) -> Result<PathBuf> {
    Ok(core.board(board)?.lock().state.root.clone())
}

fn is_mirror(core: &Core, board: &str) -> bool {
    core.board(board)
        .map(|b| b.lock().state.manifest.extra.contains_key("mirror"))
        .unwrap_or(false)
}

pub fn links_dto(core: &Core, board: &str) -> Result<BTreeMap<String, RemoteInfoDto>> {
    let root = root_of(core, board)?;
    let f = links::load(&root);
    let mirror = f.mirror.is_some();
    Ok(f.links
        .iter()
        .map(|(k, l)| (k.clone(), l.dto(mirror)))
        .collect())
}

fn emit_links(core: &Core, board: &str) {
    if let Ok(l) = links_dto(core, board) {
        emit(
            core,
            "integrations.links",
            json!({ "boardId": board, "links": l }),
        );
    }
}

pub fn link_of(core: &Core, board: &str, card: &str) -> Result<Link> {
    let root = root_of(core, board)?;
    links::load(&root)
        .links
        .get(card)
        .cloned()
        .ok_or_else(|| Error::not_found("not_linked"))
}

fn save_link(core: &Core, board: &str, card: &str, link: Option<Link>) -> Result<()> {
    let root = root_of(core, board)?;
    let mut f = links::load(&root);
    match link {
        Some(l) => f.links.insert(card.to_string(), l),
        None => f.links.remove(card),
    };
    links::save(&root, &f)?;
    emit_links(core, board);
    Ok(())
}

fn find_linked(core: &Core, board: &str, key: &str) -> Option<String> {
    let root = root_of(core, board).ok()?;
    links::load(&root)
        .links
        .iter()
        .find(|(_, l)| l.key == key)
        .map(|(c, _)| c.clone())
}

/// Write card content outside the user's undo stack (mirror boards) or as a
/// normal undoable edit (local boards).
fn write_content(core: &Core, board: &str, card: &str, content: &str, label: &str) -> Result<()> {
    if is_mirror(core, board) {
        let b = core.board(board)?;
        let mut s = b.lock();
        let ro = s.state.read_only.take();
        let r = s.apply(Op::WriteCard {
            id: card.into(),
            content: content.into(),
        });
        s.state.read_only = ro;
        let applied = r?;
        core.index_changes(&mut s, &applied.changes);
        core.emit_delta(&s, &applied.changes);
        Ok(())
    } else {
        core.apply(
            board,
            Op::WriteCard {
                id: card.into(),
                content: content.into(),
            },
            label,
            None,
        )
        .map(|_| ())
    }
}

/// Replace image references (`jira-attachment:NAME` or attachment URLs) with
/// local optimized copies. Returns the new content and local file → remote name.
async fn localize_images(
    core: &Core,
    p: &Provider,
    board: &str,
    card: &str,
    content: &str,
    issue: &RemoteIssue,
) -> (String, BTreeMap<String, String>) {
    let mut out = content.to_string();
    let mut map = BTreeMap::new();
    let mut done = 0;
    for att in &issue.attachments {
        if done >= MAX_IMAGES {
            break;
        }
        let enc = att
            .filename
            .replace(' ', "%20")
            .replace('(', "%28")
            .replace(')', "%29");
        let refs = [
            format!("]({}{})", super::adf::ATTACHMENT_SCHEME, enc),
            format!("]({}{})", super::adf::ATTACHMENT_SCHEME, att.filename),
            format!("]({})", att.url),
        ];
        if !refs.iter().any(|r| out.contains(r.as_str())) || att.url.is_empty() {
            continue;
        }
        if att.size as usize > super::http::MAX_DOWNLOAD {
            continue;
        }
        let bytes = match p.download(&att.url).await {
            Ok(b) => b,
            Err(e) => {
                tracing::info!("attachment download failed for {}: {}", issue.key, e.code());
                continue;
            }
        };
        let (data, name) = match files::optimize_image(&bytes) {
            Some((d, ext)) => {
                let stem = att
                    .filename
                    .rsplit_once('.')
                    .map(|s| s.0)
                    .unwrap_or(&att.filename);
                (d, format!("{stem}.{ext}"))
            }
            None => (bytes, att.filename.clone()),
        };
        match core.add_attachment(board, card, files::Source::Bytes(&data), &name) {
            Ok(a) => {
                for r in &refs {
                    out = out.replace(r.as_str(), &format!("]({})", a.file));
                }
                map.insert(a.file.clone(), att.filename.clone());
                done += 1;
            }
            Err(e) => tracing::info!("saving attachment for {} failed: {}", issue.key, e.code()),
        }
    }
    (out, map)
}

/// Undo [`localize_images`] before pushing: local files → `jira-attachment:` refs.
pub fn remote_refs(md: &str, images: &BTreeMap<String, String>, provider: ProviderKind) -> String {
    let mut out = md.to_string();
    if provider.is_jira() {
        for (local, remote) in images {
            let enc = remote
                .replace(' ', "%20")
                .replace('(', "%28")
                .replace(')', "%29");
            out = out.replace(
                &format!("]({local})"),
                &format!("]({}{enc})", super::adf::ATTACHMENT_SCHEME),
            );
        }
    }
    out
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReq {
    pub board: String,
    pub parent: Parent,
    pub before: Option<String>,
    pub account: String,
    pub key: String,
    #[serde(default)]
    pub force: bool,
}

/// Create a linked copy of a remote issue (drag from the panel / a mirror).
pub async fn link_issue(core: &Arc<Core>, req: LinkReq) -> Result<String> {
    if is_mirror(core, &req.board) {
        return Err(Error::ReadOnly("mirror".into()));
    }
    if !req.force
        && let Some(existing) = find_linked(core, &req.board, &req.key)
    {
        return Err(Error::Conflict(format!("already_linked:{existing}")));
    }
    let (account, p) = open(core, &req.account)?;
    let issue = p.issue(&req.key).await?;
    create_linked(
        core, &account, &p, &req.board, req.parent, req.before, &issue,
    )
    .await
}

async fn create_linked(
    core: &Arc<Core>,
    account: &Account,
    p: &Provider,
    board: &str,
    parent: Parent,
    before: Option<String>,
    issue: &RemoteIssue,
) -> Result<String> {
    let content = links::compose(
        account.provider,
        &issue.summary,
        &issue.description_md,
        &issue.key,
        &issue.url,
    );
    let id = core.new_card_id();
    let index = before.as_ref().and_then(|b| {
        let bs = core.board(board).ok()?;
        let s = bs.lock();
        s.state
            .children_of(&parent)
            .and_then(|c| c.iter().position(|x| x == b))
    });
    core.apply(
        board,
        Op::CreateCard {
            id: id.clone(),
            parent,
            index,
            content: content.clone(),
        },
        &format!("Add {} issue", account.provider.label()),
        None,
    )?;
    let (local, images) = localize_images(core, p, board, &id, &content, issue).await;
    if local != content {
        write_content(
            core,
            board,
            &id,
            &local,
            &format!("Add {} issue", account.provider.label()),
        )?;
    }
    save_link(
        core,
        board,
        &id,
        Some(Link {
            account: account.id.clone(),
            provider: account.provider,
            key: issue.key.clone(),
            id: issue.id.clone(),
            url: issue.url.clone(),
            info: LinkInfo::from_issue(issue),
            unavailable: false,
            synced: Some(hash(&content)),
            images,
        }),
    )?;
    tracing::info!("linked {} into board {board}", issue.key);
    Ok(id)
}

/// Copy cards out of a mirror board as linked copies (refetched; falls back to the mirror text offline).
pub async fn copy_from_mirror(
    core: &Arc<Core>,
    from: &str,
    ids: &[String],
    to: &str,
    parent: Parent,
    before: Option<String>,
) -> Result<Vec<String>> {
    let root = root_of(core, from)?;
    let f = links::load(&root);
    let mut out = Vec::new();
    for id in ids {
        let Some(l) = f.links.get(id) else { continue };
        let req = LinkReq {
            board: to.into(),
            parent: parent.clone(),
            before: before.clone(),
            account: l.account.clone(),
            key: l.key.clone(),
            force: false,
        };
        match link_issue(core, req).await {
            Ok(c) => out.push(c),
            Err(Error::Conflict(m)) => return Err(Error::Conflict(m)),
            Err(_) => {
                // Offline: copy the mirror text and keep the link.
                let content = core.read_card(from, id)?;
                let cid = core.new_card_id();
                core.apply(
                    to,
                    Op::CreateCard {
                        id: cid.clone(),
                        parent: parent.clone(),
                        index: None,
                        content,
                    },
                    "Copy issue",
                    None,
                )?;
                let mut link = l.clone();
                link.synced = None;
                save_link(core, to, &cid, Some(link))?;
                out.push(cid);
            }
        }
    }
    Ok(out)
}

/// Refresh the strip info of every linked card of a board (batched per account).
pub async fn refresh_links(core: &Arc<Core>, board: &str) -> Result<usize> {
    let root = root_of(core, board)?;
    let f = links::load(&root);
    if f.mirror.is_some() || f.links.is_empty() {
        return Ok(0);
    }
    let mut by_account: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for l in f.links.values() {
        by_account
            .entry(l.account.clone())
            .or_default()
            .push(l.key.clone());
    }
    let mut fresh: BTreeMap<(String, String), RemoteIssue> = BTreeMap::new();
    let mut reached = Vec::new();
    for (acc, keys) in by_account {
        let Ok((_, p)) = open(core, &acc) else {
            continue;
        };
        if let Ok(list) = p.issues(&keys).await {
            reached.push(acc.clone());
            for i in list {
                fresh.insert((acc.clone(), i.key.clone()), i);
            }
        }
    }
    let mut f = links::load(&root);
    let mut n = 0;
    for l in f.links.values_mut() {
        if let Some(i) = fresh.get(&(l.account.clone(), l.key.clone())) {
            l.info = LinkInfo::from_issue(i);
            l.unavailable = false;
            n += 1;
        } else if reached.contains(&l.account) {
            l.unavailable = true;
        }
    }
    links::save(&root, &f)?;
    emit_links(core, board);
    Ok(n)
}

// --- mirrors ------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorDto {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(flatten)]
    pub state: MirrorState,
}

pub fn mirrors(core: &Core) -> Vec<MirrorDto> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(core.paths.mirrors()) else {
        return out;
    };
    for e in rd.flatten() {
        let path = e.path();
        let Some(found) = crate::discovery::read_marker(&path) else {
            continue;
        };
        if let Some(state) = links::load(&path).mirror {
            out.push(MirrorDto {
                id: found.id,
                name: found.name,
                path: path.to_string_lossy().into_owned(),
                state,
            });
        }
    }
    out.sort_by_key(|m| m.name.to_lowercase());
    out
}

fn mirror_state(core: &Core, board: &str) -> Result<MirrorState> {
    links::load(&root_of(core, board)?)
        .mirror
        .ok_or_else(|| Error::invalid("not_a_mirror"))
}

async fn fetch_remote(
    p: &Provider,
    source: &MirrorSource,
) -> Result<(RemoteBoard, Vec<RemoteIssue>)> {
    match source {
        MirrorSource::Board { id } | MirrorSource::Trello { id } => p.board(id).await,
        MirrorSource::Query { query } => {
            let mut issues = Vec::new();
            let mut next: Option<String> = None;
            loop {
                let page = p.search(query, next.as_deref()).await?;
                issues.extend(page.issues);
                match page.next {
                    Some(n) if issues.len() < 1000 => next = Some(n),
                    _ => break,
                }
            }
            Ok((RemoteBoard::default(), issues))
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorReq {
    pub account: String,
    pub source: MirrorSource,
    pub name: String,
    #[serde(default)]
    pub watch: bool,
}

pub async fn create_mirror(core: &Arc<Core>, req: MirrorReq) -> Result<String> {
    ensure_allowed(core, Direction::Pull)?;
    let (account, p) = open(core, &req.account)?;
    let remote = fetch_remote(&p, &req.source).await?;
    let id = core.new_board_id();
    let path = core.paths.mirrors().join(&id);
    let name = if req.name.trim().is_empty() {
        remote.0.name.clone()
    } else {
        req.name.trim().to_string()
    };
    let name = if name.is_empty() {
        account.label.clone()
    } else {
        name
    };
    {
        let mut s = BoardStore::create(&path, id.clone(), name, BoardKind::Kanban)?;
        s.state.manifest.extra.insert(
            "mirror".into(),
            json!({ "provider": account.provider.service() }),
        );
        s.save_manifest()?;
    }
    let state = MirrorState {
        account: account.id.clone(),
        provider: account.provider,
        source: req.source,
        watch: req.watch,
        last_sync: None,
        last_error: None,
        columns: BTreeMap::new(),
        issues: BTreeMap::new(),
        hashes: BTreeMap::new(),
    };
    links::save(
        &path,
        &links::RemoteFile {
            schema: 1,
            links: BTreeMap::new(),
            mirror: Some(state),
        },
    )?;
    core.open_board(&path)?;
    apply_remote(core, &id, &p, remote).await?;
    emit(core, "integrations.mirrors", json!({}));
    Ok(id)
}

async fn apply_remote(
    core: &Arc<Core>,
    board: &str,
    p: &Provider,
    remote: (RemoteBoard, Vec<RemoteIssue>),
) -> Result<()> {
    let mut state = mirror_state(core, board)?;
    let d = mirror::desired(state.provider, &state.source, &remote.0, &remote.1);
    let written = mirror::apply(core, board, &mut state, &d, &remote.1)?;
    for (card, key) in written {
        let Some(issue) = remote.1.iter().find(|i| i.key == key) else {
            continue;
        };
        if issue.attachments.is_empty() {
            continue;
        }
        let content = core.read_card(board, &card)?;
        let (local, _) = localize_images(core, p, board, &card, &content, issue).await;
        if local != content {
            write_content(core, board, &card, &local, "sync")?;
        }
    }
    emit_links(core, board);
    emit(core, "integrations.mirrors", json!({}));
    Ok(())
}

/// Pull a mirror without confirmation (watch loop / after a transition).
pub async fn sync_mirror(core: &Arc<Core>, board: &str) -> Result<()> {
    ensure_allowed(core, Direction::Pull)?;
    if core.board(board).is_err() {
        core.open_board_by_id(board)?;
    }
    let state = mirror_state(core, board)?;
    let (_, p) = open(core, &state.account)?;
    let remote = match fetch_remote(&p, &state.source).await {
        Ok(r) => r,
        Err(e) => {
            let root = root_of(core, board)?;
            let mut f = links::load(&root);
            if let Some(m) = f.mirror.as_mut() {
                m.last_error = Some(e.to_string());
            }
            let _ = links::save(&root, &f);
            return Err(e);
        }
    };
    apply_remote(core, board, &p, remote).await
}

pub fn set_watch(core: &Arc<Core>, board: &str, watch: bool) -> Result<()> {
    if core.board(board).is_err() {
        core.open_board_by_id(board)?;
    }
    let root = root_of(core, board)?;
    let mut f = links::load(&root);
    let m = f
        .mirror
        .as_mut()
        .ok_or_else(|| Error::invalid("not_a_mirror"))?;
    m.watch = watch;
    links::save(&root, &f)?;
    emit(core, "integrations.mirrors", json!({}));
    Ok(())
}

pub fn remove_mirror(core: &Core, board: &str) -> Result<()> {
    let path = mirrors(core)
        .into_iter()
        .find(|m| m.id == board)
        .map(|m| PathBuf::from(m.path))
        .ok_or_else(|| Error::not_found(board))?;
    if !path.starts_with(core.paths.mirrors()) {
        return Err(Error::invalid("not_a_mirror"));
    }
    core.close_board(board);
    std::fs::remove_dir_all(&path).map_err(|e| Error::io(&path, e))?;
    core.update_registry(|r| r.boards.retain(|b| b.id != board));
    emit(core, "integrations.mirrors", json!({}));
    Ok(())
}

pub fn notes_get(core: &Core, board: &str, card: &str) -> Result<String> {
    let p = links::notes_path(&root_of(core, board)?, card)?;
    Ok(std::fs::read_to_string(p).unwrap_or_default())
}

pub fn notes_set(core: &Core, board: &str, card: &str, text: &str) -> Result<()> {
    if text.len() > 1_000_000 {
        return Err(Error::invalid("too_large"));
    }
    let p = links::notes_path(&root_of(core, board)?, card)?;
    if text.is_empty() {
        let _ = std::fs::remove_file(p);
        return Ok(());
    }
    crate::fsutil::atomic_write(&p, text.as_bytes())
}

// --- WriteGate: prepare / commit ---------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PrepareReq {
    Push {
        board: String,
        card: String,
    },
    Pull {
        board: String,
        card: Option<String>,
    },
    Comment {
        board: Option<String>,
        card: Option<String>,
        account: Option<String>,
        key: Option<String>,
        body: String,
    },
    Transition {
        board: Option<String>,
        card: Option<String>,
        account: Option<String>,
        key: Option<String>,
        id: String,
        to: String,
    },
    Assign {
        board: Option<String>,
        card: Option<String>,
        account: Option<String>,
        key: Option<String>,
        user: Option<RemoteUser>,
    },
    Create {
        board: String,
        card: String,
        account: String,
        project: String,
        issue_type: Option<String>,
        container: Option<String>,
    },
    SlackPost {
        account: String,
        channel: String,
        channel_name: String,
        text: String,
    },
}

/// Resolve the remote target: (account, key, local ctx, before status/assignee).
fn resolve(
    core: &Core,
    board: &Option<String>,
    card: &Option<String>,
    account: &Option<String>,
    key: &Option<String>,
) -> Result<(String, String, Option<(String, String)>, Option<Link>)> {
    if let (Some(b), Some(c)) = (board, card) {
        let l = link_of(core, b, c)?;
        return Ok((
            l.account.clone(),
            l.key.clone(),
            Some((b.clone(), c.clone())),
            Some(l),
        ));
    }
    match (account, key) {
        (Some(a), Some(k)) => Ok((a.clone(), k.clone(), None, None)),
        _ => Err(Error::invalid("missing_target")),
    }
}

fn target_label(key: &str, summary: &str) -> String {
    if summary.is_empty() {
        key.to_string()
    } else {
        format!("{key} “{summary}”")
    }
}

pub async fn prepare(core: &Arc<Core>, req: PrepareReq) -> Result<Prepared> {
    let draft = match req {
        PrepareReq::Push { board, card } => {
            ensure_allowed(core, Direction::Push)?;
            if is_mirror(core, &board) {
                return Err(Error::ReadOnly("mirror".into()));
            }
            let link = link_of(core, &board, &card)?;
            let (account, p) = open(core, &link.account)?;
            let remote = p.issue(&link.key).await?;
            let (title, body) = links::split(&core.read_card(&board, &card)?, &link.key);
            let body = remote_refs(&body, &link.images, account.provider);
            let changes = gate::diff(&[
                (
                    "Summary",
                    Some(remote.summary.as_str()),
                    Some(title.as_str()),
                ),
                (
                    "Description",
                    Some(remote.description_md.as_str()),
                    Some(body.as_str()),
                ),
            ]);
            let summary = changes
                .iter()
                .any(|c| c.field == "Summary")
                .then(|| title.clone());
            let md = changes
                .iter()
                .any(|c| c.field == "Description")
                .then(|| body.clone());
            Draft {
                direction: Direction::Push,
                service: account.provider.service().into(),
                site: account.host(),
                target: target_label(&link.key, &remote.summary),
                changes,
                preview: None,
                action: Action::PushCard {
                    account: account.id,
                    key: link.key,
                    summary,
                    md,
                },
            }
        }
        PrepareReq::Pull { board, card } => {
            ensure_allowed(core, Direction::Pull)?;
            if let Ok(state) = mirror_state(core, &board) {
                let (account, p) = open(core, &state.account)?;
                let remote = fetch_remote(&p, &state.source).await?;
                let d = mirror::desired(state.provider, &state.source, &remote.0, &remote.1);
                let b = core.board(&board)?;
                let (plan, name) = {
                    let s = b.lock();
                    let lane_name = |c: &str| {
                        s.state
                            .lane_of(c)
                            .and_then(|l| s.state.lane(&l).map(|x| x.name.clone()))
                    };
                    (
                        mirror::plan(&state, &lane_name, &d),
                        s.state.manifest.name.clone(),
                    )
                };
                Draft {
                    direction: Direction::Pull,
                    service: account.provider.service().into(),
                    site: account.host(),
                    target: name,
                    changes: mirror::rows(&plan, &d),
                    preview: None,
                    action: Action::PullMirror {
                        board,
                        state: Box::new(state),
                        remote: Box::new(remote),
                    },
                }
            } else {
                let card = card.ok_or_else(|| Error::invalid("missing_card"))?;
                let link = link_of(core, &board, &card)?;
                let (account, p) = open(core, &link.account)?;
                let remote = p.issue(&link.key).await?;
                let content = links::compose(
                    account.provider,
                    &remote.summary,
                    &remote.description_md,
                    &remote.key,
                    &remote.url,
                );
                let current = core.read_card(&board, &card)?;
                let (title, body) = links::split(&current, &link.key);
                let local_body = remote_refs(&body, &link.images, account.provider);
                let changes = if link.synced.as_deref() == Some(hash(&content).as_str()) {
                    vec![]
                } else {
                    gate::diff(&[
                        ("Title", Some(title.as_str()), Some(remote.summary.as_str())),
                        (
                            "Description",
                            Some(local_body.as_str()),
                            Some(remote.description_md.as_str()),
                        ),
                    ])
                };
                Draft {
                    direction: Direction::Pull,
                    service: account.provider.service().into(),
                    site: account.host(),
                    target: if title.is_empty() {
                        link.key.clone()
                    } else {
                        title
                    },
                    changes,
                    preview: None,
                    action: Action::PullCard {
                        board,
                        card,
                        content,
                        issue: Box::new(remote),
                    },
                }
            }
        }
        PrepareReq::Comment {
            board,
            card,
            account,
            key,
            body,
        } => {
            ensure_allowed(core, Direction::Push)?;
            if body.trim().is_empty() || body.len() > 32_000 {
                return Err(Error::invalid("invalid_comment"));
            }
            let (acc, key, ctx, _) = resolve(core, &board, &card, &account, &key)?;
            let a = accounts::find(&core.paths.config, &acc)?;
            Draft {
                direction: Direction::Push,
                service: a.provider.service().into(),
                site: a.host(),
                target: key.clone(),
                changes: vec![ChangeRow {
                    field: "Comment".into(),
                    before: None,
                    after: Some(body.chars().take(120).collect()),
                }],
                preview: Some(body.clone()),
                action: Action::Comment {
                    account: acc,
                    key,
                    md: body,
                    ctx,
                },
            }
        }
        PrepareReq::Transition {
            board,
            card,
            account,
            key,
            id,
            to,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (acc, key, ctx, link) = resolve(core, &board, &card, &account, &key)?;
            let a = accounts::find(&core.paths.config, &acc)?;
            let before = link.and_then(|l| l.info.status);
            Draft {
                direction: Direction::Push,
                service: a.provider.service().into(),
                site: a.host(),
                target: key.clone(),
                changes: vec![ChangeRow {
                    field: "Status".into(),
                    before,
                    after: Some(to),
                }],
                preview: None,
                action: Action::Transition {
                    account: acc,
                    key,
                    id,
                    ctx,
                },
            }
        }
        PrepareReq::Assign {
            board,
            card,
            account,
            key,
            user,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (acc, key, ctx, link) = resolve(core, &board, &card, &account, &key)?;
            let a = accounts::find(&core.paths.config, &acc)?;
            let before = link.and_then(|l| l.info.assignee.map(|u| u.name));
            Draft {
                direction: Direction::Push,
                service: a.provider.service().into(),
                site: a.host(),
                target: key.clone(),
                changes: vec![ChangeRow {
                    field: "Assignee".into(),
                    before,
                    after: user.as_ref().map(|u| u.name.clone()),
                }],
                preview: None,
                action: Action::Assign {
                    account: acc,
                    key,
                    user,
                    ctx,
                },
            }
        }
        PrepareReq::Create {
            board,
            card,
            account,
            project,
            issue_type,
            container,
        } => {
            ensure_allowed(core, Direction::Push)?;
            if find_link_exists(core, &board, &card) {
                return Err(Error::Conflict("already_linked".into()));
            }
            let a = accounts::find(&core.paths.config, &account)?;
            let content = core.read_card(&board, &card)?;
            let (title, body) = links::split(&content, "\u{0}");
            let title = if title.is_empty() {
                content
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            } else {
                title
            };
            if title.is_empty() {
                return Err(Error::invalid("missing_title"));
            }
            let mut changes = vec![
                ChangeRow {
                    field: "Project".into(),
                    before: None,
                    after: Some(project.clone()),
                },
                ChangeRow {
                    field: "Summary".into(),
                    before: None,
                    after: Some(title.clone()),
                },
            ];
            if let Some(t) = &issue_type {
                changes.insert(
                    1,
                    ChangeRow {
                        field: "Type".into(),
                        before: None,
                        after: Some(t.clone()),
                    },
                );
            }
            if !body.is_empty() {
                changes.push(ChangeRow {
                    field: "Description".into(),
                    before: None,
                    after: Some(body.chars().take(400).collect()),
                });
            }
            Draft {
                direction: Direction::Push,
                service: a.provider.service().into(),
                site: a.host(),
                target: project.clone(),
                changes,
                preview: None,
                action: Action::Create {
                    account,
                    board,
                    card,
                    req: CreateIssue {
                        project,
                        issue_type,
                        summary: title,
                        description_md: body,
                        container,
                    },
                },
            }
        }
        PrepareReq::SlackPost {
            account,
            channel,
            channel_name,
            text,
        } => {
            ensure_allowed(core, Direction::Push)?;
            super::slack::valid_channel(&channel)?;
            if text.trim().is_empty() {
                return Err(Error::invalid("invalid_message"));
            }
            Draft {
                direction: Direction::Push,
                service: "slack".into(),
                site: "slack.com".into(),
                target: format!("#{}", channel_name.trim_start_matches('#')),
                changes: vec![ChangeRow {
                    field: "Message".into(),
                    before: None,
                    after: Some(text.chars().take(120).collect()),
                }],
                preview: Some(text.clone()),
                action: Action::SlackPost {
                    account,
                    channel,
                    text,
                },
            }
        }
    };
    Ok(gate::prepare(draft))
}

fn find_link_exists(core: &Core, board: &str, card: &str) -> bool {
    link_of(core, board, card).is_ok()
}

fn journal_write(
    core: &Core,
    ctx: &Option<(String, String)>,
    provider: ProviderKind,
    key: &str,
    label: &str,
    fields: Value,
) {
    let Some((board, card)) = ctx else { return };
    core.journal_entry(
        board,
        JournalEntry {
            ts: history::now(),
            board: board.clone(),
            kind: "externalWrite".into(),
            origin: Origin::You,
            label: label.into(),
            ids: vec![card.clone()],
            details: json!({ "service": provider.service(), "key": key, "fields": fields }),
            before: None,
            after: None,
        },
    );
}

async fn after_remote_change(core: &Arc<Core>, ctx: &Option<(String, String)>) {
    let Some((board, _)) = ctx else { return };
    if is_mirror(core, board) {
        let _ = sync_mirror(core, board).await;
    } else {
        let _ = refresh_links(core, board).await;
    }
}

pub async fn commit(core: &Arc<Core>, token: &str) -> Result<Value> {
    let action = gate::take(token)?;
    match action {
        Action::PushCard {
            account,
            key,
            summary,
            md,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (a, p) = open(core, &account)?;
            p.update(&key, summary.as_deref(), md.as_deref()).await?;
            tracing::info!("pushed {key}");
            Ok(json!({ "key": key, "provider": a.provider.service() }))
        }
        Action::PullCard {
            board,
            card,
            content,
            issue,
        } => {
            ensure_allowed(core, Direction::Pull)?;
            let mut link = link_of(core, &board, &card)?;
            let (_, p) = open(core, &link.account)?;
            let label = format!("Pulled from {}", link.provider.label());
            write_content(core, &board, &card, &content, &label)?;
            let (local, images) = localize_images(core, &p, &board, &card, &content, &issue).await;
            if local != content {
                write_content(core, &board, &card, &local, &label)?;
            }
            link.info = LinkInfo::from_issue(&issue);
            link.synced = Some(hash(&content));
            link.images = images;
            link.unavailable = false;
            save_link(core, &board, &card, Some(link))?;
            Ok(json!({ "card": card }))
        }
        Action::PullMirror {
            board,
            state,
            remote,
        } => {
            ensure_allowed(core, Direction::Pull)?;
            let (_, p) = open(core, &state.account)?;
            apply_remote(core, &board, &p, *remote).await?;
            Ok(json!({ "board": board }))
        }
        Action::Comment {
            account,
            key,
            md,
            ctx,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (a, p) = open(core, &account)?;
            p.add_comment(&key, &md).await?;
            journal_write(
                core,
                &ctx,
                a.provider,
                &key,
                &format!("Commented on {key}"),
                json!(["comment"]),
            );
            Ok(json!({ "key": key }))
        }
        Action::Transition {
            account,
            key,
            id,
            ctx,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (a, p) = open(core, &account)?;
            p.transition(&key, &id).await?;
            journal_write(
                core,
                &ctx,
                a.provider,
                &key,
                &format!("Transitioned {key}"),
                json!(["status"]),
            );
            after_remote_change(core, &ctx).await;
            Ok(json!({ "key": key }))
        }
        Action::Assign {
            account,
            key,
            user,
            ctx,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (a, p) = open(core, &account)?;
            p.assign(&key, user.as_ref()).await?;
            journal_write(
                core,
                &ctx,
                a.provider,
                &key,
                &format!("Assigned {key}"),
                json!(["assignee"]),
            );
            after_remote_change(core, &ctx).await;
            Ok(json!({ "key": key }))
        }
        Action::Create {
            account,
            board,
            card,
            req,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (a, p) = open(core, &account)?;
            let key = p.create(&req).await?;
            let issue = p.issue(&key).await.unwrap_or_else(|_| RemoteIssue {
                key: key.clone(),
                summary: req.summary.clone(),
                ..Default::default()
            });
            let content = core.read_card(&board, &card)?;
            let line = links::footer_line(a.provider, &issue.key, &issue.url);
            let new_content = format!("{}\n\n{line}\n", content.trim_end());
            write_content(
                core,
                &board,
                &card,
                &new_content,
                &format!("Linked to {key}"),
            )?;
            let composed = links::compose(
                a.provider,
                &issue.summary,
                &issue.description_md,
                &issue.key,
                &issue.url,
            );
            save_link(
                core,
                &board,
                &card,
                Some(Link {
                    account: a.id.clone(),
                    provider: a.provider,
                    key: issue.key.clone(),
                    id: issue.id.clone(),
                    url: issue.url.clone(),
                    info: LinkInfo::from_issue(&issue),
                    unavailable: false,
                    synced: Some(hash(&composed)),
                    images: BTreeMap::new(),
                }),
            )?;
            journal_write(
                core,
                &Some((board, card)),
                a.provider,
                &key,
                &format!("Created {key}"),
                json!(["summary", "description"]),
            );
            tracing::info!("created {key}");
            Ok(json!({ "key": key, "url": issue.url }))
        }
        Action::SlackPost {
            account,
            channel,
            text,
        } => {
            ensure_allowed(core, Direction::Push)?;
            let (_, s) = provider::slack(&core.paths.config, &account)?;
            s.post(&channel, &text).await?;
            tracing::info!("posted a Slack message");
            Ok(json!({ "ok": true }))
        }
    }
}

pub fn unlink(core: &Core, board: &str, card: &str) -> Result<()> {
    if is_mirror(core, board) {
        return Err(Error::ReadOnly("mirror".into()));
    }
    save_link(core, board, card, None)
}

// --- watch loop ---------------------------------------------------------------------

/// Background loop: pulls watched mirrors and refreshes linked-card strips of
/// open boards every `integrations.watchIntervalSec` (default 60 s).
pub fn start_watcher(core: Arc<Core>) {
    std::thread::Builder::new()
        .name("lull-integrations".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                tracing::warn!("integrations watcher: no runtime");
                return;
            };
            rt.block_on(async move {
                loop {
                    let secs = core
                        .settings()
                        .get("integrations.watchIntervalSec")
                        .and_then(Value::as_u64)
                        .unwrap_or(60)
                        .clamp(30, 3600);
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    if Arc::strong_count(&core) <= 1 {
                        break;
                    }
                    tick(&core).await;
                }
            });
        })
        .ok();
}

pub async fn tick(core: &Arc<Core>) {
    if !setting_bool(core, ALLOW_PULL, true) || accounts::load(&core.paths.config).is_empty() {
        return;
    }
    let mut offline = false;
    for m in mirrors(core).into_iter().filter(|m| m.state.watch) {
        if let Err(e) = sync_mirror(core, &m.id).await {
            offline |= e.to_string().contains("remote_offline");
            tracing::info!("mirror sync failed for {}: {}", m.id, e.code());
        }
    }
    for b in core.open_board_ids() {
        let _ = refresh_links(core, &b).await;
    }
    emit(
        core,
        "integrations.status",
        json!({ "offline": offline, "at": history::now() }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AppPaths, NullSink};

    #[test]
    fn gate_toggles_and_refs() {
        let d = tempfile::tempdir().unwrap();
        let core = Core::new(AppPaths::under(d.path()), Arc::new(NullSink)).unwrap();
        assert!(
            ensure_allowed(&core, Direction::Push).is_err(),
            "push is off by default"
        );
        assert!(ensure_allowed(&core, Direction::Pull).is_ok());
        core.set_settings(json!({ ALLOW_PUSH: true, ALLOW_PULL: false }))
            .unwrap();
        assert!(ensure_allowed(&core, Direction::Push).is_ok());
        assert_eq!(
            ensure_allowed(&core, Direction::Pull)
                .unwrap_err()
                .to_string(),
            "invalid operation: pull_disabled"
        );

        let mut images = BTreeMap::new();
        images.insert(
            "cabc123.x1y2-shot.jpg".to_string(),
            "shot one.png".to_string(),
        );
        let md = "See ![s](cabc123.x1y2-shot.jpg)";
        assert_eq!(
            remote_refs(md, &images, ProviderKind::JiraCloud),
            "See ![s](jira-attachment:shot%20one.png)"
        );
        assert_eq!(remote_refs(md, &images, ProviderKind::Trello), md);
        assert!(valid_host("api.media.atlassian.com") && !valid_host("a/b") && !valid_host(".x"));
    }

    #[test]
    fn accounts_update_keeps_own_host() {
        let d = tempfile::tempdir().unwrap();
        let core = Core::new(AppPaths::under(d.path()), Arc::new(NullSink)).unwrap();
        let base = accounts::normalize_base_url("https://acme.atlassian.net", false).unwrap();
        let a = Account {
            id: "a1".into(),
            provider: ProviderKind::JiraCloud,
            label: "Acme".into(),
            base_url: accounts::base_string(&base),
            allowed_hosts: accounts::default_hosts(ProviderKind::JiraCloud, &base),
            insecure_http: false,
            default_query: None,
            saved_queries: vec![],
            ca_cert_path: None,
            client_cert_path: None,
            user_name: None,
            server_version: None,
            persisted: false,
            created: String::new(),
        };
        accounts::save(&core.paths.config, &[a]).unwrap();
        let u = update_account(
            &core,
            "a1",
            AccountPatch {
                allowed_hosts: Some(vec!["Files.Acme.io".into(), "bad host".into()]),
                saved_queries: Some(vec![
                    SavedQuery {
                        name: "Mine".into(),
                        query: "assignee = currentUser()".into(),
                    },
                    SavedQuery::default(),
                ]),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(u.allowed_hosts, vec!["acme.atlassian.net", "files.acme.io"]);
        assert_eq!(u.saved_queries.len(), 1);
        let text = std::fs::read_to_string(accounts::file(&core.paths.config)).unwrap();
        assert!(!text.contains("token"));
        remove_account(&core, "a1").unwrap();
        assert!(accounts::load(&core.paths.config).is_empty());
    }

    #[test]
    fn mirror_apply_is_idempotent() {
        let d = tempfile::tempdir().unwrap();
        let core = Core::new(AppPaths::under(d.path()), Arc::new(NullSink)).unwrap();
        let id = core.new_board_id();
        let path = core.paths.mirrors().join(&id);
        {
            let mut s =
                BoardStore::create(&path, id.clone(), "M".into(), BoardKind::Kanban).unwrap();
            s.state
                .manifest
                .extra
                .insert("mirror".into(), json!({ "provider": "jira" }));
            s.save_manifest().unwrap();
        }
        let mut state = MirrorState {
            account: "a".into(),
            provider: ProviderKind::JiraCloud,
            source: MirrorSource::Query { query: "x".into() },
            watch: true,
            last_sync: None,
            last_error: None,
            columns: BTreeMap::new(),
            issues: BTreeMap::new(),
            hashes: BTreeMap::new(),
        };
        links::save(
            &path,
            &links::RemoteFile {
                schema: 1,
                links: BTreeMap::new(),
                mirror: Some(state.clone()),
            },
        )
        .unwrap();
        let snap = core.open_board(&path).unwrap();
        assert!(
            snap.header
                .read_only
                .as_deref()
                .unwrap()
                .starts_with("mirror")
        );
        assert!(core.registry().get(&id).unwrap().mirror);
        let mk = |k: &str, st: &str, cat| RemoteIssue {
            key: k.into(),
            summary: format!("S {k}"),
            status: st.into(),
            status_category: cat,
            url: format!("https://a/browse/{k}"),
            ..Default::default()
        };
        let issues = vec![
            mk("K-1", "Open", StatusCategory::Todo),
            mk("K-2", "Done", StatusCategory::Done),
        ];
        let dsr = mirror::desired(
            state.provider,
            &state.source,
            &RemoteBoard::default(),
            &issues,
        );
        let w = mirror::apply(&core, &id, &mut state, &dsr, &issues).unwrap();
        assert_eq!(w.len(), 2);
        let w2 = mirror::apply(&core, &id, &mut state, &dsr, &issues).unwrap();
        assert!(w2.is_empty(), "second sync writes nothing");
        // User edits are refused; the issue moving is applied by the sync.
        let c1 = state.issues["K-1"].clone();
        assert!(
            core.apply(
                &id,
                Op::WriteCard {
                    id: c1.clone(),
                    content: "x".into()
                },
                "e",
                None
            )
            .is_err()
        );
        let issues = vec![mk("K-1", "Done", StatusCategory::Done)];
        let dsr = mirror::desired(
            state.provider,
            &state.source,
            &RemoteBoard::default(),
            &issues,
        );
        mirror::apply(&core, &id, &mut state, &dsr, &issues).unwrap();
        let snap = core.snapshot(&id).unwrap();
        assert_eq!(snap.lanes.len(), 1);
        assert_eq!(snap.lanes[0].name, "Done");
        assert_eq!(snap.lanes[0].order, vec![c1.clone()]);
        assert!(!state.issues.contains_key("K-2"));
        let l = links_dto(&core, &id).unwrap();
        assert_eq!(l[&c1].key, "K-1");
        assert!(l[&c1].mirror);
        notes_set(&core, &id, &c1, "private").unwrap();
        assert_eq!(notes_get(&core, &id, &c1).unwrap(), "private");
        assert_eq!(mirrors(&core).len(), 1);
        remove_mirror(&core, &id).unwrap();
        assert!(mirrors(&core).is_empty());
    }
}
