//! Mirror boards: a remote board (Jira board columns, a JQL query grouped by
//! status, or a Trello board's lists) kept as a read-only local board in
//! `<app data>/mirrors/<id>/`. The remote is the source of truth; the sync is
//! idempotent thanks to the key ↔ card and column ↔ lane maps in `remote.json`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use super::gate::ChangeRow;
use super::links::{self, Link, LinkInfo, MirrorSource, MirrorState};
use super::types::*;
use crate::app::Core;
use crate::error::Result;
use crate::fsutil::sha256_hex;
use crate::history::{self, JournalEntry, Origin};
use crate::ids::{IdKind, new_id};
use crate::model::Parent;
use crate::store::{Changes, Op};

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredCard {
    pub key: String,
    pub lane: String,
    pub title: String,
    pub content: String,
    pub hash: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Desired {
    pub lanes: Vec<String>,
    /// In rank order.
    pub cards: Vec<DesiredCard>,
}

fn category_rank(c: StatusCategory) -> u8 {
    match c {
        StatusCategory::Todo => 0,
        StatusCategory::InProgress => 1,
        StatusCategory::Done => 2,
    }
}

/// Compute the desired lanes and cards from the remote state.
pub fn desired(
    provider: ProviderKind,
    source: &MirrorSource,
    board: &RemoteBoard,
    issues: &[RemoteIssue],
) -> Desired {
    let by_columns = !matches!(source, MirrorSource::Query { .. }) && !board.columns.is_empty();
    let mut lanes: Vec<String> = Vec::new();
    let mut lane_of: Vec<(usize, String)> = Vec::new();
    if by_columns {
        for c in &board.columns {
            if !lanes.contains(&c.name) {
                lanes.push(c.name.clone());
            }
        }
        for (i, is) in issues.iter().enumerate() {
            // Issues whose status is not mapped to a column are hidden (as in Jira).
            if let Some(c) = board
                .columns
                .iter()
                .find(|c| c.statuses.contains(&is.status_id))
            {
                lane_of.push((i, c.name.clone()));
            }
        }
    } else {
        let mut statuses: Vec<(u8, usize, String)> = Vec::new();
        for (i, is) in issues.iter().enumerate() {
            let name = if is.status.is_empty() {
                "—".to_string()
            } else {
                is.status.clone()
            };
            if !statuses.iter().any(|s| s.2 == name) {
                statuses.push((category_rank(is.status_category), i, name.clone()));
            }
            lane_of.push((i, name));
        }
        statuses.sort();
        lanes = statuses.into_iter().map(|s| s.2).collect();
    }
    let cards = lane_of
        .into_iter()
        .map(|(i, lane)| {
            let is = &issues[i];
            let content =
                links::compose(provider, &is.summary, &is.description_md, &is.key, &is.url);
            DesiredCard {
                key: is.key.clone(),
                lane,
                title: is.summary.clone(),
                hash: sha256_hex(content.as_bytes())[..16].to_string(),
                content,
            }
        })
        .collect();
    Desired { lanes, cards }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub new_lanes: Vec<String>,
    pub removed_lanes: Vec<String>,
    pub created: Vec<String>,
    pub updated: Vec<String>,
    /// (key, from lane, to lane)
    pub moved: Vec<(String, String, String)>,
    pub removed: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.new_lanes.is_empty()
            && self.removed_lanes.is_empty()
            && self.created.is_empty()
            && self.updated.is_empty()
            && self.moved.is_empty()
            && self.removed.is_empty()
    }
}

/// What a sync would change. `lane_of_card` gives the current lane *name* of a card.
pub fn plan(
    state: &MirrorState,
    lane_of_card: &dyn Fn(&str) -> Option<String>,
    d: &Desired,
) -> Plan {
    let mut p = Plan::default();
    for l in &d.lanes {
        if !state.columns.contains_key(l) {
            p.new_lanes.push(l.clone());
        }
    }
    for l in state.columns.keys() {
        if !d.lanes.contains(l) {
            p.removed_lanes.push(l.clone());
        }
    }
    let wanted: BTreeSet<&str> = d.cards.iter().map(|c| c.key.as_str()).collect();
    for c in &d.cards {
        match state.issues.get(&c.key) {
            None => p.created.push(c.key.clone()),
            Some(card) => {
                if state.hashes.get(&c.key) != Some(&c.hash) {
                    p.updated.push(c.key.clone());
                }
                let from = lane_of_card(card).unwrap_or_default();
                if from != c.lane {
                    p.moved.push((c.key.clone(), from, c.lane.clone()));
                }
            }
        }
    }
    for k in state.issues.keys() {
        if !wanted.contains(k.as_str()) {
            p.removed.push(k.clone());
        }
    }
    p
}

