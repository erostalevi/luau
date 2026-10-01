//! Activity facts: a pure reduction of journal entries (plus the current board
//! state for pending/overdue cards) into per-board, per-category item lists.
//!
//! Repeated moves collapse to the net move; edit sessions are merged (edits
//! closer than [`EDIT_SESSION_GAP_MIN`] minutes count as one session).

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::stage::{Stage, from_status_category, infer_stage};
use crate::history::{JournalEntry, Origin};

pub const EDIT_SESSION_GAP_MIN: i64 = 30;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lane: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// RFC 3339 time of the (last) event that put the item in this list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovedItem {
    pub item: Item,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditedItem {
    pub item: Item,
    /// Merged editing sessions.
    pub edits: u32,
    /// Net characters added (negative = removed).
    pub chars: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneStage {
    pub name: String,
    pub stage: Stage,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardFacts {
    pub id: String,
    pub name: String,
    /// Current lanes with their inferred stage (context for the AI).
    #[serde(default)]
    pub lanes: Vec<LaneStage>,
    pub created: Vec<Item>,
    pub completed: Vec<Item>,
    pub started: Vec<Item>,
    pub moved: Vec<MovedItem>,
    pub edited: Vec<EditedItem>,
    pub deleted: Vec<Item>,
    pub archived: Vec<Item>,
    pub external: Vec<Item>,
    pub pending: Vec<Item>,
    pub overdue: Vec<Item>,
}

impl BoardFacts {
    pub fn has_activity(&self) -> bool {
        !(self.created.is_empty()
            && self.completed.is_empty()
            && self.started.is_empty()
            && self.moved.is_empty()
            && self.edited.is_empty()
            && self.deleted.is_empty()
            && self.archived.is_empty()
            && self.external.is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub boards: usize,
    pub created: usize,
    pub completed: usize,
    pub started: usize,
    pub moved: usize,
    pub edited: usize,
    pub deleted: usize,
    pub archived: usize,
    pub external: usize,
    pub pending: usize,
    pub overdue: usize,
    /// Journal entries considered.
    pub events: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Period {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityFacts {
    pub period: Period,
    pub boards: Vec<BoardFacts>,
    pub totals: Totals,
}

// --- input: current board state ---------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct CardView {
    pub id: String,
    pub title: String,
    pub lane: Option<String>,
    pub due: Option<String>,
    pub priority: Option<String>,
    pub tags: Vec<String>,
    pub archived: bool,
    /// Explicit status category (Jira `statusCategory` or a `status:` footer); wins over the lane name.
    pub status: Option<Stage>,
}

#[derive(Debug, Clone, Default)]
pub struct BoardView {
    pub id: String,
    pub name: String,
    pub kanban: bool,
    /// Lane names in board order.
    pub lanes: Vec<String>,
    pub cards: Vec<CardView>,
}

impl BoardView {
    fn card(&self, id: &str) -> Option<&CardView> {
        self.cards.iter().find(|c| c.id == id)
    }
}

// --- extraction -----------------------------------------------------------------

fn s(v: &Value) -> Option<String> {
    v.as_str().map(str::to_string).filter(|x| !x.is_empty())
}

/// `details.<side>.items[]` entry for `id`.
fn side_item<'a>(details: &'a Value, side: &str, id: &str) -> Option<&'a Value> {
    details
        .get(side)?
        .get("items")?
        .as_array()?
        .iter()
        .find(|i| i.get("id").and_then(Value::as_str) == Some(id))
}

fn side_ids(details: &Value, side: &str) -> Vec<String> {
    details
        .get(side)
        .and_then(|x| x.get("items"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|i| i.get("id").and_then(Value::as_str).map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Best-known title of `id` in this entry.
fn entry_title(e: &JournalEntry, id: &str) -> Option<String> {
    let d = &e.details;
    for side in ["after", "before"] {
        if let Some(t) = side_item(d, side, id)
            .and_then(|i| i.get("title"))
            .and_then(s)
        {
            return Some(t);
        }
    }
    if let Some(t) = d.get("title").and_then(s) {
        return Some(t);
    }
    if let Some(titles) = d.get("titles").and_then(Value::as_array)
        && titles.len() == e.ids.len()
        && let Some(i) = e.ids.iter().position(|x| x == id)
    {
        return titles.get(i).and_then(s);
    }
    None
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

pub fn priority_rank(p: Option<&str>) -> u8 {
    match p.map(|x| x.trim().to_lowercase()).as_deref() {
        Some("urgent" | "highest" | "blocker" | "critical") => 0,
        Some("high") => 1,
        Some("medium" | "normal") => 2,
        Some("low" | "lowest") => 3,
        _ => 4,
    }
}

#[derive(Default)]
struct CardAcc {
    title: Option<String>,
    created: Option<(String, Option<String>)>, // (ts, lane)
    first_from: Option<Option<String>>,
    last_to: Option<(String, Option<String>)>, // (ts, lane)
    edits: Vec<(DateTime<Utc>, i64, String)>,
    deleted: Option<String>,
    archived: Option<(bool, String)>,
    external: Option<String>,
    from_other_board: bool,
}

struct Ctx<'a> {
    view: Option<&'a BoardView>,
}

impl Ctx<'_> {
    fn item(&self, id: &str, acc: &CardAcc, lane: Option<String>, at: Option<String>) -> Item {
        let cv = self.view.and_then(|v| v.card(id));
        Item {
            id: id.to_string(),
            title: cv
                .map(|c| c.title.clone())
                .filter(|t| !t.is_empty())
                .or_else(|| acc.title.clone())
                .unwrap_or_else(|| id.to_string()),
            lane: lane.or_else(|| cv.and_then(|c| c.lane.clone())),
            due: cv.and_then(|c| c.due.clone()),
            priority: cv.and_then(|c| c.priority.clone()),
            tags: cv.map(|c| c.tags.clone()).unwrap_or_default(),
            at,
        }
    }

    /// Stage of a lane the card ended in; an explicit card status wins when it is the card's current lane.
    fn stage_of(&self, id: &str, lane: Option<&str>) -> Stage {
        if let Some(c) = self.view.and_then(|v| v.card(id))
            && let Some(st) = c.status
            && c.lane.as_deref() == lane
        {
            return st;
        }
        lane.map(infer_stage).unwrap_or(Stage::Other)
    }
}

/// Reduce `entries` (any order, any boards) into facts. `boards` gives the
/// board order and which boards to include even without activity; `views`
/// holds the current state of each board. `today` decides "overdue".
pub fn extract(
    entries: &[JournalEntry],
    boards: &[String],
    views: &HashMap<String, BoardView>,
    period: Period,
    today: NaiveDate,
    include_idle: bool,
) -> ActivityFacts {
    let mut by_board: BTreeMap<String, Vec<&JournalEntry>> = BTreeMap::new();
    for e in entries {
        by_board.entry(e.board.clone()).or_default().push(e);
    }
    let mut order: Vec<String> = boards.to_vec();
    for b in by_board.keys() {
        if !order.contains(b) {
            order.push(b.clone());
        }
    }
    let mut out = ActivityFacts {
        period,
        ..Default::default()
    };
    out.totals.events = entries.len();
    for bid in order {
        let view = views.get(&bid);
        let mut list = by_board.remove(&bid).unwrap_or_default();
        list.sort_by(|a, b| a.ts.cmp(&b.ts));
        let bf = board_facts(&bid, view, &list, today);
        if bf.has_activity() || include_idle {
            out.boards.push(bf);
        }
    }
    let t = &mut out.totals;
    for b in &out.boards {
        t.created += b.created.len();
        t.completed += b.completed.len();
        t.started += b.started.len();
        t.moved += b.moved.len();
        t.edited += b.edited.len();
        t.deleted += b.deleted.len();
        t.archived += b.archived.len();
        t.external += b.external.len();
        t.pending += b.pending.len();
        t.overdue += b.overdue.len();
    }
    t.boards = out.boards.len();
    out
}

fn board_facts(
    bid: &str,
    view: Option<&BoardView>,
    list: &[&JournalEntry],
    today: NaiveDate,
) -> BoardFacts {
    let ctx = Ctx { view };
    let mut accs: HashMap<String, CardAcc> = HashMap::new();
    let mut seen_order: Vec<String> = Vec::new();
    let mut touch = |accs: &mut HashMap<String, CardAcc>, id: &str| {
        if !accs.contains_key(id) {
            seen_order.push(id.to_string());
        }
        accs.entry(id.to_string()).or_default();
    };
    for e in list {
        let d = &e.details;
        match e.kind.as_str() {
            "createCard" => {
                for id in side_ids(d, "after") {
                    touch(&mut accs, &id);
                    let a = accs.get_mut(&id).unwrap();
                    a.title = entry_title(e, &id).or(a.title.take());
                    let lane = side_item(d, "after", &id)
                        .and_then(|i| i.get("lane"))
                        .and_then(s);
                    a.created = Some((e.ts.clone(), lane));
                }
            }
            "move" | "place" => {
                for id in side_ids(d, "after") {
                    let from = side_item(d, "before", &id)
                        .and_then(|i| i.get("lane"))
                        .and_then(s);
                    let to = side_item(d, "after", &id)
                        .and_then(|i| i.get("lane"))
                        .and_then(s)
                        .or_else(|| {
                            d.get("after")
                                .and_then(|a| a.get("to"))
                                .and_then(|t| t.get("lane"))
                                .and_then(s)
                        });
                    touch(&mut accs, &id);
                    let a = accs.get_mut(&id).unwrap();
                    if let Some(t) = entry_title(e, &id) {
                        a.title = Some(t);
                    }
                    if a.first_from.is_none() {
                        a.first_from = Some(from);
                    }
                    a.last_to = Some((e.ts.clone(), to));
                }
            }
            "moveBoard" => {
                // Journaled on both boards; on the target it reads as an arrival.
                if d.get("to").and_then(Value::as_str) == Some(bid) {
                    for id in &e.ids {
                        touch(&mut accs, id);
                        let a = accs.get_mut(id).unwrap();
                        a.title = entry_title(e, id).or(a.title.take());
                        a.from_other_board = true;
                        let lane = view.and_then(|v| v.card(id)).and_then(|c| c.lane.clone());
                        if a.first_from.is_none() {
                            a.first_from = Some(None);
                        }
                        a.last_to = Some((e.ts.clone(), lane));
                    }
                }
            }
            "edit" => {
                for id in &e.ids {
                    touch(&mut accs, id);
                    let a = accs.get_mut(id).unwrap();
                    if let Some(t) = entry_title(e, id) {
                        a.title = Some(t);
                    }
                    let chars = d.get("chars").and_then(Value::as_i64).unwrap_or(0);
                    if let Some(ts) = parse_ts(&e.ts) {
                        a.edits.push((ts, chars, e.ts.clone()));
                    }
                }
            }
            "trash" => {
                for id in side_ids(d, "before") {
                    touch(&mut accs, &id);
                    let a = accs.get_mut(&id).unwrap();
                    a.title = entry_title(e, &id).or(a.title.take());
                    a.deleted = Some(e.ts.clone());
                }
            }
            "restore" => {
                for id in &e.ids {
                    if let Some(a) = accs.get_mut(id) {
                        a.deleted = None;
                    }
                }
            }
            "setArchived" => {
                if let Some(items) = d
                    .get("after")
                    .and_then(|x| x.get("items"))
                    .and_then(Value::as_array)
                {
                    for it in items {
                        let Some(id) = it.get("id").and_then(Value::as_str) else {
                            continue;
                        };
                        let archived = it.get("archived").and_then(Value::as_bool).unwrap_or(true);
                        touch(&mut accs, id);
                        let a = accs.get_mut(id).unwrap();
                        a.title = entry_title(e, id).or(a.title.take());
                        a.archived = Some((archived, e.ts.clone()));
                    }
                }
            }
            "externalEdit" | "externalWrite" | "remoteSync" => {
                for id in &e.ids {
                    touch(&mut accs, id);
                    let a = accs.get_mut(id).unwrap();
                    if let Some(t) = entry_title(e, id) {
                        a.title = Some(t);
                    }
                    a.external = Some(e.ts.clone());
                }
            }
            _ => {
                if e.origin != Origin::You {
                    for id in &e.ids {
                        touch(&mut accs, id);
                        accs.get_mut(id).unwrap().external = Some(e.ts.clone());
                    }
                }
            }
        }
    }

    let mut bf = BoardFacts {
        id: bid.to_string(),
        name: view
            .map(|v| v.name.clone())
            .unwrap_or_else(|| bid.to_string()),
        lanes: view
            .map(|v| {
                v.lanes
                    .iter()
                    .map(|l| LaneStage {
                        name: l.clone(),
                        stage: infer_stage(l),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        ..Default::default()
    };
    for id in &seen_order {
        let a = &accs[id];
        if let Some(ts) = &a.deleted {
            bf.deleted.push(ctx.item(id, a, None, Some(ts.clone())));
            continue; // net effect: gone
        }
        if let Some((ts, lane)) = &a.created {
            bf.created
                .push(ctx.item(id, a, lane.clone(), Some(ts.clone())));
            if a.last_to.is_none() && ctx.stage_of(id, lane.as_deref()) == Stage::Done {
                bf.completed
                    .push(ctx.item(id, a, lane.clone(), Some(ts.clone())));
            }
        }
        if let (Some(from), Some((ts, to))) = (&a.first_from, &a.last_to) {
            // A card created in the period "comes from" its creation lane.
            let from = match &a.created {
                Some(c) if !a.from_other_board => c.1.clone(),
                _ => from.clone(),
            };
            if from != *to || a.from_other_board {
                let from_stage = from.as_deref().map(infer_stage).unwrap_or(Stage::Other);
                let to_stage = ctx.stage_of(id, to.as_deref());
                bf.moved.push(MovedItem {
                    item: ctx.item(id, a, to.clone(), Some(ts.clone())),
                    from,
                    to: to.clone(),
                });
                if to_stage == Stage::Done && from_stage != Stage::Done {
                    bf.completed
                        .push(ctx.item(id, a, to.clone(), Some(ts.clone())));
                } else if to_stage == Stage::InProgress
                    && !matches!(from_stage, Stage::InProgress | Stage::Done)
                {
                    bf.started
                        .push(ctx.item(id, a, to.clone(), Some(ts.clone())));
                }
            }
        }
        if !a.edits.is_empty() {
            let mut sessions = 0u32;
            let mut last: Option<DateTime<Utc>> = None;
            for (t, _, _) in &a.edits {
                if last.is_none_or(|l| (*t - l).num_minutes() > EDIT_SESSION_GAP_MIN) {
                    sessions += 1;
                }
                last = Some(*t);
            }
            let chars = a.edits.iter().map(|x| x.1).sum();
            let at = a.edits.last().map(|x| x.2.clone());
            bf.edited.push(EditedItem {
                item: ctx.item(id, a, None, at),
                edits: sessions,
                chars,
            });
        }
        if let Some((true, ts)) = &a.archived {
            bf.archived.push(ctx.item(id, a, None, Some(ts.clone())));
        }
        if let Some(ts) = &a.external {
            bf.external.push(ctx.item(id, a, None, Some(ts.clone())));
        }
    }

    // Pending / overdue from the current state.
    if let Some(v) = view.filter(|v| v.kanban) {
        let deleted: HashSet<&str> = bf.deleted.iter().map(|i| i.id.as_str()).collect();
        let mut pending: Vec<Item> = v
            .cards
            .iter()
            .filter(|c| !c.archived && !deleted.contains(c.id.as_str()))
            .filter(|c| {
                let st = c
                    .status
                    .unwrap_or_else(|| c.lane.as_deref().map(infer_stage).unwrap_or(Stage::Other));
                st != Stage::Done
            })
            .map(|c| Item {
                id: c.id.clone(),
                title: if c.title.is_empty() {
                    c.id.clone()
                } else {
                    c.title.clone()
                },
                lane: c.lane.clone(),
                due: c.due.clone(),
                priority: c.priority.clone(),
                tags: c.tags.clone(),
                at: None,
            })
            .collect();
        pending.sort_by(|a, b| {
            priority_rank(a.priority.as_deref())
                .cmp(&priority_rank(b.priority.as_deref()))
                .then_with(|| match (&a.due, &b.due) {
                    (Some(x), Some(y)) => x.cmp(y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    _ => std::cmp::Ordering::Equal,
                })
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        });
        bf.overdue = pending
            .iter()
            .filter(|i| {
                i.due
                    .as_deref()
                    .and_then(|d| {
                        NaiveDate::parse_from_str(d.get(..10).unwrap_or(d), "%Y-%m-%d").ok()
                    })
                    .is_some_and(|d| d < today)
            })
            .cloned()
            .collect();
        bf.pending = pending;
    }
    bf
}

/// Parse a status string (`done`, `In Progress`, …) into a stage, via the
/// status-category names first and the lane dictionary second.
pub fn status_stage(status: &str) -> Option<Stage> {
    from_status_category(status).or_else(|| match infer_stage(status) {
        Stage::Other => None,
        s => Some(s),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn e(ts: &str, kind: &str, ids: &[&str], details: Value) -> JournalEntry {
        JournalEntry {
            ts: ts.into(),
            board: "b1".into(),
            kind: kind.into(),
            origin: if kind.starts_with("external") {
                Origin::External
            } else {
                Origin::You
            },
            label: kind.into(),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            details,
            before: None,
            after: None,
        }
    }

    fn mv(ts: &str, id: &str, title: &str, from: &str, to: &str) -> JournalEntry {
        e(
            ts,
            "move",
            &[id],
            json!({
                "before": {"items": [{"id": id, "title": title, "lane": from}]},
                "after": {"items": [{"id": id, "title": title, "lane": to}], "to": {"lane": to}},
            }),
        )
    }

    fn view() -> BoardView {
        BoardView {
            id: "b1".into(),
            name: "Work".into(),
            kanban: true,
            lanes: vec!["📥 Inbox".into(), "Doing".into(), "✅ Done".into()],
            cards: vec![
                CardView {
                    id: "c1".into(),
                    title: "Fix login".into(),
                    lane: Some("✅ Done".into()),
                    ..Default::default()
                },
                CardView {
                    id: "c2".into(),
                    title: "Write docs".into(),
                    lane: Some("Doing".into()),
                    priority: Some("high".into()),
                    ..Default::default()
                },
                CardView {
                    id: "c3".into(),
                    title: "Pay invoice".into(),
                    lane: Some("📥 Inbox".into()),
                    due: Some("2026-09-01".into()),
                    ..Default::default()
                },
                CardView {
                    id: "c4".into(),
                    title: "Old".into(),
                    lane: Some("📥 Inbox".into()),
                    archived: true,
                    ..Default::default()
                },
            ],
        }
    }

    fn run(entries: Vec<JournalEntry>) -> ActivityFacts {
        let views = HashMap::from([("b1".to_string(), view())]);
        extract(
            &entries,
            &["b1".into()],
            &views,
            Period {
                from: "2026-09-29T00:00:00Z".into(),
                to: "2026-09-30T23:59:59Z".into(),
            },
            NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            false,
        )
    }

    #[test]
    fn net_moves_and_completion() {
        let f = run(vec![
            mv(
                "2026-09-29T10:00:00.000Z",
                "c1",
                "Fix login",
                "📥 Inbox",
                "Doing",
            ),
            mv(
                "2026-09-29T12:00:00.000Z",
                "c1",
                "Fix login",
                "Doing",
                "✅ Done",
            ),
            // Moved back and forth: net zero → not listed.
            mv(
                "2026-09-29T13:00:00.000Z",
                "c3",
                "Pay invoice",
                "📥 Inbox",
                "Doing",
            ),
            mv(
                "2026-09-29T13:05:00.000Z",
                "c3",
                "Pay invoice",
                "Doing",
                "📥 Inbox",
            ),
            mv(
                "2026-09-29T14:00:00.000Z",
                "c2",
                "Write docs",
                "📥 Inbox",
                "Doing",
            ),
        ]);
        let b = &f.boards[0];
        assert_eq!(b.moved.len(), 2);
        assert_eq!(b.moved[0].from.as_deref(), Some("📥 Inbox"));
        assert_eq!(b.moved[0].to.as_deref(), Some("✅ Done"));
        assert_eq!(
            b.completed
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["c1"]
        );
        assert_eq!(
            b.started.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["c2"]
        );
        assert_eq!(f.totals.completed, 1);
    }

    #[test]
    fn created_edited_deleted_archived() {
        let f = run(vec![
            e(
                "2026-09-29T09:00:00.000Z",
                "createCard",
                &["c2"],
                json!({"before": {"items": [{"id": "c2", "title": null}]}, "after": {"items": [{"id": "c2", "title": "Write docs", "lane": "📥 Inbox"}]}}),
            ),
            e(
                "2026-09-29T09:10:00.000Z",
                "edit",
                &["c2"],
                json!({"title": "Write docs", "chars": 120}),
            ),
            e(
                "2026-09-29T09:20:00.000Z",
                "edit",
                &["c2"],
                json!({"title": "Write docs", "chars": 30}),
            ),
            e(
                "2026-09-29T15:00:00.000Z",
                "edit",
                &["c2"],
                json!({"title": "Write docs", "chars": -10}),
            ),
            e(
                "2026-09-29T16:00:00.000Z",
                "createCard",
                &["c9"],
                json!({"after": {"items": [{"id": "c9", "title": "Temp", "lane": "📥 Inbox"}]}}),
            ),
            e(
                "2026-09-29T16:05:00.000Z",
                "trash",
                &["c9"],
                json!({"before": {"items": [{"id": "c9", "title": "Temp"}]}}),
            ),
            e(
                "2026-09-29T17:00:00.000Z",
                "setArchived",
                &["c4"],
                json!({"after": {"items": [{"id": "c4", "title": "Old", "archived": true}]}}),
            ),
            e(
                "2026-09-29T18:00:00.000Z",
                "externalEdit",
                &["c3"],
                json!({"titles": ["Pay invoice"]}),
            ),
            e(
                "2026-09-29T19:00:00.000Z",
                "createCard",
                &["c8"],
                json!({"after": {"items": [{"id": "c8", "title": "Logged", "lane": "✅ Done"}]}}),
            ),
        ]);
        let b = &f.boards[0];
        assert_eq!(
            b.created.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["c2", "c8"]
        );
        assert_eq!(
            b.completed
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["c8"]
        );
        assert_eq!(b.edited.len(), 1);
        assert_eq!(b.edited[0].edits, 2); // two sessions (gap > 30 min)
        assert_eq!(b.edited[0].chars, 140);
        assert_eq!(b.deleted.len(), 1);
        assert_eq!(b.deleted[0].title, "Temp");
        assert_eq!(b.archived[0].id, "c4");
        assert_eq!(b.external[0].title, "Pay invoice");
    }

    #[test]
    fn pending_and_overdue_from_current_state() {
        let f = run(vec![e(
            "2026-09-29T09:10:00.000Z",
            "edit",
            &["c2"],
            json!({"chars": 1}),
        )]);
        let b = &f.boards[0];
        // Done card and archived card excluded; high priority first, then due.
        assert_eq!(
            b.pending.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["c2", "c3"]
        );
        assert_eq!(
            b.overdue.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["c3"]
        );
    }

    #[test]
    fn status_overrides_lane() {
        let mut v = view();
        v.cards[1].status = Some(Stage::Done); // c2 is done in Jira while sitting in "Doing"
        let views = HashMap::from([("b1".to_string(), v)]);
        let f = extract(
            &[mv(
                "2026-09-29T10:00:00.000Z",
                "c2",
                "Write docs",
                "📥 Inbox",
                "Doing",
            )],
            &["b1".into()],
            &views,
            Period::default(),
            NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            false,
        );
        assert_eq!(f.boards[0].completed.len(), 1);
        assert!(f.boards[0].pending.iter().all(|i| i.id != "c2"));
    }

    #[test]
    fn idle_boards_skipped_unless_requested() {
        let f = run(vec![]);
        assert!(f.boards.is_empty());
        assert_eq!(status_stage("In Progress"), Some(Stage::InProgress));
        assert_eq!(status_stage("whatever"), None);
    }
}
