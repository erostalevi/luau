//! Background task lifecycle events (`CoreEvent::Task`) for the status light.
//!
//! Every task that tells the UI it `started` must also tell it that it
//! `finished` or `failed`. [`TaskGuard`] makes that structural: the closing
//! event is emitted from `Drop`, so early returns, `?` and panics all close the
//! task. The core keeps the last event of each task so a window that loads
//! later (or reloads) can ask for the current state instead of guessing.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskPhase {
    Started,
    Progress,
    /// Running, but blocked on something slow (a folder that does not answer,
    /// typically a macOS privacy prompt). `issues[0]` says what.
    Waiting,
    Finished,
    Failed,
}

impl TaskPhase {
    pub fn is_closing(self) -> bool {
        matches!(self, TaskPhase::Finished | TaskPhase::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueLevel {
    Warn,
    Error,
}

/// Something worth telling the user. `code` is an i18n key suffix
/// (`status.issues.<code>`); `subject` is a board name or a folder's last
/// component, never a full path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIssue {
    pub level: IssueLevel,
    pub code: String,
    pub subject: Option<String>,
}

impl TaskIssue {
    pub fn warn(code: &str, subject: Option<String>) -> Self {
        TaskIssue {
            level: IssueLevel::Warn,
            code: code.into(),
            subject,
        }
    }
    pub fn error(code: &str, subject: Option<String>) -> Self {
        TaskIssue {
            level: IssueLevel::Error,
            code: code.into(),
            subject,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub task: String,
    pub phase: TaskPhase,
    pub done: Option<usize>,
    pub total: Option<usize>,
    /// Short label (current board name); never a path.
    pub detail: Option<String>,
    pub issues: Vec<TaskIssue>,
}

pub type Emit = Arc<dyn Fn(TaskEvent) + Send + Sync>;

/// Last event per task, for windows that subscribe after the fact.
#[derive(Default)]
pub struct TaskBoard(Mutex<BTreeMap<String, TaskEvent>>);

impl TaskBoard {
    pub fn record(&self, e: &TaskEvent) {
        self.0.lock().insert(e.task.clone(), e.clone());
    }
    pub fn last(&self, task: &str) -> Option<TaskEvent> {
        self.0.lock().get(task).cloned()
    }
    pub fn snapshot(&self) -> Vec<TaskEvent> {
        self.0.lock().values().cloned().collect()
    }
}

/// Emits `started` on creation and exactly one `finished` / `failed` when
/// dropped (also during a panic).
pub struct TaskGuard {
    emit: Emit,
    task: String,
    issues: Vec<TaskIssue>,
    failure: Option<String>,
}

impl TaskGuard {
    pub fn start(emit: Emit, task: &str, total: Option<usize>) -> Self {
        let g = TaskGuard {
            emit,
            task: task.into(),
            issues: Vec::new(),
            failure: None,
        };
        g.send(TaskPhase::Started, Some(0), total, None, Vec::new());
        g
    }

    fn send(
        &self,
        phase: TaskPhase,
        done: Option<usize>,
        total: Option<usize>,
        detail: Option<String>,
        issues: Vec<TaskIssue>,
    ) {
        (self.emit)(TaskEvent {
            task: self.task.clone(),
            phase,
            done,
            total,
            detail,
            issues,
        });
    }

    pub fn progress(&self, done: usize, total: Option<usize>, detail: Option<String>) {
        self.send(TaskPhase::Progress, Some(done), total, detail, Vec::new());
    }

    /// Report that the task is blocked on `why` (cleared by the next progress).
    pub fn waiting(&self, done: usize, why: TaskIssue) {
        self.send(TaskPhase::Waiting, Some(done), None, None, vec![why]);
    }

    /// Record an issue; it is reported with the closing event.
    pub fn issue(&mut self, i: TaskIssue) {
        if !self.issues.contains(&i) {
            self.issues.push(i);
        }
    }

    pub fn issues(&self) -> &[TaskIssue] {
        &self.issues
    }

    /// Close as failed with an error code (`status.issues.<code>`).
    pub fn fail(mut self, code: &str) {
        self.failure = Some(code.into());
    }

    /// Close normally (same as dropping; reads better at call sites).
    pub fn finish(self) {}
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        let mut issues = std::mem::take(&mut self.issues);
        let failure = if std::thread::panicking() {
            Some("taskCrashed".to_string())
        } else {
            self.failure.take()
        };
        let phase = match failure {
            Some(code) => {
                issues.push(TaskIssue::error(&code, None));
                TaskPhase::Failed
            }
            None => TaskPhase::Finished,
        };
        self.send(phase, None, None, None, issues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collector() -> (Emit, Arc<Mutex<Vec<TaskEvent>>>) {
        let v = Arc::new(Mutex::new(Vec::new()));
        let v2 = v.clone();
        (Arc::new(move |e| v2.lock().push(e)), v)
    }

    fn phases(v: &Mutex<Vec<TaskEvent>>) -> Vec<TaskPhase> {
        v.lock().iter().map(|e| e.phase).collect()
    }

    #[test]
    fn start_is_paired_with_finish() {
        let (emit, v) = collector();
        let mut g = TaskGuard::start(emit, "index", Some(2));
        g.progress(1, Some(2), Some("Work".into()));
        g.issue(TaskIssue::warn("indexFailed", Some("Work".into())));
        g.issue(TaskIssue::warn("indexFailed", Some("Work".into())));
        g.finish();
        assert_eq!(
            phases(&v),
            vec![TaskPhase::Started, TaskPhase::Progress, TaskPhase::Finished]
        );
        let last = v.lock().last().cloned().unwrap();
        assert_eq!(last.issues.len(), 1, "issues are deduplicated");
    }

    #[test]
    fn early_return_still_finishes() {
        let (emit, v) = collector();
        fn step(fail: bool) -> Result<(), ()> {
            if fail { Err(()) } else { Ok(()) }
        }
        fn work(emit: Emit) -> Result<(), ()> {
            let _g = TaskGuard::start(emit, "discovery", None);
            step(true)?;
            step(false)
        }
        assert!(work(emit).is_err());
        assert_eq!(phases(&v), vec![TaskPhase::Started, TaskPhase::Finished]);
    }

    #[test]
    fn fail_emits_failed_with_error_issue() {
        let (emit, v) = collector();
        let g = TaskGuard::start(emit, "discovery", None);
        g.fail("discoveryFailed");
        assert_eq!(phases(&v), vec![TaskPhase::Started, TaskPhase::Failed]);
        let last = v.lock().last().cloned().unwrap();
        assert_eq!(last.issues[0].level, IssueLevel::Error);
        assert_eq!(last.issues[0].code, "discoveryFailed");
    }

    #[test]
    fn panic_emits_failed() {
        let (emit, v) = collector();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _g = TaskGuard::start(emit, "index", None);
            panic!("boom");
        }));
        assert!(r.is_err());
        assert_eq!(phases(&v), vec![TaskPhase::Started, TaskPhase::Failed]);
        assert_eq!(v.lock()[1].issues[0].code, "taskCrashed");
    }

    #[test]
    fn task_board_keeps_last_event_per_task() {
        let b = TaskBoard::default();
        let (emit, v) = collector();
        drop(TaskGuard::start(emit, "index", None));
        for e in v.lock().iter() {
            b.record(e);
        }
        let s = b.snapshot();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].phase, TaskPhase::Finished);
    }

    #[test]
    fn serializes_camel_case() {
        let e = TaskEvent {
            task: "discovery".into(),
            phase: TaskPhase::Waiting,
            done: Some(3),
            total: None,
            detail: None,
            issues: vec![TaskIssue::warn("waitingAccess", Some("Documents".into()))],
        };
        let j = serde_json::to_value(&e).unwrap();
        assert_eq!(j["phase"], "waiting");
        assert_eq!(j["issues"][0]["level"], "warn");
    }
}