/// Rows for the confirmation dialog.
pub fn rows(p: &Plan, d: &Desired) -> Vec<ChangeRow> {
    let title = |k: &str| {
        d.cards
            .iter()
            .find(|c| c.key == k)
            .map(|c| format!("{k} {}", c.title))
            .unwrap_or_else(|| k.to_string())
    };
    let mut out = Vec::new();
    for l in &p.new_lanes {
        out.push(ChangeRow {
            field: "Column".into(),
            before: None,
            after: Some(l.clone()),
        });
    }
    for l in &p.removed_lanes {
        out.push(ChangeRow {
            field: "Column".into(),
            before: Some(l.clone()),
            after: None,
        });
    }
    for k in &p.created {
        out.push(ChangeRow {
            field: "Issue".into(),
            before: None,
            after: Some(title(k)),
        });
    }
    for (k, from, to) in &p.moved {
        out.push(ChangeRow {
            field: format!("Status {k}"),
            before: Some(from.clone()),
            after: Some(to.clone()),
        });
    }
    for k in p
        .updated
        .iter()
        .filter(|k| !p.moved.iter().any(|m| &m.0 == *k))
    {
        out.push(ChangeRow {
            field: format!("Content {k}"),
            before: Some("…".into()),
            after: Some(title(k)),
        });
    }
    for k in &p.removed {
        out.push(ChangeRow {
            field: "Issue".into(),
            before: Some(k.clone()),
            after: None,
        });
    }
    out
}

