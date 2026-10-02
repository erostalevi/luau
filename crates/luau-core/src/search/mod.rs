//! Full-text search index (SQLite FTS5, trigram tokenizer).
//!
//! The index is a disposable cache in the app data folder; it can always be
//! rebuilt from the board files. Trigrams give substring matching; queries
//! shorter than 3 characters fall back to `LIKE`. Case-sensitive mode adds an
//! `instr()` post-filter.

pub mod query;

use std::path::Path;

use parking_lot::Mutex;
use rusqlite::{Connection, params, params_from_iter, types::Value as SqlValue};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::markdown::ParsedCard;
use crate::model::{AttachmentKind, BoardKind, BoardState, Parent};
pub use query::{Cmp, Filter, Query, Term};

const SCHEMA_VERSION: i64 = 4;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDoc {
    pub board: String,
    pub id: String,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub labels: Vec<String>,
    pub lane: Option<String>,
    pub lane_name: Option<String>,
    pub parent: Option<String>,
    pub is_group: bool,
    pub archived: bool,
    pub kind: String,
    pub tasks_total: usize,
    pub tasks_done: usize,
    pub has_image: bool,
    pub has_attachment: bool,
    pub due: Option<String>,
    /// `start:` from the property footer.
    pub start: Option<String>,
    pub priority: Option<String>,
    pub assignees: Vec<String>,
    pub mentions: Vec<String>,
    pub links: Vec<String>,
    pub status: Option<String>,
    pub status_cat: Option<String>,
    pub remote_key: Option<String>,
    pub mtime: i64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub board: String,
    pub board_name: String,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub lane_name: Option<String>,
    pub tags: Vec<String>,
    pub kind: String,
    pub is_group: bool,
    pub archived: bool,
    pub mtime: i64,
    pub remote_key: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchOptions {
    /// Restrict to these boards (ids). Empty = all.
    pub boards: Vec<String>,
    pub limit: Option<usize>,
    pub include_archived: bool,
}

pub struct SearchIndex {
    conn: Mutex<Connection>,
}

fn join_tokens(v: &[String]) -> String {
    if v.is_empty() {
        String::new()
    } else {
        format!(
            " {} ",
            v.iter()
                .map(|s| s.to_lowercase())
                .collect::<Vec<_>>()
                .join(" ")
        )
    }
}

fn split_tokens(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

impl SearchIndex {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| Error::io(p, e))?;
        }
        let conn = Connection::open(path).map_err(|e| Error::Other(format!("search db: {e}")))?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory().map_err(|e| Error::Other(e.to_string()))?)
    }

    fn init(conn: Connection) -> Result<Self> {
        let map = |e: rusqlite::Error| Error::Other(format!("search init: {e}"));
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA temp_store=MEMORY;",
        )
        .map_err(map)?;
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(map)?;
        if v != SCHEMA_VERSION {
            conn.execute_batch(
                "DROP TABLE IF EXISTS cards_fts; DROP TABLE IF EXISTS cards; DROP TABLE IF EXISTS boards;",
            )
            .map_err(map)?;
            conn.execute_batch(&format!(
                "CREATE TABLE cards(
                    rowid INTEGER PRIMARY KEY, board TEXT NOT NULL, id TEXT NOT NULL,
                    title TEXT NOT NULL DEFAULT '', body TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '',
                    labels TEXT NOT NULL DEFAULT '', lane TEXT, lane_name TEXT, parent TEXT,
                    is_group INT NOT NULL DEFAULT 0, archived INT NOT NULL DEFAULT 0, kind TEXT NOT NULL DEFAULT 'card',
                    tasks_total INT NOT NULL DEFAULT 0, tasks_done INT NOT NULL DEFAULT 0,
                    has_image INT NOT NULL DEFAULT 0, has_attachment INT NOT NULL DEFAULT 0,
                    due TEXT, start TEXT, priority TEXT, assignees TEXT NOT NULL DEFAULT '', mentions TEXT NOT NULL DEFAULT '',
                    links TEXT NOT NULL DEFAULT '', status TEXT, status_cat TEXT, remote_key TEXT,
                    mtime INT NOT NULL DEFAULT 0, hash TEXT NOT NULL DEFAULT '', UNIQUE(board, id));
                 CREATE INDEX cards_id ON cards(id);
                 CREATE TABLE boards(board TEXT PRIMARY KEY, name TEXT NOT NULL, kind TEXT NOT NULL);
                 CREATE VIRTUAL TABLE cards_fts USING fts5(title, body, tags, content='cards', content_rowid='rowid',
                    tokenize='trigram case_sensitive 0 remove_diacritics 1');
                 CREATE TRIGGER cards_ai AFTER INSERT ON cards BEGIN
                    INSERT INTO cards_fts(rowid, title, body, tags) VALUES (new.rowid, new.title, new.body, new.tags); END;
                 CREATE TRIGGER cards_ad AFTER DELETE ON cards BEGIN
                    INSERT INTO cards_fts(cards_fts, rowid, title, body, tags) VALUES ('delete', old.rowid, old.title, old.body, old.tags); END;
                 CREATE TRIGGER cards_au AFTER UPDATE ON cards BEGIN
                    INSERT INTO cards_fts(cards_fts, rowid, title, body, tags) VALUES ('delete', old.rowid, old.title, old.body, old.tags);
                    INSERT INTO cards_fts(rowid, title, body, tags) VALUES (new.rowid, new.title, new.body, new.tags); END;
                 PRAGMA user_version = {SCHEMA_VERSION};"
            ))
            .map_err(map)?;
        }
        Ok(SearchIndex {
            conn: Mutex::new(conn),
        })
    }

    pub fn set_board(&self, id: &str, name: &str, kind: &str) -> Result<()> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO boards(board, name, kind) VALUES (?1, ?2, ?3)
                 ON CONFLICT(board) DO UPDATE SET name = excluded.name, kind = excluded.kind",
                params![id, name, kind],
            )
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    /// Hashes currently indexed for a board (to skip unchanged docs).
    pub fn hashes(&self, board: &str) -> std::collections::HashMap<String, String> {
        let c = self.conn.lock();
        let mut out = std::collections::HashMap::new();
        if let Ok(mut st) = c.prepare("SELECT id, hash FROM cards WHERE board = ?1")
            && let Ok(rows) = st.query_map([board], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
        {
            out.extend(rows.flatten());
        }
        out
    }

    pub fn upsert(&self, docs: &[IndexDoc]) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction().map_err(|e| Error::Other(e.to_string()))?;
        {
            let mut st = tx
                .prepare_cached(
                    "INSERT INTO cards(board, id, title, body, tags, labels, lane, lane_name, parent, is_group, archived, kind,
                        tasks_total, tasks_done, has_image, has_attachment, due, start, priority, assignees, mentions, links,
                        status, status_cat, remote_key, mtime, hash)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27)
                     ON CONFLICT(board, id) DO UPDATE SET title=excluded.title, body=excluded.body, tags=excluded.tags,
                        labels=excluded.labels, lane=excluded.lane, lane_name=excluded.lane_name, parent=excluded.parent,
                        is_group=excluded.is_group, archived=excluded.archived, kind=excluded.kind,
                        tasks_total=excluded.tasks_total, tasks_done=excluded.tasks_done, has_image=excluded.has_image,
                        has_attachment=excluded.has_attachment, due=excluded.due, start=excluded.start, priority=excluded.priority,
                        assignees=excluded.assignees, mentions=excluded.mentions, links=excluded.links,
                        status=excluded.status, status_cat=excluded.status_cat, remote_key=excluded.remote_key,
                        mtime=excluded.mtime, hash=excluded.hash",
                )
                .map_err(|e| Error::Other(e.to_string()))?;
            for d in docs {
                st.execute(params![
                    d.board,
                    d.id,
                    d.title,
                    d.body,
                    join_tokens(&d.tags),
                    join_tokens(&d.labels),
                    d.lane,
                    d.lane_name,
                    d.parent,
                    d.is_group as i64,
                    d.archived as i64,
                    d.kind,
                    d.tasks_total as i64,
                    d.tasks_done as i64,
                    d.has_image as i64,
                    d.has_attachment as i64,
                    d.due,
                    d.start,
                    d.priority,
                    join_tokens(&d.assignees),
                    join_tokens(&d.mentions),
                    join_tokens(&d.links),
                    d.status,
                    d.status_cat,
                    d.remote_key,
                    d.mtime,
                    d.hash
                ])
                .map_err(|e| Error::Other(e.to_string()))?;
            }
        }
        tx.commit().map_err(|e| Error::Other(e.to_string()))
    }

    pub fn remove(&self, board: &str, ids: &[String]) -> Result<()> {
        let c = self.conn.lock();
        for id in ids {
            c.execute(
                "DELETE FROM cards WHERE board = ?1 AND id = ?2",
                params![board, id],
            )
            .map_err(|e| Error::Other(e.to_string()))?;
        }
        Ok(())
    }

    /// Remove every doc of `board` not in `keep`.
    pub fn retain(&self, board: &str, keep: &std::collections::HashSet<String>) -> Result<()> {
        let existing: Vec<String> = self
            .hashes(board)
            .into_keys()
            .filter(|id| !keep.contains(id))
            .collect();
        self.remove(board, &existing)
    }

    pub fn drop_board(&self, board: &str) -> Result<()> {
        let c = self.conn.lock();
        c.execute("DELETE FROM cards WHERE board = ?1", [board])
            .map_err(|e| Error::Other(e.to_string()))?;
        c.execute("DELETE FROM boards WHERE board = ?1", [board])
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(())
    }

    /// True when any board already uses this card id.
    pub fn id_taken(&self, id: &str) -> bool {
        self.conn
            .lock()
            .query_row(
                "SELECT 1 FROM cards WHERE id = ?1 LIMIT 1",
                [id],
                |_| Ok(()),
            )
            .is_ok()
    }

    /// Find which board holds a card id.
    pub fn locate(&self, id: &str) -> Option<(String, String)> {
        self.conn
            .lock()
            .query_row(
                "SELECT board, title FROM cards WHERE id = ?1 LIMIT 1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok()
    }

    /// Titles for many ids at once (link chips for cross-board links).
    pub fn titles(&self, ids: &[String]) -> Vec<(String, String, String)> {
        let c = self.conn.lock();
        let mut out = Vec::new();
        if let Ok(mut st) =
            c.prepare_cached("SELECT id, board, title FROM cards WHERE id = ?1 LIMIT 1")
        {
            for id in ids {
                if let Ok(row) = st.query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))) {
                    out.push(row);
                }
            }
        }
        out
    }

    /// Cards linking to `id` (backlinks) across all boards.
    pub fn backlinks(&self, id: &str) -> Vec<(String, String, String)> {
        let c = self.conn.lock();
        let mut out = Vec::new();
        if let Ok(mut st) =
            c.prepare("SELECT board, id, title FROM cards WHERE links LIKE ?1 LIMIT 200")
            && let Ok(rows) = st.query_map([format!("% {id} %")], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
        {
            out.extend(rows.flatten());
        }
        out
    }

    /// Distinct tags (with counts), optionally scoped to boards.
    pub fn tags(&self, boards: &[String]) -> Vec<(String, usize)> {
        let c = self.conn.lock();
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        let sql = if boards.is_empty() {
            "SELECT tags || labels FROM cards".to_string()
        } else {
            format!(
                "SELECT tags || labels FROM cards WHERE board IN ({})",
                vec!["?"; boards.len()].join(",")
            )
        };
        if let Ok(mut st) = c.prepare(&sql)
            && let Ok(rows) =
                st.query_map(params_from_iter(boards.iter()), |r| r.get::<_, String>(0))
        {
            for row in rows.flatten() {
                for t in split_tokens(&row) {
                    *counts.entry(t).or_default() += 1;
                }
            }
        }
        let mut v: Vec<_> = counts.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    /// Distinct people mentioned or assigned.
    pub fn people(&self) -> Vec<String> {
        let c = self.conn.lock();
        let mut set = std::collections::BTreeSet::new();
        if let Ok(mut st) = c.prepare("SELECT mentions || assignees FROM cards")
            && let Ok(rows) = st.query_map([], |r| r.get::<_, String>(0))
        {
            for row in rows.flatten() {
                set.extend(split_tokens(&row));
            }
        }
        set.into_iter().collect()
    }

    pub fn search(&self, text: &str, opts: &SearchOptions) -> Result<Vec<SearchHit>> {
        let q = query::parse(text);
        self.search_query(&q, opts)
    }

    #[allow(clippy::type_complexity)]
    pub fn search_query(&self, q: &Query, opts: &SearchOptions) -> Result<Vec<SearchHit>> {
        let mut wh: Vec<String> = Vec::new();
        let mut args: Vec<SqlValue> = Vec::new();
        let mut fts_parts: Vec<String> = Vec::new();
        let cols = if q.title_only {
            "c.title"
        } else {
            "c.title || ' ' || c.body"
        };
        for t in &q.terms {
            let long = t.value.chars().count() >= 3;
            if long && !t.negate && !q.title_only {
                fts_parts.push(format!("\"{}\"", t.value.replace('"', "\"\"")));
            } else if long && t.negate && !q.title_only {
                wh.push(
                    "c.rowid NOT IN (SELECT rowid FROM cards_fts WHERE cards_fts MATCH ?)".into(),
                );
                args.push(SqlValue::Text(format!(
                    "\"{}\"",
                    t.value.replace('"', "\"\"")
                )));
            } else {
                let like = format!("%{}%", t.value.replace('%', "\\%").replace('_', "\\_"));
                let clause = format!("({cols}) LIKE ? ESCAPE '\\'");
                wh.push(if t.negate {
                    format!("NOT {clause}")
                } else {
                    clause
                });
                args.push(SqlValue::Text(like));
            }
            if q.case_sensitive {
                let clause = format!("instr({cols}, ?) > 0");
                wh.push(if t.negate {
                    format!("NOT {clause}")
                } else {
                    clause
                });
                args.push(SqlValue::Text(t.value.clone()));
            }
        }
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        for f in &q.filters {
            let v = f.value.to_lowercase();
            let (clause, vals): (String, Vec<SqlValue>) = match f.key.as_str() {
                "tag" | "label" => (
                    "((c.tags || c.labels) LIKE ? OR (c.tags || c.labels) LIKE ?)".into(),
                    vec![SqlValue::Text(format!("% {v} %")), SqlValue::Text(format!("% {v}/%"))],
                ),
                "board" => (
                    "(c.board = ? OR c.board IN (SELECT board FROM boards WHERE lower(name) LIKE ?))".into(),
                    vec![SqlValue::Text(f.value.clone()), SqlValue::Text(format!("%{v}%"))],
                ),
                "lane" => (
                    "(c.lane = ? OR lower(c.lane_name) LIKE ?)".into(),
                    vec![SqlValue::Text(f.value.clone()), SqlValue::Text(format!("%{v}%"))],
                ),
                "status" => ("(lower(c.status) LIKE ? OR lower(c.status_cat) = ?)".into(), vec![SqlValue::Text(format!("%{v}%")), SqlValue::Text(v.clone())]),
                "priority" => ("lower(c.priority) = ?".into(), vec![SqlValue::Text(v.clone())]),
                "assignee" => ("c.assignees LIKE ?".into(), vec![SqlValue::Text(format!("% {v} %"))]),
                "mention" => ("(c.mentions || c.assignees) LIKE ?".into(), vec![SqlValue::Text(format!("% {v} %"))]),
                "links" => ("c.links LIKE ?".into(), vec![SqlValue::Text(format!("% {v} %"))]),
                "linkedfrom" => (
                    "(SELECT links FROM cards x WHERE x.id = ? LIMIT 1) LIKE ('% ' || c.id || ' %')".into(),
                    vec![SqlValue::Text(f.value.clone())],
                ),
                "id" => ("c.id = ?".into(), vec![SqlValue::Text(f.value.clone())]),
                "key" => ("lower(c.remote_key) = ?".into(), vec![SqlValue::Text(v.clone())]),
                "type" => ("c.kind = ?".into(), vec![SqlValue::Text(v.clone())]),
                "is" => match v.as_str() {
                    "open" => ("c.tasks_total > c.tasks_done".into(), vec![]),
                    "done" => ("(c.tasks_total > 0 AND c.tasks_total = c.tasks_done) OR lower(c.status_cat) = 'done'".into(), vec![]),
                    "archived" => ("c.archived = 1".into(), vec![]),
                    "group" => ("c.is_group = 1".into(), vec![]),
                    "card" => ("c.is_group = 0".into(), vec![]),
                    "remote" | "jira" | "linked" => ("c.remote_key IS NOT NULL".into(), vec![]),
                    "doc" | "document" => ("c.kind = 'doc'".into(), vec![]),
                    "mirror" => ("c.kind = 'mirror'".into(), vec![]),
                    _ => continue,
                },
                "has" => match v.as_str() {
                    "image" | "images" => ("c.has_image = 1".into(), vec![]),
                    "attachment" | "attachments" | "file" => ("c.has_attachment = 1".into(), vec![]),
                    "tasks" | "checklist" => ("c.tasks_total > 0".into(), vec![]),
                    "link" | "links" => ("c.links != ''".into(), vec![]),
                    "due" => ("c.due IS NOT NULL".into(), vec![]),
                    "start" => ("c.start IS NOT NULL".into(), vec![]),
                    "tag" | "tags" => ("(c.tags || c.labels) != ''".into(), vec![]),
                    _ => continue,
                },
                "due" | "start" | "started" | "updated" | "created" => {
                    let col = match f.key.as_str() {
                        "due" => "c.due",
                        "start" | "started" => "c.start",
                        _ => "date(c.mtime / 1000, 'unixepoch', 'localtime')",
                    };
                    let (cmp, val) = match v.as_str() {
                        "overdue" => (Cmp::Lt, today.clone()),
                        "today" => (Cmp::Eq, today.clone()),
                        _ => (f.cmp, f.value.clone()),
                    };
                    let op = match cmp {
                        Cmp::Eq => "=",
                        Cmp::Lt => "<",
                        Cmp::Le => "<=",
                        Cmp::Gt => ">",
                        Cmp::Ge => ">=",
                    };
                    (format!("({col} IS NOT NULL AND substr({col}, 1, 10) {op} ?)"), vec![SqlValue::Text(val)])
                }
                _ => continue,
            };
            wh.push(if f.negate {
                format!("NOT ({clause})")
            } else {
                clause
            });
            args.extend(vals);
        }
        if !opts.boards.is_empty() {
            wh.push(format!(
                "c.board IN ({})",
                vec!["?"; opts.boards.len()].join(",")
            ));
            args.extend(opts.boards.iter().map(|b| SqlValue::Text(b.clone())));
        }
        let wants_archived = q
            .filters
            .iter()
            .any(|f| f.key == "is" && f.value == "archived" && !f.negate);
        if !opts.include_archived && !wants_archived {
            wh.push("c.archived = 0".into());
        }
        let limit = opts.limit.unwrap_or(200).min(2000);
        let use_fts = !fts_parts.is_empty();
        let sql = if use_fts {
            let mut all_args = vec![SqlValue::Text(fts_parts.join(" AND "))];
            all_args.extend(args);
            args = all_args;
            format!(
                "SELECT c.board, COALESCE(b.name, c.board), c.id, c.title,
                    snippet(cards_fts, 1, char(2), char(3), '…', 14), c.lane_name, c.tags, c.kind, c.is_group,
                    c.archived, c.mtime, c.remote_key, c.status
                 FROM cards_fts JOIN cards c ON c.rowid = cards_fts.rowid LEFT JOIN boards b ON b.board = c.board
                 WHERE cards_fts MATCH ? {} ORDER BY bm25(cards_fts, 8.0, 1.0, 3.0) LIMIT {limit}",
                wh.iter().map(|w| format!(" AND {w}")).collect::<String>()
            )
        } else {
            format!(
                "SELECT c.board, COALESCE(b.name, c.board), c.id, c.title, substr(c.body, 1, 160), c.lane_name, c.tags,
                    c.kind, c.is_group, c.archived, c.mtime, c.remote_key, c.status
                 FROM cards c LEFT JOIN boards b ON b.board = c.board
                 {} ORDER BY c.mtime DESC LIMIT {limit}",
                if wh.is_empty() { String::new() } else { format!("WHERE {}", wh.join(" AND ")) }
            )
        };
        let c = self.conn.lock();
        let mut st = c
            .prepare(&sql)
            .map_err(|e| Error::Other(format!("search: {e}")))?;
        let rows = st
            .query_map(params_from_iter(args.iter()), |r| {
                Ok(SearchHit {
                    board: r.get(0)?,
                    board_name: r.get(1)?,
                    id: r.get(2)?,
                    title: r.get(3)?,
                    snippet: r.get::<_, String>(4)?.replace('\n', " "),
                    lane_name: r.get(5)?,
                    tags: split_tokens(&r.get::<_, String>(6)?),
                    kind: r.get(7)?,
                    is_group: r.get::<_, i64>(8)? != 0,
                    archived: r.get::<_, i64>(9)? != 0,
                    mtime: r.get(10)?,
                    remote_key: r.get(11)?,
                    status: r.get(12)?,
                })
            })
            .map_err(|e| Error::Other(format!("search: {e}")))?;
        Ok(rows.flatten().collect())
    }
}

/// Build index docs for every node of a board. `extra` lets integrations add
/// remote info (key, status) per card id.
pub fn docs_for_board(
    st: &BoardState,
    remote: &dyn Fn(&str) -> Option<(String, Option<String>, Option<String>)>,
    body_of: &dyn Fn(&str) -> Option<String>,
) -> Vec<IndexDoc> {
    let kind = match st.manifest.kind {
        BoardKind::Kanban => "card",
        BoardKind::Files => "doc",
    };
    st.nodes
        .values()
        .map(|n| {
            let lane = st.lane_of(&n.id);
            let lane_name = lane
                .as_ref()
                .and_then(|k| st.lane(k))
                .map(|l| l.name.clone());
            let body = if n.meta.plain.is_empty() {
                body_of(&n.id).unwrap_or_default()
            } else {
                n.meta.plain.clone()
            };
            let r = remote(&n.id);
            doc_from(
                &st.manifest.id,
                &n.id,
                &n.meta,
                n,
                lane,
                lane_name,
                kind,
                body,
                r,
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn doc_from(
    board: &str,
    id: &str,
    meta: &ParsedCard,
    n: &crate::model::Node,
    lane: Option<String>,
    lane_name: Option<String>,
    kind: &str,
    body: String,
    remote: Option<(String, Option<String>, Option<String>)>,
) -> IndexDoc {
    let parent = match &n.parent {
        Parent::Card(c) => Some(c.clone()),
        _ => None,
    };
    let (remote_key, status, status_cat) = match remote {
        Some((k, s, c)) => (Some(k), s, c),
        None => (None, None, None),
    };
    IndexDoc {
        board: board.to_string(),
        id: id.to_string(),
        title: meta.title.clone(),
        body,
        tags: meta.tags.clone(),
        labels: meta.footer.labels.clone(),
        lane,
        lane_name,
        parent,
        is_group: n.is_group,
        archived: n.archived,
        kind: kind.to_string(),
        tasks_total: meta.tasks.total,
        tasks_done: meta.tasks.done,
        has_image: n
            .attachments
            .iter()
            .any(|a| a.kind == AttachmentKind::Image)
            || meta.file_refs.iter().any(|f| {
                AttachmentKind::from_ext(f.rsplit('.').next().unwrap_or(""))
                    == AttachmentKind::Image
            }),
        has_attachment: !n.attachments.is_empty(),
        due: meta
            .footer
            .due
            .clone()
            .or_else(|| meta.dates.first().cloned()),
        start: meta.footer.start.clone(),
        priority: meta.footer.priority.clone(),
        assignees: meta.footer.assignees.clone(),
        mentions: meta.mentions.clone(),
        links: meta.links.iter().filter_map(|l| l.id.clone()).collect(),
        status,
        status_cat,
        remote_key,
        mtime: n.mtime,
        hash: n.hash.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(id: &str, title: &str, body: &str, tags: &[&str]) -> IndexDoc {
        IndexDoc {
            board: "b1".into(),
            id: id.into(),
            title: title.into(),
            body: body.into(),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            kind: "card".into(),
            hash: "h".into(),
            ..Default::default()
        }
    }

    #[test]
    fn started_filters_on_the_footer_start_date() {
        let idx = SearchIndex::in_memory().unwrap();
        idx.set_board("b1", "B", "kanban").unwrap();
        let mut a = doc("c1", "Early", "", &[]);
        a.start = Some("2026-09-01".into());
        let mut b = doc("c2", "Late", "", &[]);
        b.start = Some("2026-10-05".into());
        idx.upsert(&[a, b, doc("c3", "Never", "", &[])]).unwrap();
        let hits = |q: &str| {
            let mut v = idx
                .search(q, &SearchOptions::default())
                .unwrap()
                .into_iter()
                .map(|h| h.id)
                .collect::<Vec<_>>();
            v.sort();
            v
        };
        assert_eq!(hits("started:<2026-10-01"), vec!["c1"]);
        assert_eq!(hits("start:>=2026-10-01"), vec!["c2"]);
        assert_eq!(hits("has:start"), vec!["c1", "c2"]);
    }

    #[test]
    fn substring_case_and_filters() {
        let idx = SearchIndex::in_memory().unwrap();
        idx.set_board("b1", "Project Alpha", "kanban").unwrap();
        idx.upsert(&[
            doc(
                "c1",
                "Fix Relogin flow",
                "Safari breaks the OAuth dance",
                &["backend", "work/client-a"],
            ),
            doc(
                "c2",
                "Design screens",
                "Figma mockups for onboarding",
                &["design"],
            ),
            doc("c3", "Ops", "rotate keys", &["backend"]),
        ])
        .unwrap();
        let hits = |q: &str| {
            idx.search(q, &SearchOptions::default())
                .unwrap()
                .into_iter()
                .map(|h| h.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(hits("login"), vec!["c1"]); // substring inside "Relogin"
        assert_eq!(hits("oauth"), vec!["c1"]); // case-insensitive
        assert!(hits("oauth case:yes").is_empty());
        assert_eq!(hits("OAuth case:yes"), vec!["c1"]);
        assert_eq!(hits("tag:work"), vec!["c1"]); // nested tag prefix
        let mut b = hits("tag:backend");
        b.sort();
        assert_eq!(b, vec!["c1", "c3"]);
        assert_eq!(hits("tag:backend -oauth"), vec!["c3"]);
        assert_eq!(hits("ke"), vec!["c3"]); // short term -> LIKE
        assert_eq!(hits("board:alpha design"), vec!["c2"]);
        let snip = &idx.search("mockups", &SearchOptions::default()).unwrap()[0].snippet;
        assert!(snip.contains('\u{2}'));
        assert!(idx.id_taken("c2"));
        assert_eq!(idx.tags(&[])[0], (String::from("backend"), 2));
    }
}
