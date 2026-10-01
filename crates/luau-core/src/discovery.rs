//! Board discovery: walk search roots looking for `.luau/board.json` markers.
//!
//! Prunes system and heavy folders, never follows symlinks, and stops
//! descending at board roots (nested boards are separate boards).
//!
//! Folders guarded by an OS privacy prompt (macOS Desktop, Documents,
//! Downloads) are walked last, each unit on its own thread under a watchdog,
//! so an unanswered prompt shows up as "waiting" instead of freezing discovery.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::brand::{BOARD_FILE, MARKER_DIR};
use crate::model::{BoardKind, BoardManifest};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub id: String,
    pub name: String,
    pub kind: BoardKind,
    pub path: String,
    /// `board.json` did not parse (the board opens read-only).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub damaged: bool,
}

/// Default folder names skipped everywhere.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    "target",
    "build",
    "dist",
    ".git",
    ".hg",
    ".svn",
    "Library",
    "AppData",
    "Applications",
    "Program Files",
    "Program Files (x86)",
    "Windows",
    "$Recycle.Bin",
    "System Volume Information",
    "venv",
    ".venv",
    "__pycache__",
    ".cache",
    ".Trash",
    "Pictures",
    "Music",
    "Movies",
    "Photos Library.photoslibrary",
    "go",
    "vendor",
    ".npm",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    "snap",
    "proc",
    "sys",
    "dev",
];

pub fn default_roots() -> Vec<PathBuf> {
    dirs::home_dir().into_iter().collect()
}

/// Home sub-folders macOS guards with a privacy (TCC) prompt. Reading them
/// blocks the calling thread until the user answers the prompt.
pub const PROTECTED_NAMES: &[&str] = &["Desktop", "Documents", "Downloads"];

/// Protected folders on this OS (empty outside macOS).
pub fn protected_dirs() -> Vec<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    PROTECTED_NAMES.iter().map(|n| home.join(n)).collect()
}

pub struct Options {
    pub roots: Vec<PathBuf>,
    pub excludes: Vec<String>,
    pub max_depth: usize,
    /// Folders that may trigger an OS privacy prompt (see [`protected_dirs`]).
    pub protected: Vec<PathBuf>,
    /// Scan protected folders found *inside* a root. Roots that are (inside)
    /// a protected folder were chosen explicitly and are always scanned.
    pub include_protected: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            roots: default_roots(),
            excludes: DEFAULT_EXCLUDES.iter().map(|s| s.to_string()).collect(),
            max_depth: 12,
            protected: protected_dirs(),
            include_protected: true,
        }
    }
}

pub fn read_marker(root: &Path) -> Option<Found> {
    let p = root.join(MARKER_DIR).join(BOARD_FILE);
    let text = std::fs::read_to_string(p).ok()?;
    // A damaged manifest still marks a board: salvage it (read-only on open).
    let (m, damaged) = match serde_json::from_str::<BoardManifest>(&text) {
        Ok(m) => (m, false),
        Err(_) => (crate::store::salvage_manifest(root, &text), true),
    };
    Some(Found {
        id: m.id,
        name: m.name,
        kind: m.kind,
        path: root.to_string_lossy().into_owned(),
        damaged,
    })
}

/// One walk of the scan plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub path: PathBuf,
    /// May block on an OS privacy prompt.
    pub protected: bool,
    /// Sub-folders walked by their own (protected) unit, or not at all.
    pub skip: Vec<PathBuf>,
    pub max_depth: usize,
}

impl Unit {
    /// Last path component, for the UI. Never logged; never a full path.
    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }
}

