//! Deterministic ("basic") Markdown rendering of [`ActivityFacts`], per
//! detail level (1 = a few bullets per board … 5 = exhaustive), localized, and
//! honouring obvious intents in the user's prompt.

use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, Timelike, Utc, Weekday};

use super::facts::{ActivityFacts, BoardFacts, Item, priority_rank};
use super::locale::{Locale, Strings, fill};
use super::stage::normalize;

/// Time zone used to show times (the machine's local zone in the app; UTC in tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Zone {
    #[default]
    Local,
    Utc,
}

impl Zone {
    pub fn to_local(self, t: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Local => t.with_timezone(&chrono::Local).naive_local(),
            Zone::Utc => t.naive_utc(),
        }
    }
    pub fn parse(self, ts: &str) -> Option<NaiveDateTime> {
        DateTime::parse_from_rfc3339(ts)
            .ok()
            .map(|d| self.to_local(d.with_timezone(&Utc)))
    }
}

/// Intents detected in the free-text prompt (multilingual, accent-insensitive).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Intents {
    pub pending: bool,
    pub priorities: bool,
    pub weekend: bool,
    pub overdue: bool,
}

const PENDING_WORDS: &[&str] = &[
    "pending",
    "open",
    "todo",
    "to do",
    "backlog",
    "remaining",
    "left to",
    "unfinished",
    "not done",
    "pendiente",
    "pendientes",
    "abierto",
    "abiertos",
    "abiertas",
    "falta",
    "faltan",
    "por hacer",
    "sin terminar",
    "pendente",
    "pendentes",
    "aberto",
    "abertos",
    "a fazer",
];
const PRIORITY_WORDS: &[&str] = &[
    "priorit",
    "prioridad",
    "prioridade",
    "focus",
    "foco",
    "this week",
    "next week",
    "esta semana",
    "proxima semana",
    "essa semana",
    "nesta semana",
    "plan",
    "urgent",
    "urgente",
    "importante",
    "important",
];
const WEEKEND_WORDS: &[&str] = &[
    "weekend",
    "week end",
    "fin de semana",
    "finde",
    "fim de semana",
    "saturday",
    "sunday",
    "sabado",
    "domingo",
];
const OVERDUE_WORDS: &[&str] = &[
    "overdue", "late", "deadline", "due", "vencid", "atrasad", "retrasad", "vence", "prazo",
    "plazo",
];

pub fn detect_intents(prompt: &str) -> Intents {
    let p = format!(" {} ", normalize(prompt));
    let any = |words: &[&str]| {
        words.iter().any(|w| {
            if w.ends_with(' ') || w.len() <= 4 {
                p.contains(&format!(" {w} "))
            } else {
                p.contains(w)
            }
        })
    };
    let priorities = any(PRIORITY_WORDS);
    Intents {
        pending: any(PENDING_WORDS) || priorities,
        priorities,
        weekend: any(WEEKEND_WORDS),
        overdue: any(OVERDUE_WORDS),
    }
}

#[derive(Debug, Clone)]
pub struct RenderOpts {
    pub locale: Locale,
    /// 1..=5
    pub detail: u8,
    /// Emit `[[id]]` card links (chips) instead of plain titles.
    pub links: bool,
    pub intents: Intents,
    pub zone: Zone,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts {
            locale: Locale::En,
            detail: 3,
            links: false,
            intents: Intents::default(),
            zone: Zone::Local,
        }
    }
}

struct R<'a> {
    o: &'a RenderOpts,
    s: &'static Strings,
    out: String,
}

