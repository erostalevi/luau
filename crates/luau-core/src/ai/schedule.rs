//! Scheduled summaries: model, validation, due-time and period computation.
//! Pure (local `NaiveDateTime` in, local `NaiveDateTime` out) so it is fully
//! unit-testable; the scheduler converts to/from the machine's time zone.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CadenceKind {
    #[default]
    Daily,
    /// On the listed ISO weekdays (1 = Monday … 7 = Sunday).
    Weekly,
    /// On `day` of the month (clamped to the month's last day).
    Monthly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Cadence {
    pub kind: CadenceKind,
    /// Local time `HH:MM`.
    pub time: String,
    pub weekdays: Vec<u8>,
    pub day: Option<u8>,
}

impl Default for Cadence {
    fn default() -> Self {
        Cadence {
            kind: CadenceKind::Daily,
            time: "09:00".into(),
            weekdays: vec![],
            day: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PeriodKind {
    /// The previous calendar day.
    #[default]
    LastDay,
    /// The previous working day; on Mondays, Friday through Sunday.
    LastWorkday,
    /// From midnight until the run.
    Today,
    /// The 7 days before the run.
    LastWeek,
    /// The previous calendar week (Monday–Sunday).
    PreviousWeek,
    /// Since the previous run (1 day on the first run).
    SinceLastRun,
    /// The `days` days before the run.
    Days,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct SchedulePeriod {
    pub kind: PeriodKind,
    pub days: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Delivery {
    /// Native notification.
    pub notification: bool,
    pub slack: bool,
    /// `#channel` or a channel id.
    pub slack_channel: Option<String>,
}

impl Default for Delivery {
    fn default() -> Self {
        Delivery {
            notification: true,
            slack: false,
            slack_channel: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Engine {
    /// AI when a local model is available, else the deterministic renderer.
    #[default]
    Auto,
    Ai,
    Basic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub cadence: Cadence,
    pub period: SchedulePeriod,
    /// Board ids; empty = all boards.
    pub boards: Vec<String>,
    pub prompt: String,
    /// 1 (brief) … 5 (exhaustive).
    pub detail: u8,
    pub engine: Engine,
    pub deliver: Delivery,
    /// `en` | `es` | `pt`; empty = app language at run time.
    pub locale: String,
    /// RFC 3339 (UTC) of the creation / last run.
    pub created: Option<String>,
    pub last_run: Option<String>,
    /// `ok` or a short error message.
    pub last_status: Option<String>,
}

impl Default for Schedule {
    fn default() -> Self {
        Schedule {
            id: String::new(),
            name: String::new(),
            enabled: true,
            cadence: Cadence::default(),
            period: SchedulePeriod::default(),
            boards: vec![],
            prompt: String::new(),
            detail: 3,
            engine: Engine::Auto,
            deliver: Delivery::default(),
            locale: String::new(),
            created: None,
            last_run: None,
            last_status: None,
        }
    }
}

pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let (h, m) = s.trim().split_once(':')?;
    NaiveTime::from_hms_opt(h.parse().ok()?, m.parse().ok()?, 0)
}

fn valid_board_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

pub fn valid_slack_channel(s: &str) -> bool {
    let s = s.trim();
    let name = s.strip_prefix('#').unwrap_or(s);
    !name.is_empty()
        && name.len() <= 80
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
        || (s.len() >= 8
            && s.len() <= 24
            && s.starts_with(['C', 'G', 'D'])
            && s.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
}

impl Schedule {
    /// Normalize and validate user input (OWASP: strict input validation).
    pub fn validate(&mut self) -> Result<()> {
        self.name = self
            .name
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(80)
            .collect();
        if self.name.is_empty() {
            return Err(Error::invalid("schedule name is required"));
        }
        let t =
            parse_time(&self.cadence.time).ok_or_else(|| Error::invalid("time must be HH:MM"))?;
        self.cadence.time = t.format("%H:%M").to_string();
        self.cadence.weekdays.retain(|d| (1..=7).contains(d));
        self.cadence.weekdays.sort_unstable();
        self.cadence.weekdays.dedup();
        if self.cadence.kind == CadenceKind::Weekly && self.cadence.weekdays.is_empty() {
            return Err(Error::invalid("pick at least one weekday"));
        }
        if self.cadence.kind == CadenceKind::Monthly {
            let d = self.cadence.day.unwrap_or(1);
            if !(1..=31).contains(&d) {
                return Err(Error::invalid("day of month must be 1–31"));
            }
            self.cadence.day = Some(d);
        }
        if let Some(d) = self.period.days {
            self.period.days = Some(d.clamp(1, 366));
        }
        if self.boards.len() > 500 || !self.boards.iter().all(|b| valid_board_id(b)) {
            return Err(Error::invalid("invalid board list"));
        }
        self.detail = self.detail.clamp(1, 5);
        self.prompt = self.prompt.chars().take(4000).collect();
        self.locale = match self.locale.trim() {
            "en" | "es" | "pt" => self.locale.trim().to_string(),
            _ => String::new(),
        };
        if let Some(c) = &self.deliver.slack_channel {
            let c = c.trim().to_string();
            if c.is_empty() {
                self.deliver.slack_channel = None;
            } else if !valid_slack_channel(&c) {
                return Err(Error::invalid("invalid Slack channel"));
            } else {
                self.deliver.slack_channel = Some(c);
            }
        }
        if self.deliver.slack && self.deliver.slack_channel.is_none() {
            return Err(Error::invalid("pick a Slack channel"));
        }
        Ok(())
    }
}

fn last_day_of_month(y: i32, m: u32) -> u32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(28)
}

fn matches_day(c: &Cadence, d: NaiveDate) -> bool {
    match c.kind {
        CadenceKind::Daily => true,
        CadenceKind::Weekly => c
            .weekdays
            .contains(&(d.weekday().number_from_monday() as u8)),
        CadenceKind::Monthly => {
            let want = u32::from(c.day.unwrap_or(1)).min(last_day_of_month(d.year(), d.month()));
            d.day() == want
        }
    }
}

/// First run strictly after `after` (local time).
pub fn next_run(c: &Cadence, after: NaiveDateTime) -> Option<NaiveDateTime> {
    let t = parse_time(&c.time)?;
    let mut d = after.date();
    for _ in 0..400 {
        let at = d.and_time(t);
        if at > after && matches_day(c, d) {
            return Some(at);
        }
        d = d.succ_opt()?;
    }
    None
}

/// The run that is due at `now`, if any: the first slot after the anchor
/// (last run, else creation) that is not in the future. Missed runs while
/// the app was closed collapse into one catch-up run.
pub fn due_at(s: &Schedule, anchor: NaiveDateTime, now: NaiveDateTime) -> Option<NaiveDateTime> {
    if !s.enabled {
        return None;
    }
    next_run(&s.cadence, anchor).filter(|at| *at <= now)
}

fn midnight(d: NaiveDate) -> NaiveDateTime {
    d.and_hms_opt(0, 0, 0).unwrap_or_default()
}

fn end_of(d: NaiveDate) -> NaiveDateTime {
    d.and_hms_milli_opt(23, 59, 59, 999).unwrap_or_default()
}

/// Local `(from, to)` covered by a run at `at`.
pub fn period_range(
    p: &SchedulePeriod,
    at: NaiveDateTime,
    last_run: Option<NaiveDateTime>,
) -> (NaiveDateTime, NaiveDateTime) {
    let today = at.date();
    let yesterday = today.pred_opt().unwrap_or(today);
    match p.kind {
        PeriodKind::LastDay => (midnight(yesterday), end_of(yesterday)),
        PeriodKind::LastWorkday => {
            let from = match today.weekday() {
                Weekday::Mon => today - Duration::days(3),
                Weekday::Sun => today - Duration::days(2),
                _ => yesterday,
            };
            (midnight(from), end_of(yesterday))
        }
        PeriodKind::Today => (midnight(today), at),
        PeriodKind::LastWeek => (at - Duration::days(7), at),
        PeriodKind::PreviousWeek => {
            let this_monday =
                today - Duration::days(i64::from(today.weekday().num_days_from_monday()));
            let prev_monday = this_monday - Duration::days(7);
            (
                midnight(prev_monday),
                end_of(this_monday.pred_opt().unwrap_or(this_monday)),
            )
        }
        PeriodKind::SinceLastRun => (
            last_run
                .filter(|l| *l < at)
                .unwrap_or(at - Duration::days(1)),
            at,
        ),
        PeriodKind::Days => (
            at - Duration::days(i64::from(p.days.unwrap_or(1).clamp(1, 366))),
            at,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(y: i32, m: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, mi, 0)
            .unwrap()
    }

    fn weekly(days: &[u8], time: &str) -> Cadence {
        Cadence {
            kind: CadenceKind::Weekly,
            time: time.into(),
            weekdays: days.to_vec(),
            day: None,
        }
    }

    #[test]
    fn daily_next_run() {
        let c = Cadence {
            time: "09:00".into(),
            ..Default::default()
        };
        // 2026-09-30 is a Wednesday.
        assert_eq!(
            next_run(&c, dt(2026, 9, 30, 8, 0)),
            Some(dt(2026, 9, 30, 9, 0))
        );
        assert_eq!(
            next_run(&c, dt(2026, 9, 30, 9, 0)),
            Some(dt(2026, 10, 1, 9, 0))
        );
    }

    #[test]
    fn weekly_and_monthly() {
        let c = weekly(&[1, 5], "08:30"); // Mon, Fri
        assert_eq!(
            next_run(&c, dt(2026, 9, 30, 12, 0)),
            Some(dt(2026, 10, 2, 8, 30))
        );
        assert_eq!(
            next_run(&c, dt(2026, 10, 2, 9, 0)),
            Some(dt(2026, 10, 5, 8, 30))
        );
        let m = Cadence {
            kind: CadenceKind::Monthly,
            time: "07:00".into(),
            weekdays: vec![],
            day: Some(31),
        };
        assert_eq!(
            next_run(&m, dt(2026, 9, 1, 0, 0)),
            Some(dt(2026, 9, 30, 7, 0))
        );
        assert_eq!(
            next_run(&m, dt(2026, 9, 30, 8, 0)),
            Some(dt(2026, 10, 31, 7, 0))
        );
        let feb = Cadence { day: Some(30), ..m };
        assert_eq!(
            next_run(&feb, dt(2027, 2, 1, 0, 0)),
            Some(dt(2027, 2, 28, 7, 0))
        );
    }

    #[test]
    fn due_computation() {
        let s = Schedule {
            cadence: Cadence {
                time: "09:00".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let anchor = dt(2026, 9, 29, 9, 0); // last run yesterday 09:00
        assert_eq!(due_at(&s, anchor, dt(2026, 9, 30, 8, 59)), None);
        assert_eq!(
            due_at(&s, anchor, dt(2026, 9, 30, 9, 0)),
            Some(dt(2026, 9, 30, 9, 0))
        );
        // App closed for days: one catch-up run.
        assert_eq!(
            due_at(&s, anchor, dt(2026, 10, 5, 12, 0)),
            Some(dt(2026, 9, 30, 9, 0))
        );
        let off = Schedule {
            enabled: false,
            ..s
        };
        assert_eq!(due_at(&off, anchor, dt(2026, 9, 30, 10, 0)), None);
    }

    #[test]
    fn periods() {
        let mon = dt(2026, 9, 28, 9, 0);
        let p = |kind, days| SchedulePeriod { kind, days };
        assert_eq!(
            period_range(&p(PeriodKind::LastDay, None), mon, None),
            (
                dt(2026, 9, 27, 0, 0),
                end_of(NaiveDate::from_ymd_opt(2026, 9, 27).unwrap())
            )
        );
        let (f, _) = period_range(&p(PeriodKind::LastWorkday, None), mon, None);
        assert_eq!(f, dt(2026, 9, 25, 0, 0)); // Friday
        let (f, t) = period_range(
            &p(PeriodKind::PreviousWeek, None),
            dt(2026, 9, 30, 9, 0),
            None,
        );
        assert_eq!(f, dt(2026, 9, 21, 0, 0));
        assert_eq!(t.date(), NaiveDate::from_ymd_opt(2026, 9, 27).unwrap());
        assert_eq!(
            period_range(&p(PeriodKind::Days, Some(3)), mon, None).0,
            dt(2026, 9, 25, 9, 0)
        );
        assert_eq!(
            period_range(
                &p(PeriodKind::SinceLastRun, None),
                mon,
                Some(dt(2026, 9, 26, 9, 0))
            )
            .0,
            dt(2026, 9, 26, 9, 0)
        );
        assert_eq!(
            period_range(&p(PeriodKind::SinceLastRun, None), mon, None).0,
            dt(2026, 9, 27, 9, 0)
        );
        assert_eq!(
            period_range(&p(PeriodKind::Today, None), mon, None),
            (dt(2026, 9, 28, 0, 0), mon)
        );
    }

    #[test]
    fn validation() {
        let mut s = Schedule {
            name: "  Daily\n ".into(),
            cadence: Cadence {
                time: "9:5".into(),
                ..Default::default()
            },
            detail: 9,
            ..Default::default()
        };
        s.validate().unwrap();
        assert_eq!(s.name, "Daily");
        assert_eq!(s.cadence.time, "09:05");
        assert_eq!(s.detail, 5);
        let mut bad = Schedule {
            name: "x".into(),
            cadence: Cadence {
                time: "25:00".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(bad.validate().is_err());
        let mut w = Schedule {
            name: "x".into(),
            cadence: weekly(&[9], "09:00"),
            ..Default::default()
        };
        assert!(w.validate().is_err());
        let mut sl = Schedule {
            name: "x".into(),
            deliver: Delivery {
                slack: true,
                slack_channel: Some("#team-daily".into()),
                notification: false,
            },
            ..Default::default()
        };
        sl.validate().unwrap();
        let mut sl2 = Schedule {
            name: "x".into(),
            deliver: Delivery {
                slack: true,
                slack_channel: Some("#Bad Name!".into()),
                notification: false,
            },
            ..Default::default()
        };
        assert!(sl2.validate().is_err());
        assert!(valid_slack_channel("C01ABCDEF"));
        let mut b = Schedule {
            name: "x".into(),
            boards: vec!["../x".into()],
            ..Default::default()
        };
        assert!(b.validate().is_err());
    }
}