/// Order the roots into walk units: ordinary folders first, protected folders
/// last, so one unanswered privacy prompt cannot hold back the other boards.
/// Protected folders nested in a root are only included when
/// `include_protected` is set; roots that are themselves protected always are.
pub fn plan(opts: &Options) -> Vec<Unit> {
    fn push(list: &mut Vec<Unit>, u: Unit) {
        if !list.iter().any(|x| x.path == u.path) {
            list.push(u);
        }
    }
    let mut normal: Vec<Unit> = Vec::new();
    let mut guarded: Vec<Unit> = Vec::new();
    for root in &opts.roots {
        if opts.protected.iter().any(|p| root.starts_with(p)) {
            push(
                &mut guarded,
                Unit {
                    path: root.clone(),
                    protected: true,
                    skip: Vec::new(),
                    max_depth: opts.max_depth,
                },
            );
            continue;
        }
        let nested: Vec<PathBuf> = opts
            .protected
            .iter()
            .filter(|p| p.starts_with(root) && *p != root)
            .cloned()
            .collect();
        if opts.include_protected {
            for p in &nested {
                let below = p.components().count() - root.components().count();
                push(
                    &mut guarded,
                    Unit {
                        path: p.clone(),
                        protected: true,
                        skip: Vec::new(),
                        max_depth: opts.max_depth.saturating_sub(below),
                    },
                );
            }
        }
        push(
            &mut normal,
            Unit {
                path: root.clone(),
                protected: false,
                skip: nested,
                max_depth: opts.max_depth,
            },
        );
    }
    normal.extend(guarded);
    normal
}

/// Protected folders inside the roots that `plan` leaves out (not opted in).
pub fn excluded_protected(opts: &Options) -> Vec<PathBuf> {
    if opts.include_protected {
        return Vec::new();
    }
    opts.protected
        .iter()
        .filter(|p| opts.roots.iter().any(|r| p.starts_with(r) && *p != r))
        .cloned()
        .collect()
}

/// Why a unit's root could not be walked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootError {
    Missing,
    Denied,
    Other,
}

#[derive(Debug, Default)]
pub struct UnitResult {
    pub found: Vec<Found>,
    pub root_error: Option<RootError>,
    /// Unreadable entries below the root (counted, not reported one by one).
    pub errors: usize,
}

/// Walk one unit. `seen` is bumped for every entry so a watchdog can tell a
/// slow walk from a blocked one.
pub fn walk_unit(unit: &Unit, excludes: &[String], seen: &AtomicUsize) -> UnitResult {
    let mut out = UnitResult::default();
    let mut it = walkdir::WalkDir::new(&unit.path)
        .follow_links(false)
        .max_depth(unit.max_depth)
        .into_iter();
    while let Some(entry) = it.next() {
        seen.fetch_add(1, Ordering::Relaxed);
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                if e.depth() == 0 {
                    out.root_error = Some(match e.io_error().map(|x| x.kind()) {
                        Some(std::io::ErrorKind::NotFound) => RootError::Missing,
                        Some(std::io::ErrorKind::PermissionDenied) => RootError::Denied,
                        _ => RootError::Other,
                    });
                } else {
                    out.errors += 1;
                }
                continue;
            }
        };
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if entry.depth() > 0 {
            let hidden = name.starts_with('.') && name != MARKER_DIR;
            if hidden
                || excludes.iter().any(|x| x.eq_ignore_ascii_case(&name))
                || name.ends_with(".app")
                || unit.skip.iter().any(|s| s == entry.path())
            {
                it.skip_current_dir();
                continue;
            }
        }
        if name == MARKER_DIR {
            it.skip_current_dir();
            continue;
        }
        if let Some(found) = read_marker(entry.path()) {
            out.found.push(found);
            // Boards are leaves for discovery: nested boards are not allowed.
            it.skip_current_dir();
        }
    }
    out
}

/// Walk all roots in plan order on the calling thread and return every board
/// found (no watchdog; for tests and tools).
pub fn scan(opts: &Options) -> Vec<Found> {
    let seen = AtomicUsize::new(0);
    plan(opts)
        .iter()
        .flat_map(|u| walk_unit(u, &opts.excludes, &seen).found)
        .collect()
}

// --- watchdog runner ------------------------------------------------------------