impl R<'_> {
    fn line(&mut self, l: impl AsRef<str>) {
        self.out.push_str(l.as_ref());
        self.out.push('\n');
    }

    fn limit(&self) -> usize {
        match self.o.detail {
            0 | 1 => 3,
            2 => 5,
            3 => 25,
            4 => 60,
            _ => usize::MAX,
        }
    }

    fn weekday(&self, w: Weekday) -> &'static str {
        self.s.weekdays[w.num_days_from_monday() as usize]
    }

    fn date(&self, d: NaiveDate) -> String {
        format!(
            "{} {} {} {}",
            self.weekday(d.weekday()),
            d.day(),
            self.s.months[d.month0() as usize],
            d.year()
        )
    }

    fn when(&self, ts: &str) -> Option<String> {
        let t = self.o.zone.parse(ts)?;
        Some(format!(
            "{} {:02}:{:02}",
            self.weekday(t.weekday()),
            t.hour(),
            t.minute()
        ))
    }

    fn name(&self, i: &Item) -> String {
        if self.o.links {
            format!("[[{}]]", i.id)
        } else {
            escape(&i.title)
        }
    }

    /// Item with annotations appropriate for the detail level.
    fn item(&self, i: &Item, with_lane: bool) -> String {
        let mut s = self.name(i);
        let mut notes: Vec<String> = Vec::new();
        if self.o.detail >= 4 {
            if with_lane && let Some(l) = &i.lane {
                notes.push(escape(l));
            }
            if let Some(p) = i.priority.as_ref().filter(|p| priority_rank(Some(p)) < 4) {
                notes.push(format!("{} {}", self.s.priority, p));
            }
            if let Some(d) = &i.due {
                notes.push(format!("{} {}", self.s.due, d));
            }
            if !i.tags.is_empty() {
                notes.push(
                    i.tags
                        .iter()
                        .map(|t| format!("#{t}"))
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
        }
        if self.o.detail >= 5
            && let Some(w) = i.at.as_deref().and_then(|a| self.when(a))
        {
            notes.push(w);
        }
        if !notes.is_empty() {
            s.push_str(&format!(" — {}", notes.join(", ")));
        }
        s
    }

    fn items(&self, v: &[Item], lane: bool) -> Vec<String> {
        v.iter().map(|i| self.item(i, lane)).collect()
    }

    fn inline_list(&self, items: &[String]) -> String {
        let lim = self.limit();
        let mut shown: Vec<String> = items.iter().take(lim).cloned().collect();
        if items.len() > lim {
            shown.push(fill(self.s.more, items.len() - lim));
        }
        shown.join(", ")
    }

    fn section(&mut self, title: &str, rows: Vec<String>) {
        if rows.is_empty() {
            return;
        }
        let lim = self.limit();
        self.line(format!("\n### {title} ({})", rows.len()));
        for r in rows.iter().take(lim) {
            self.line(format!("- {r}"));
        }
        if rows.len() > lim {
            self.line(format!("- {}", fill(self.s.more, rows.len() - lim)));
        }
    }

    fn board(&mut self, b: &BoardFacts) {
        let s = self.s;
        let d = self.o.detail;
        let it = self.o.intents;
        self.line(format!("\n## {}", escape(&b.name)));
        if d <= 1 {
            let mut bullets: Vec<String> = Vec::new();
            let names = |v: &[Item]| v.iter().map(|i| self.name(i)).collect::<Vec<_>>();
            for (label, list) in [
                (s.completed, &b.completed),
                (s.started, &b.started),
                (s.created, &b.created),
            ] {
                if !list.is_empty() {
                    bullets.push(format!(
                        "**{label} ({})**: {}",
                        list.len(),
                        self.inline_list(&names(list))
                    ));
                }
            }
            if !b.edited.is_empty() {
                bullets.push(fill(s.edited_count, b.edited.len()));
            }
            if !b.deleted.is_empty() {
                bullets.push(format!("{} ({})", s.deleted, b.deleted.len()));
            }
            if it.pending && !b.pending.is_empty() {
                let pend: Vec<String> = b.pending.iter().map(|i| self.name(i)).collect();
                bullets.push(format!(
                    "**{} ({})**: {}",
                    s.pending,
                    b.pending.len(),
                    self.inline_list(&pend)
                ));
            }
            if (it.overdue || it.pending) && !b.overdue.is_empty() {
                bullets.push(format!(
                    "**{} ({})**: {}",
                    s.overdue,
                    b.overdue.len(),
                    self.inline_list(&names(&b.overdue))
                ));
            }
            if bullets.is_empty() {
                bullets.push(s.no_activity.to_string());
            }
            for x in bullets {
                self.line(format!("- {x}"));
            }
            return;
        }
        let completed = self.items(&b.completed, true);
        let started = self.items(&b.started, true);
        let created = self.items(&b.created, true);
        self.section(s.completed, completed);
        self.section(s.started, started);
        self.section(s.created, created);
        if d >= 3 {
            let moved = b
                .moved
                .iter()
                .map(|m| {
                    let from = m
                        .from
                        .clone()
                        .map(|x| escape(&x))
                        .unwrap_or_else(|| s.another_board.to_string());
                    let to =
                        m.to.clone()
                            .map(|x| escape(&x))
                            .unwrap_or_else(|| "—".into());
                    let when = if d >= 5 {
                        m.item
                            .at
                            .as_deref()
                            .and_then(|a| self.when(a))
                            .map(|w| format!(" ({w})"))
                    } else {
                        None
                    };
                    format!(
                        "{}: {from} → {to}{}",
                        self.name(&m.item),
                        when.unwrap_or_default()
                    )
                })
                .collect();
            self.section(s.moved, moved);
        }
        let edited = b
            .edited
            .iter()
            .map(|e| {
                let mut x = self.item(&e.item, false);
                if d >= 4 {
                    let sign = if e.chars >= 0 { "+" } else { "" };
                    x.push_str(&format!(
                        " ({}, {})",
                        fill(s.sessions, e.edits),
                        fill(s.chars, format!("{sign}{}", e.chars))
                    ));
                }
                x
            })
            .collect();
        self.section(s.edited, edited);
        let deleted = self.items(&b.deleted, false);
        self.section(s.deleted, deleted);
        if d >= 3 {
            let archived = self.items(&b.archived, false);
            self.section(s.archived, archived);
        }
        if d >= 4 {
            let external = self.items(&b.external, false);
            self.section(s.external, external);
        }
        if d >= 3 || it.overdue || it.pending {
            let overdue = self.items(&b.overdue, true);
            self.section(s.overdue, overdue);
        }
        if it.priorities {
            let pri: Vec<String> = b
                .pending
                .iter()
                .filter(|i| priority_rank(i.priority.as_deref()) <= 1 || i.due.is_some())
                .map(|i| {
                    let mut i = i.clone();
                    i.at = None;
                    self.item_forced(&i)
                })
                .collect();
            self.section(s.priorities, pri);
        }
        if it.pending || d >= 5 {
            let pend = b.pending.iter().map(|i| self.item(i, true)).collect();
            self.section(s.pending, pend);
        }
    }

    /// Item with priority/due always shown (priorities section).
    fn item_forced(&self, i: &Item) -> String {
        let mut notes = Vec::new();
        if let Some(p) = &i.priority {
            notes.push(format!("{} {p}", self.s.priority));
        }
        if let Some(d) = &i.due {
            notes.push(format!("{} {d}", self.s.due));
        }
        if let Some(l) = &i.lane {
            notes.push(escape(l));
        }
        if notes.is_empty() {
            self.name(i)
        } else {
            format!("{} — {}", self.name(i), notes.join(", "))
        }
    }

    fn weekend(&mut self, f: &ActivityFacts) {
        let mut rows = Vec::new();
        for b in &f.boards {
            let mut seen = std::collections::HashSet::new();
            let all = b
                .completed
                .iter()
                .chain(&b.started)
                .chain(&b.created)
                .chain(b.moved.iter().map(|m| &m.item))
                .chain(b.edited.iter().map(|e| &e.item))
                .chain(&b.external);
            for i in all {
                let Some(t) = i.at.as_deref().and_then(|a| self.o.zone.parse(a)) else {
                    continue;
                };
                if matches!(t.weekday(), Weekday::Sat | Weekday::Sun) && seen.insert(i.id.clone()) {
                    rows.push(format!(
                        "{} ({}) — {}",
                        self.name(i),
                        escape(&b.name),
                        self.when(i.at.as_deref().unwrap()).unwrap_or_default()
                    ));
                }
            }
        }
        if rows.is_empty() {
            return;
        }
        let lim = self.limit().max(5);
        self.line(format!("\n## {}", self.s.weekend));
        for r in rows.iter().take(lim) {
            self.line(format!("- {r}"));
        }
        if rows.len() > lim {
            self.line(format!("- {}", fill(self.s.more, rows.len() - lim)));
        }
    }
}

/// Escape characters that would break inline Markdown.
fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '*' | '_' | '`' | '[' | ']' | '<' | '>' | '#' | '|') {
            o.push('\\');
        }
        if c != '\n' && c != '\r' {
            o.push(c);
        }
    }
    o
}

