//! Background scheduler: a single thread that wakes every 30 s, finds the
//! schedules that are due (see [`super::schedule::due_at`]) and runs them one
//! at a time. Missed runs while the app was closed collapse into one catch-up.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{Local, NaiveDateTime};

use super::schedule::{Schedule, due_at};
use super::service::{Notifier, utc_to_local};
use crate::app::Core;

const TICK: Duration = Duration::from_secs(30);
static STARTED: AtomicBool = AtomicBool::new(false);

/// Ids of the schedules due at `now` (local time). Pure; unit-tested.
pub fn due_ids(list: &[Schedule], now: NaiveDateTime) -> Vec<String> {
    list.iter()
        .filter(|s| {
            let anchor = s
                .last_run
                .as_deref()
                .or(s.created.as_deref())
                .and_then(utc_to_local)
                .unwrap_or(now);
            due_at(s, anchor, now).is_some()
        })
        .map(|s| s.id.clone())
        .collect()
}

/// Start the scheduler thread (idempotent: only the first call starts it).
pub fn start(core: Arc<Core>, notifier: Option<Notifier>) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("lull-scheduler".into())
        .spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                tracing::warn!("scheduler: could not build runtime");
                return;
            };
            // Let the app finish starting before the first catch-up run.
            std::thread::sleep(Duration::from_secs(20));
            loop {
                let now = Local::now().naive_local();
                for id in due_ids(&core.ai_store().schedules(), now) {
                    match rt.block_on(core.run_schedule(&id, notifier.as_ref())) {
                        Ok(_) => tracing::info!("scheduled summary {id} ran"),
                        Err(e) => tracing::warn!("scheduled summary {id} failed: {}", e.code()),
                    }
                }
                std::thread::sleep(TICK);
            }
        });
    if spawned.is_err() {
        STARTED.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::schedule::{Cadence, CadenceKind};
    use crate::ai::service::local_to_utc;
    use chrono::NaiveDate;

    fn at(d: u32, h: u32, m: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 9, d)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
    }

    #[test]
    fn due_after_slot_and_not_twice() {
        let mut s = Schedule {
            id: "h1".into(),
            name: "Daily".into(),
            cadence: Cadence {
                kind: CadenceKind::Daily,
                time: "09:00".into(),
                ..Default::default()
            },
            created: Some(local_to_utc(at(28, 12, 0))),
            ..Default::default()
        };
        assert!(due_ids(std::slice::from_ref(&s), at(29, 8, 59)).is_empty());
        assert_eq!(
            due_ids(std::slice::from_ref(&s), at(29, 9, 0)),
            vec!["h1".to_string()]
        );
        // Ran at 09:00 on the 29th: not due again until the 30th.
        s.last_run = Some(local_to_utc(at(29, 9, 0)));
        assert!(due_ids(std::slice::from_ref(&s), at(29, 18, 0)).is_empty());
        // App closed for days: exactly one catch-up.
        assert_eq!(due_ids(std::slice::from_ref(&s), at(30, 9, 5)).len(), 1);
        s.enabled = false;
        assert!(due_ids(&[s], at(30, 9, 5)).is_empty());
    }
}