/// Watchdog timings for [`run`].
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// How often the watchdog looks at a unit's progress.
    pub poll: Duration,
    /// No progress for this long: report "waiting".
    pub stall: Duration,
    /// No progress for this long: give the unit up (its thread is left to
    /// finish on its own; late results go to the `late` callback).
    pub give_up: Duration,
    /// Minimum time between progress notices.
    pub progress_every: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            poll: Duration::from_millis(200),
            stall: Duration::from_secs(2),
            give_up: Duration::from_secs(30),
            progress_every: Duration::from_millis(500),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The root does not exist.
    Missing,
    /// The OS refused access (e.g. "Don't Allow" on the privacy prompt).
    Denied,
    /// A protected folder never answered (privacy prompt still open), or was
    /// not started because an earlier one did not answer.
    AccessPending,
    /// An ordinary folder stopped making progress (unresponsive drive or share).
    Stalled,
    /// The walk crashed or failed for another reason.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitProblem {
    pub name: String,
    pub protected: bool,
    pub problem: Problem,
}

#[derive(Debug, Default)]
pub struct Report {
    pub found: Vec<Found>,
    pub problems: Vec<UnitProblem>,
    /// Folders whose boards were not looked at in this run (given up or
    /// skipped). Registry entries below them must not be marked missing, nor
    /// touched (touching them could block on a prompt too).
    pub unscanned: Vec<PathBuf>,
    /// Unreadable entries below the roots.
    pub errors: usize,
}

pub enum Notice<'a> {
    /// Total entries looked at so far in this run.
    Progress { seen: usize },
    /// `unit` made no progress for `Timing::stall`.
    Waiting { unit: &'a Unit, seen: usize },
}

pub type Walker = Arc<dyn Fn(&Unit, &AtomicUsize) -> UnitResult + Send + Sync>;
/// A unit given up on finished after all: its display name and the boards it
/// found (possibly none). Its in-flight slot is already released.
pub type Late = Arc<dyn Fn(&str, Vec<Found>) + Send + Sync>;
/// Units whose walker thread is still alive (possibly from an earlier run).
pub type InFlight = Arc<Mutex<HashSet<PathBuf>>>;

fn problem(unit: &Unit, problem: Problem) -> UnitProblem {
    UnitProblem {
        name: unit.display_name(),
        protected: unit.protected,
        problem,
    }
}