pub fn period_line(f: &ActivityFacts, o: &RenderOpts) -> String {
    let r = R {
        o,
        s: o.locale.strings(),
        out: String::new(),
    };
    let fmt = |ts: &str| {
        o.zone
            .parse(ts)
            .map(|t| format!("{} {:02}:{:02}", r.date(t.date()), t.hour(), t.minute()))
            .unwrap_or_else(|| ts.to_string())
    };
    format!("{} – {}", fmt(&f.period.from), fmt(&f.period.to))
}

/// Render facts to Markdown.
pub fn render(f: &ActivityFacts, o: &RenderOpts) -> String {
    let s = o.locale.strings();
    let mut r = R {
        o,
        s,
        out: String::new(),
    };
    r.line(format!("# {}", s.title));
    r.line(format!("_{}_", period_line(f, o)));
    let t = &f.totals;
    let nothing =
        f.boards.iter().all(|b| !b.has_activity()) && !(o.intents.pending && t.pending > 0);
    if nothing {
        r.line(format!("\n{}", s.no_activity));
        if !(o.intents.pending || o.intents.overdue) {
            return r.out;
        }
    }
    if o.detail >= 2 && !nothing {
        let mut parts = Vec::new();
        for (label, n) in [
            (s.completed, t.completed),
            (s.started, t.started),
            (s.created, t.created),
            (s.edited, t.edited),
            (s.deleted, t.deleted),
        ] {
            if n > 0 {
                parts.push(format!("{label}: {n}"));
            }
        }
        if o.intents.pending && t.pending > 0 {
            parts.push(format!("{}: {}", s.pending, t.pending));
        }
        if t.overdue > 0 && (o.detail >= 3 || o.intents.overdue || o.intents.pending) {
            parts.push(format!("{}: {}", s.overdue, t.overdue));
        }
        if !parts.is_empty() {
            r.line(format!("\n**{}**", parts.join(" · ")));
        }
    }
    if o.intents.weekend {
        r.weekend(f);
    }
    for b in &f.boards {
        if !b.has_activity()
            && !(o.intents.pending && !b.pending.is_empty())
            && !(o.intents.overdue && !b.overdue.is_empty())
        {
            continue;
        }
        r.board(b);
    }
    r.out
}