/// Apply the desired state to the mirror board (sync engine only; bypasses
/// the read-only flag under the store lock). Returns cards whose content was
/// (re)written, for image downloads.
pub fn apply(
    core: &Core,
    board: &str,
    state: &mut MirrorState,
    d: &Desired,
    issues: &[RemoteIssue],
) -> Result<Vec<(String, String)>> {
    // Card ids are allocated before locking (id allocation checks every open board).
    let mut fresh: Vec<String> = d
        .cards
        .iter()
        .filter(|c| !state.issues.contains_key(&c.key))
        .map(|_| core.new_card_id())
        .collect();
    let b = core.board(board)?;
    let mut s = b.lock();
    let ro = s.state.read_only.take();
    let mut ch = Changes::default();
    let mut written = Vec::new();
    let mut ids_changed = BTreeSet::new();
    let res: Result<()> = (|| {
        // 1. Lanes (create / reorder).
        for (i, name) in d.lanes.iter().enumerate() {
            let existing = state
                .columns
                .get(name)
                .filter(|id| s.state.lane(id).is_some())
                .cloned();
            let id = match existing {
                Some(id) => id,
                None => {
                    let id = new_id(IdKind::Lane, |x| s.state.lane(x).is_some());
                    ch.merge(
                        s.apply(Op::CreateLane {
                            id: id.clone(),
                            name: name.clone(),
                            index: None,
                        })?
                        .changes,
                    );
                    state.columns.insert(name.clone(), id.clone());
                    id
                }
            };
            if s.state.lanes.iter().position(|l| l.id == id) != Some(i) {
                ch.merge(s.apply(Op::MoveLane { id, index: i })?.changes);
            }
        }
        // 2. Cards.
        let mut per_lane: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in &d.cards {
            let lane = state.columns[&c.lane].clone();
            let card = match state
                .issues
                .get(&c.key)
                .filter(|id| s.state.nodes.contains_key(*id))
                .cloned()
            {
                Some(id) => {
                    if state.hashes.get(&c.key) != Some(&c.hash) {
                        ch.merge(
                            s.apply(Op::WriteCard {
                                id: id.clone(),
                                content: c.content.clone(),
                            })?
                            .changes,
                        );
                        written.push((id.clone(), c.key.clone()));
                        ids_changed.insert(id.clone());
                    }
                    id
                }
                None => {
                    let id = fresh
                        .pop()
                        .unwrap_or_else(|| new_id(IdKind::Card, |x| s.state.nodes.contains_key(x)));
                    ch.merge(
                        s.apply(Op::CreateCard {
                            id: id.clone(),
                            parent: Parent::Lane(lane.clone()),
                            index: None,
                            content: c.content.clone(),
                        })?
                        .changes,
                    );
                    written.push((id.clone(), c.key.clone()));
                    ids_changed.insert(id.clone());
                    id
                }
            };
            state.issues.insert(c.key.clone(), card.clone());
            state.hashes.insert(c.key.clone(), c.hash.clone());
            per_lane.entry(lane).or_default().push(card);
        }
        // 3. Order by rank inside each lane (also moves cards across lanes).
        for (lane, want) in &per_lane {
            let cur = s
                .state
                .lane(lane)
                .map(|l| l.order.clone())
                .unwrap_or_default();
            if &cur != want {
                for id in want {
                    if s.state.lane_of(id).as_deref() != Some(lane.as_str()) {
                        ids_changed.insert(id.clone());
                    }
                }
                ch.merge(
                    s.apply(Op::Move {
                        ids: want.clone(),
                        to: Parent::Lane(lane.clone()),
                        before: None,
                    })?
                    .changes,
                );
            }
        }
        // 4. Issues no longer on the remote board.
        let wanted: BTreeSet<&str> = d.cards.iter().map(|c| c.key.as_str()).collect();
        let gone: Vec<(String, String)> = state
            .issues
            .iter()
            .filter(|(k, _)| !wanted.contains(k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let trash: Vec<String> = gone
            .iter()
            .map(|g| g.1.clone())
            .filter(|id| s.state.nodes.contains_key(id))
            .collect();
        if !trash.is_empty() {
            ch.merge(
                s.apply(Op::Trash {
                    nodes: trash.clone(),
                    lanes: vec![],
                })?
                .changes,
            );
            ids_changed.extend(trash);
        }
        for (k, _) in &gone {
            state.issues.remove(k);
            state.hashes.remove(k);
        }
        // 5. Columns removed on the remote.
        let old: Vec<(String, String)> = state
            .columns
            .iter()
            .filter(|(n, _)| !d.lanes.contains(n))
            .map(|(n, id)| (n.clone(), id.clone()))
            .collect();
        let lanes: Vec<String> = old
            .iter()
            .map(|o| o.1.clone())
            .filter(|id| s.state.lane(id).is_some())
            .collect();
        if !lanes.is_empty() {
            ch.merge(
                s.apply(Op::Trash {
                    nodes: vec![],
                    lanes,
                })?
                .changes,
            );
        }
        for (n, _) in old {
            state.columns.remove(&n);
        }
        Ok(())
    })();
    s.state.read_only = ro;
    // Links (for the strip) are rebuilt from the issues.
    let root = s.state.root.clone();
    let mut file = links::load(&root);
    file.links.clear();
    for is in issues {
        if let Some(card) = state.issues.get(&is.key) {
            file.links.insert(
                card.clone(),
                Link {
                    account: state.account.clone(),
                    provider: state.provider,
                    key: is.key.clone(),
                    id: is.id.clone(),
                    url: is.url.clone(),
                    info: LinkInfo::from_issue(is),
                    unavailable: false,
                    synced: None,
                    images: BTreeMap::new(),
                },
            );
        }
    }
    state.last_sync = Some(history::now());
    state.last_error = res.as_ref().err().map(|e| e.code().to_string());
    file.mirror = Some(state.clone());
    links::save(&root, &file)?;
    core.index_changes(&mut s, &ch);
    core.emit_delta(&s, &ch);
    drop(s);
    if !ids_changed.is_empty() {
        core.journal_entry(
            board,
            JournalEntry {
                ts: history::now(),
                board: board.to_string(),
                kind: "remoteSync".into(),
                origin: Origin::Remote,
                label: format!("Synced from {}", state.provider.label()),
                ids: ids_changed.into_iter().collect(),
                details: json!({ "service": state.provider.service() }),
                before: None,
                after: None,
            },
        );
    }
    res.map(|_| written)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(key: &str, status: &str, sid: &str, cat: StatusCategory) -> RemoteIssue {
        RemoteIssue {
            key: key.into(),
            id: key.into(),
            url: format!("https://a/browse/{key}"),
            summary: format!("Sum {key}"),
            status: status.into(),
            status_id: sid.into(),
            status_category: cat,
            ..Default::default()
        }
    }

    fn board() -> RemoteBoard {
        RemoteBoard {
            id: "1".into(),
            name: "B".into(),
            kind: "kanban".into(),
            columns: vec![
                RemoteColumn {
                    name: "To Do".into(),
                    statuses: vec!["1".into()],
                },
                RemoteColumn {
                    name: "Doing".into(),
                    statuses: vec!["3".into()],
                },
                RemoteColumn {
                    name: "Done".into(),
                    statuses: vec!["10".into(), "11".into()],
                },
            ],
            ..Default::default()
        }
    }

    fn state() -> MirrorState {
        MirrorState {
            account: "a".into(),
            provider: ProviderKind::JiraCloud,
            source: MirrorSource::Board { id: "1".into() },
            watch: false,
            last_sync: None,
            last_error: None,
            columns: BTreeMap::new(),
            issues: BTreeMap::new(),
            hashes: BTreeMap::new(),
        }
    }

    #[test]
    fn lanes_from_board_columns() {
        let is = vec![
            issue("K-1", "Done", "11", StatusCategory::Done),
            issue("K-2", "Hidden", "99", StatusCategory::Todo),
            issue("K-3", "Doing", "3", StatusCategory::InProgress),
        ];
        let d = desired(
            ProviderKind::JiraCloud,
            &MirrorSource::Board { id: "1".into() },
            &board(),
            &is,
        );
        assert_eq!(d.lanes, vec!["To Do", "Doing", "Done"]);
        let lanes: Vec<(&str, &str)> = d
            .cards
            .iter()
            .map(|c| (c.key.as_str(), c.lane.as_str()))
            .collect();
        assert_eq!(lanes, vec![("K-1", "Done"), ("K-3", "Doing")]);
        assert!(d.cards[0].content.starts_with("# Sum K-1\n"));
    }

    #[test]
    fn lanes_by_status_for_queries() {
        let is = vec![
            issue("K-1", "Done", "11", StatusCategory::Done),
            issue("K-2", "Open", "1", StatusCategory::Todo),
            issue("K-3", "Review", "5", StatusCategory::InProgress),
        ];
        let d = desired(
            ProviderKind::JiraCloud,
            &MirrorSource::Query { query: "x".into() },
            &board(),
            &is,
        );
        assert_eq!(d.lanes, vec!["Open", "Review", "Done"]);
    }

    #[test]
    fn plan_and_rows() {
        let is = vec![
            issue("K-1", "Done", "11", StatusCategory::Done),
            issue("K-3", "Doing", "3", StatusCategory::InProgress),
        ];
        let d = desired(
            ProviderKind::JiraCloud,
            &MirrorSource::Board { id: "1".into() },
            &board(),
            &is,
        );
        let mut st = state();
        st.columns.insert("To Do".into(), "k1".into());
        st.columns.insert("Old".into(), "k9".into());
        st.issues.insert("K-1".into(), "c1".into());
        st.hashes.insert("K-1".into(), d.cards[0].hash.clone());
        st.issues.insert("K-9".into(), "c9".into());
        let p = plan(&st, &|c| (c == "c1").then(|| "To Do".to_string()), &d);
        assert_eq!(p.new_lanes, vec!["Doing", "Done"]);
        assert_eq!(p.removed_lanes, vec!["Old"]);
        assert_eq!(p.created, vec!["K-3"]);
        assert!(p.updated.is_empty());
        assert_eq!(p.moved, vec![("K-1".into(), "To Do".into(), "Done".into())]);
        assert_eq!(p.removed, vec!["K-9"]);
        let r = rows(&p, &d);
        assert!(
            r.iter()
                .any(|r| r.field == "Status K-1" && r.after.as_deref() == Some("Done"))
        );
        assert!(
            r.iter()
                .any(|r| r.field == "Issue" && r.after.as_deref() == Some("K-3 Sum K-3"))
        );
        // A second identical plan after applying would be empty.
        let mut st2 = state();
        for l in &d.lanes {
            st2.columns.insert(l.clone(), l.clone());
        }
        for c in &d.cards {
            st2.issues.insert(c.key.clone(), c.key.clone());
            st2.hashes.insert(c.key.clone(), c.hash.clone());
        }
        let lane_of = |card: &str| {
            d.cards
                .iter()
                .find(|c| c.key == card)
                .map(|c| c.lane.clone())
        };
        assert!(plan(&st2, &lane_of, &d).is_empty());
    }
}