/// Run the plan in order, each unit on its own thread under a watchdog.
///
/// - A unit that stops making progress is reported (`Notice::Waiting`), then
///   given up after `give_up`; its thread is not killed (a blocked syscall
///   cannot be), and if it finishes later its boards go to `late`.
/// - Once a protected unit is given up, the remaining protected units are not
///   started in this run: macOS queues privacy prompts, so they would block
///   too. They are reported as `AccessPending`.
/// - A unit whose thread from an earlier run is still alive is not started
///   again (no pile-up of blocked threads).
pub fn run(
    units: &[Unit],
    walker: Walker,
    timing: &Timing,
    inflight: &InFlight,
    notify: &mut dyn FnMut(Notice),
    late: Late,
) -> Report {
    let mut report = Report::default();
    let mut base = 0usize;
    let mut access_blocked = false;
    let mut last_notice = Instant::now();
    for unit in units {
        let pending = if unit.protected {
            Problem::AccessPending
        } else {
            Problem::Stalled
        };
        if unit.protected && access_blocked {
            report.problems.push(problem(unit, pending));
            report.unscanned.push(unit.path.clone());
            continue;
        }
        if !inflight.lock().insert(unit.path.clone()) {
            access_blocked |= unit.protected;
            report.problems.push(problem(unit, pending));
            report.unscanned.push(unit.path.clone());
            continue;
        }
        let seen = Arc::new(AtomicUsize::new(0));
        // Rendezvous channel: a result is either received or handed back to
        // the sender (never lost between the watchdog giving up and a send).
        let (tx, rx) = std::sync::mpsc::sync_channel::<UnitResult>(0);
        let spawned = {
            let (u, seen, walker, inflight, late) = (
                unit.clone(),
                seen.clone(),
                walker.clone(),
                inflight.clone(),
                late.clone(),
            );
            std::thread::Builder::new()
                .name("luau-discovery".into())
                .spawn(move || {
                    // Release the in-flight slot even if the walker panics.
                    struct Release(InFlight, PathBuf);
                    impl Drop for Release {
                        fn drop(&mut self) {
                            self.0.lock().remove(&self.1);
                        }
                    }
                    let release = Release(inflight, u.path.clone());
                    let res = walker(&u, &seen);
                    if let Err(back) = tx.send(res) {
                        // The watchdog gave up on this unit: deliver late,
                        // after freeing the slot so a rescan may walk it again.
                        drop(release);
                        if back.0.root_error.is_none() {
                            late(&u.display_name(), back.0.found);
                        }
                    }
                })
        };
        if spawned.is_err() {
            inflight.lock().remove(&unit.path);
            report.problems.push(problem(unit, Problem::Failed));
            report.unscanned.push(unit.path.clone());
            continue;
        }
        let mut last_seen = 0usize;
        let mut last_change = Instant::now();
        let mut waiting = false;
        loop {
            match rx.recv_timeout(timing.poll) {
                Ok(res) => {
                    base += seen.load(Ordering::Relaxed);
                    report.errors += res.errors;
                    if let Some(e) = res.root_error {
                        let p = match e {
                            RootError::Missing => Problem::Missing,
                            RootError::Denied => Problem::Denied,
                            RootError::Other => Problem::Failed,
                        };
                        report.problems.push(problem(unit, p));
                    }
                    report.found.extend(res.found);
                    notify(Notice::Progress { seen: base });
                    last_notice = Instant::now();
                    break;
                }
                Err(RecvTimeoutError::Timeout) => {
                    let n = seen.load(Ordering::Relaxed);
                    if n != last_seen {
                        last_seen = n;
                        last_change = Instant::now();
                        if waiting || last_notice.elapsed() >= timing.progress_every {
                            waiting = false;
                            last_notice = Instant::now();
                            notify(Notice::Progress { seen: base + n });
                        }
                    } else if last_change.elapsed() >= timing.give_up {
                        // Blocked before listing anything: the privacy prompt.
                        let blocked_on_access = unit.protected && n <= 1;
                        access_blocked |= blocked_on_access;
                        let p = if blocked_on_access {
                            Problem::AccessPending
                        } else {
                            Problem::Stalled
                        };
                        report.problems.push(problem(unit, p));
                        report.unscanned.push(unit.path.clone());
                        base += n;
                        break; // dropping `rx` routes a late result to `late`
                    } else if !waiting && last_change.elapsed() >= timing.stall {
                        waiting = true;
                        notify(Notice::Waiting {
                            unit,
                            seen: base + n,
                        });
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    // The walker panicked.
                    report.problems.push(problem(unit, Problem::Failed));
                    report.unscanned.push(unit.path.clone());
                    break;
                }
            }
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::BoardStore;

    #[test]
    fn finds_boards_and_skips_excluded() {
        let d = tempfile::tempdir().unwrap();
        BoardStore::create(
            &d.path().join("a/Work"),
            "b00000a".into(),
            "Work".into(),
            BoardKind::Kanban,
        )
        .unwrap();
        BoardStore::create(
            &d.path().join("node_modules/x"),
            "b00000b".into(),
            "Hidden".into(),
            BoardKind::Kanban,
        )
        .unwrap();
        BoardStore::create(
            &d.path().join(".secret/y"),
            "b00000c".into(),
            "Dot".into(),
            BoardKind::Kanban,
        )
        .unwrap();
        BoardStore::create(
            &d.path().join("notes"),
            "b00000d".into(),
            "Notes".into(),
            BoardKind::Files,
        )
        .unwrap();
        let opts = Options {
            roots: vec![d.path().to_path_buf()],
            protected: Vec::new(),
            ..Default::default()
        };
        let mut found: Vec<String> = scan(&opts).into_iter().map(|f| f.name).collect();
        found.sort();
        assert_eq!(found, vec!["Notes", "Work"]);
    }

    fn opts(roots: &[&str], protected: &[&str], include: bool) -> Options {
        Options {
            roots: roots.iter().map(PathBuf::from).collect(),
            excludes: Vec::new(),
            max_depth: 12,
            protected: protected.iter().map(PathBuf::from).collect(),
            include_protected: include,
        }
    }

    const PROT: &[&str] = &["/h/Desktop", "/h/Documents", "/h/Downloads"];

    #[test]
    fn protected_folders_are_planned_last() {
        let units = plan(&opts(&["/h", "/data"], PROT, true));
        let paths: Vec<_> = units.iter().map(|u| u.path.clone()).collect();
        assert_eq!(
            paths,
            ["/h", "/data", "/h/Desktop", "/h/Documents", "/h/Downloads"]
                .map(PathBuf::from)
                .to_vec()
        );
        assert!(!units[0].protected && !units[1].protected);
        assert!(units[2..].iter().all(|u| u.protected));
        // The home walk skips them (they have their own unit).
        assert_eq!(units[0].skip.len(), 3);
        assert_eq!(
            units[2].max_depth, 11,
            "depth budget is relative to the root"
        );
    }

    #[test]
    fn protected_folders_are_skipped_unless_opted_in() {
        let o = opts(&["/h"], PROT, false);
        let units = plan(&o);
        assert_eq!(units.len(), 1);
        assert_eq!(
            units[0].skip.len(),
            3,
            "still never walked by the home unit"
        );
        assert_eq!(excluded_protected(&o).len(), 3);
    }

    #[test]
    fn explicit_protected_root_is_always_scanned_last() {
        let units = plan(&opts(&["/h/Documents/Boards", "/data"], PROT, false));
        assert_eq!(units[0].path, PathBuf::from("/data"));
        assert_eq!(units[1].path, PathBuf::from("/h/Documents/Boards"));
        assert!(units[1].protected);
    }

    #[test]
    fn duplicate_units_are_merged() {
        let units = plan(&opts(&["/h", "/h/Documents"], PROT, true));
        let docs = units
            .iter()
            .filter(|u| u.path == Path::new("/h/Documents"))
            .count();
        assert_eq!(docs, 1);
    }

    #[test]
    fn walk_skips_planned_subfolders_and_reports_missing_root() {
        let d = tempfile::tempdir().unwrap();
        for (p, id) in [("Documents/A", "b0000aa"), ("Other/B", "b0000ab")] {
            BoardStore::create(&d.path().join(p), id.into(), p.into(), BoardKind::Kanban).unwrap();
        }
        let u = Unit {
            path: d.path().to_path_buf(),
            protected: false,
            skip: vec![d.path().join("Documents")],
            max_depth: 12,
        };
        let seen = AtomicUsize::new(0);
        let r = walk_unit(&u, &[], &seen);
        assert_eq!(r.found.len(), 1);
        assert_eq!(r.found[0].name, "Other/B");
        assert!(seen.load(Ordering::Relaxed) > 0);

        let gone = Unit {
            path: d.path().join("nope"),
            ..u
        };
        assert_eq!(
            walk_unit(&gone, &[], &seen).root_error,
            Some(RootError::Missing)
        );
    }

    #[test]
    fn damaged_manifest_is_flagged() {
        let d = tempfile::tempdir().unwrap();
        let b = d.path().join("Broken");
        std::fs::create_dir_all(b.join(MARKER_DIR)).unwrap();
        std::fs::write(b.join(MARKER_DIR).join(BOARD_FILE), "{ not json").unwrap();
        let f = read_marker(&b).unwrap();
        assert!(f.damaged);
    }

    fn found(name: &str) -> Found {
        Found {
            id: format!("b{name}"),
            name: name.into(),
            kind: BoardKind::Kanban,
            path: format!("/x/{name}"),
            damaged: false,
        }
    }

    fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(5),
            stall: Duration::from_millis(30),
            give_up: Duration::from_millis(120),
            progress_every: Duration::from_millis(1),
        }
    }

    /// A blocked protected folder does not hold back the others, is reported
    /// (waiting, then access pending), the other protected folders are not
    /// started, and its boards still arrive late once it unblocks.
    #[test]
    fn blocked_protected_unit_is_given_up_and_delivers_late() {
        let units = plan(&opts(&["/h"], PROT, true));
        let release = Arc::new(std::sync::Barrier::new(2));
        let started = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
        let walker: Walker = {
            let (release, started) = (release.clone(), started.clone());
            Arc::new(move |u: &Unit, seen: &AtomicUsize| {
                started.lock().push(u.path.clone());
                if u.path == Path::new("/h/Desktop") {
                    release.wait(); // "privacy prompt" open until the test answers
                    return UnitResult {
                        found: vec![found("Late")],
                        ..Default::default()
                    };
                }
                seen.fetch_add(5, Ordering::Relaxed);
                UnitResult {
                    found: vec![found("Home")],
                    ..Default::default()
                }
            })
        };
        let (late_tx, late_rx) = std::sync::mpsc::channel::<(String, Vec<Found>)>();
        let late_tx = Mutex::new(late_tx);
        let late: Late = Arc::new(move |name, f| {
            let _ = late_tx.lock().send((name.to_string(), f));
        });
        let inflight: InFlight = Arc::default();
        let mut waits = Vec::new();
        let report = run(
            &units,
            walker,
            &fast(),
            &inflight,
            &mut |n| {
                if let Notice::Waiting { unit, .. } = n {
                    waits.push(unit.display_name());
                }
            },
            late,
        );
        assert_eq!(report.found.len(), 1);
        assert_eq!(report.found[0].name, "Home");
        assert_eq!(waits, vec!["Desktop"]);
        let pending: Vec<_> = report
            .problems
            .iter()
            .filter(|p| p.problem == Problem::AccessPending)
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(pending, vec!["Desktop", "Documents", "Downloads"]);
        assert_eq!(report.unscanned.len(), 3);
        assert_eq!(
            started.lock().len(),
            2,
            "Documents and Downloads were not started behind the blocked prompt"
        );
        // Still blocked: a second run does not start another thread for it.
        assert!(inflight.lock().contains(Path::new("/h/Desktop")));
        // The user answers: the boards arrive through `late`.
        release.wait();
        let (name, got) = late_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(name, "Desktop");
        assert_eq!(got[0].name, "Late");
        assert!(
            !inflight.lock().contains(Path::new("/h/Desktop")),
            "slot released before the late delivery"
        );
    }

    #[test]
    fn slow_but_progressing_unit_is_not_given_up() {
        let units = plan(&opts(&["/data"], &[], true));
        let walker: Walker = Arc::new(|_: &Unit, seen: &AtomicUsize| {
            for _ in 0..30 {
                seen.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(10));
            }
            UnitResult {
                found: vec![found("Slow")],
                ..Default::default()
            }
        });
        let report = run(
            &units,
            walker,
            &fast(),
            &Arc::default(),
            &mut |_| {},
            Arc::new(|_, _| {}),
        );
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert_eq!(report.found[0].name, "Slow");
    }

    #[test]
    fn panicking_walker_is_reported_as_failed() {
        let units = plan(&opts(&["/data"], &[], true));
        let walker: Walker = Arc::new(|_: &Unit, _: &AtomicUsize| panic!("boom"));
        let inflight: InFlight = Arc::default();
        let report = run(
            &units,
            walker,
            &fast(),
            &inflight,
            &mut |_| {},
            Arc::new(|_, _| {}),
        );
        assert_eq!(report.problems[0].problem, Problem::Failed);
        // The slot is released even though the walker panicked.
        std::thread::sleep(Duration::from_millis(50));
        assert!(inflight.lock().is_empty());
    }
}