/// Plain-text version for notifications and chat: strips Markdown markers.
pub fn plain_text(md: &str) -> String {
    md.lines()
        .map(|l| {
            let l = l.trim_start_matches('#').trim_start();
            let l = l.replace("**", "").replace("\\", "");
            let l = if l.starts_with('_') && l.ends_with('_') && l.len() > 1 {
                l[1..l.len() - 1].to_string()
            } else {
                l
            };
            l.replacen("- ", "• ", 1)
        })
        .filter(|l| !l.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::facts::{EditedItem, MovedItem, Period, Totals};

    fn it(id: &str, title: &str, at: &str) -> Item {
        Item {
            id: id.into(),
            title: title.into(),
            lane: Some("Done".into()),
            at: Some(at.into()),
            ..Default::default()
        }
    }

    fn facts() -> ActivityFacts {
        let b = BoardFacts {
            id: "b1".into(),
            name: "Work".into(),
            completed: vec![it("c1", "Fix login", "2026-09-27T10:15:00Z")],
            created: (0..8)
                .map(|i| {
                    it(
                        &format!("n{i}"),
                        &format!("Card {i}"),
                        "2026-09-29T09:00:00Z",
                    )
                })
                .collect(),
            moved: vec![MovedItem {
                item: it("c1", "Fix login", "2026-09-27T10:15:00Z"),
                from: Some("Doing".into()),
                to: Some("Done".into()),
            }],
            edited: vec![EditedItem {
                item: it("c2", "Docs", "2026-09-29T11:00:00Z"),
                edits: 2,
                chars: 140,
            }],
            pending: vec![Item {
                id: "p1".into(),
                title: "Ship v2".into(),
                lane: Some("Doing".into()),
                priority: Some("high".into()),
                due: Some("2026-10-02".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        ActivityFacts {
            period: Period {
                from: "2026-09-26T00:00:00Z".into(),
                to: "2026-09-29T23:59:00Z".into(),
            },
            totals: Totals {
                boards: 1,
                completed: 1,
                created: 8,
                moved: 1,
                edited: 1,
                pending: 1,
                ..Default::default()
            },
            boards: vec![b],
        }
    }

    fn opts(detail: u8, locale: Locale, prompt: &str) -> RenderOpts {
        RenderOpts {
            locale,
            detail,
            links: false,
            intents: detect_intents(prompt),
            zone: Zone::Utc,
        }
    }

    #[test]
    fn detail_levels_grow() {
        let f = facts();
        let lens: Vec<usize> = (1..=5)
            .map(|d| render(&f, &opts(d, Locale::En, "")).len())
            .collect();
        assert!(lens.windows(2).all(|w| w[0] <= w[1]), "{lens:?}");
        let d1 = render(&f, &opts(1, Locale::En, ""));
        assert!(d1.contains("**Completed (1)**: Fix login"));
        assert!(d1.contains("+5 more"));
        assert!(!d1.contains("###"));
        let d5 = render(&f, &opts(5, Locale::En, ""));
        assert!(d5.contains("Fix login: Doing → Done"));
        assert!(d5.contains("Sun 10:15"));
        assert!(d5.contains("(2 sessions, +140 chars)"));
        assert!(d5.contains("### Pending (1)"));
    }

    #[test]
    fn prompt_intents_shape_output() {
        let f = facts();
        let p = "For this monday summary I want to see pending stuff from past week and stuff that might have changed over the weekend as well as a breakdown of this week priorities";
        let i = detect_intents(p);
        assert!(i.pending && i.priorities && i.weekend);
        let md = render(&f, &opts(2, Locale::En, p));
        assert!(md.contains("## Over the weekend"));
        assert!(md.contains("### Priorities (1)"));
        assert!(md.contains("Ship v2 — priority high, due 2026-10-02, Doing"));
        assert!(md.contains("### Pending (1)"));
        let plain = render(&f, &opts(2, Locale::En, "what did I do"));
        assert!(!plain.contains("Pending"));
    }

    #[test]
    fn localized() {
        let f = facts();
        let es = render(
            &f,
            &opts(3, Locale::Es, "tareas pendientes y fin de semana"),
        );
        assert!(es.starts_with("# Resumen de actividad"));
        assert!(es.contains("### Completadas (1)"));
        assert!(es.contains("## Durante el fin de semana"));
        assert!(es.contains("### Pendientes (1)"));
        let pt = render(&f, &opts(3, Locale::Pt, ""));
        assert!(pt.contains("### Concluídos (1)"));
        assert!(pt.contains("sáb 26 set 2026"));
    }

    #[test]
    fn empty_and_links_and_plain() {
        let mut f = facts();
        f.boards.clear();
        f.totals = Totals::default();
        assert!(render(&f, &opts(3, Locale::En, "")).contains("No activity in this period."));
        let f = facts();
        let md = render(
            &f,
            &RenderOpts {
                links: true,
                ..opts(3, Locale::En, "")
            },
        );
        assert!(md.contains("[[c1]]"));
        let txt = plain_text(&render(&f, &opts(1, Locale::En, "")));
        assert!(txt.starts_with("Activity summary\n"));
        assert!(!txt.contains("**"));
        assert_eq!(escape("a*b_[x]"), "a\\*b\\_\\[x\\]");
    }
}
