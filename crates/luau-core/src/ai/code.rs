//! Python code cells: runs a cell with the local `python3` (found on PATH or
//! configured), as a subprocess without a shell, in a fresh temp directory,
//! with a scrubbed environment, a timeout and output caps. Matplotlib figures
//! left open by the cell are returned as PNG (base64).
//!
//! Security: a board must be trusted (`code.trust`) before its cells run; the
//! check lives in the service layer. Cell code is never logged.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use crate::error::{Error, Result};

pub const MAX_CODE_BYTES: usize = 200 * 1024;
pub const MAX_STREAM_BYTES: usize = 256 * 1024;
pub const MAX_IMAGES: usize = 8;
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CodeResult {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    /// PNG images, base64 (no `data:` prefix).
    pub images: Vec<String>,
    pub ms: u64,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    /// Output was cut at the size cap.
    pub truncated: bool,
}

/// Normalized language id, or `None` when unsupported.
pub fn language(lang: &str) -> Option<&'static str> {
    match lang.trim().to_ascii_lowercase().as_str() {
        "python" | "python3" | "py" => Some("python"),
        _ => None,
    }
}

/// Cache key for a cell.
pub fn cell_hash(lang: &str, code: &str) -> String {
    crate::fsutil::sha256_hex(format!("{}\u{0}{code}", language(lang).unwrap_or(lang)).as_bytes())
}

const RUNNER: &str = r#"
import os, sys, warnings
warnings.filterwarnings("ignore", message=".*non-interactive.*")
_src_path, _out_dir = sys.argv[1], sys.argv[2]
sys.argv = ["cell"]
with open(_src_path, encoding="utf-8") as _f:
    _src = _f.read()
_rc = 0
try:
    exec(compile(_src, "<cell>", "exec"), {"__name__": "__main__"})
except SystemExit as _e:
    _rc = _e.code if isinstance(_e.code, int) else (0 if _e.code is None else 1)
except BaseException as _e:
    import traceback
    traceback.print_exception(type(_e), _e, _e.__traceback__.tb_next)
    _rc = 1
try:
    _plt = sys.modules.get("matplotlib.pyplot")
    if _plt is not None:
        for _i, _n in enumerate(_plt.get_fignums()[:8]):
            _plt.figure(_n).savefig(os.path.join(_out_dir, "fig%02d.png" % _i), format="png", dpi=110, bbox_inches="tight")
except Exception as _e:
    print("could not save figures: %s" % _e, file=sys.stderr)
sys.stdout.flush()
sys.stderr.flush()
sys.exit(_rc)
"#;

fn exe_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["python3.exe", "python.exe", "py.exe"]
    } else {
        &["python3", "python"]
    }
}

/// Locate python: an explicit configured path, else PATH plus common install dirs
/// (GUI apps on macOS get a minimal PATH).
pub fn find_python(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(c) = configured.map(str::trim).filter(|c| !c.is_empty()) {
        let p = PathBuf::from(c);
        let name_ok = p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.to_ascii_lowercase().starts_with("python") || n == "py.exe");
        return (p.is_absolute() && p.is_file() && name_ok).then_some(p);
    }
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for extra in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        dirs.push(PathBuf::from(extra));
    }
    for d in dirs {
        for n in exe_names() {
            let p = d.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

async fn read_capped<R: tokio::io::AsyncRead + Unpin>(mut r: R, cap: usize) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    let mut buf = [0u8; 8192];
    let mut truncated = false;
    loop {
        match r.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let room = cap.saturating_sub(out.len());
                if n > room {
                    truncated = true;
                }
                out.extend_from_slice(&buf[..n.min(room)]);
                // Keep draining so the child never blocks on a full pipe.
            }
        }
    }
    (out, truncated)
}

fn scratch_dir() -> Result<PathBuf> {
    let d = std::env::temp_dir().join(format!("luau-cell-{}", crate::ids::random_suffix(10)));
    std::fs::create_dir_all(&d).map_err(|e| Error::io(&d, e))?;
    Ok(d)
}

fn collect_images(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("fig") && n.ends_with(".png"))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .take(MAX_IMAGES)
        .filter(|p| std::fs::metadata(p).is_ok_and(|m| m.len() <= MAX_IMAGE_BYTES))
        .filter_map(|p| std::fs::read(p).ok())
        .filter(|b| b.starts_with(b"\x89PNG"))
        .map(|b| base64::engine::general_purpose::STANDARD.encode(b))
        .collect()
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub python: PathBuf,
    pub timeout: Duration,
    /// Persistent matplotlib cache (font list), optional.
    pub mpl_dir: Option<PathBuf>,
}

/// Run a python cell. The caller has already checked trust.
pub async fn run_python(code: &str, opts: &RunOptions) -> Result<CodeResult> {
    if code.len() > MAX_CODE_BYTES {
        return Err(Error::invalid("code cell is too large"));
    }
    let dir = scratch_dir()?;
    let res = run_in(&dir, code, opts).await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
    res
}

async fn run_in(dir: &Path, code: &str, opts: &RunOptions) -> Result<CodeResult> {
    let cell = dir.join("cell.py");
    let runner = dir.join("_luau_runner.py");
    let out_dir = dir.join("out");
    // Async file I/O: this runs on the app's async runtime.
    tokio::fs::create_dir_all(&out_dir)
        .await
        .map_err(|e| Error::io(&out_dir, e))?;
    tokio::fs::write(&cell, code)
        .await
        .map_err(|e| Error::io(&cell, e))?;
    tokio::fs::write(&runner, RUNNER)
        .await
        .map_err(|e| Error::io(&runner, e))?;

    let mut cmd = tokio::process::Command::new(&opts.python);
    cmd.arg("-u").arg(&runner).arg(&cell).arg(&out_dir);
    cmd.current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Scrubbed environment: no app secrets or tokens leak into user code.
    cmd.env_clear();
    for k in [
        "PATH",
        "HOME",
        "USERPROFILE",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "SYSTEMROOT",
        "WINDIR",
        "TMPDIR",
        "TEMP",
        "TMP",
        "VIRTUAL_ENV",
        "CONDA_PREFIX",
    ] {
        if let Some(v) = std::env::var_os(k) {
            cmd.env(k, v);
        }
    }
    cmd.env("MPLBACKEND", "Agg")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONDONTWRITEBYTECODE", "1");
    if let Some(m) = &opts.mpl_dir {
        let _ = std::fs::create_dir_all(m);
        cmd.env("MPLCONFIGDIR", m);
    }

    let start = Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Other(format!("could not start python: {e}")))?;
    let out_task = child
        .stdout
        .take()
        .map(|s| tokio::spawn(read_capped(s, MAX_STREAM_BYTES)));
    let err_task = child
        .stderr
        .take()
        .map(|s| tokio::spawn(read_capped(s, MAX_STREAM_BYTES)));
    let (status, timed_out) = match tokio::time::timeout(opts.timeout, child.wait()).await {
        Ok(s) => (s.ok(), false),
        Err(_) => {
            let _ = child.start_kill();
            (child.wait().await.ok(), true)
        }
    };
    let grab = |t: Option<tokio::task::JoinHandle<(Vec<u8>, bool)>>| async move {
        match t {
            Some(t) => tokio::time::timeout(Duration::from_secs(2), t)
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or_default(),
            None => (Vec::new(), false),
        }
    };
    let (out, t1) = grab(out_task).await;
    let (err, t2) = grab(err_task).await;
    let ms = start.elapsed().as_millis() as u64;
    let exit_code = status.and_then(|s| s.code());
    let mut stderr = String::from_utf8_lossy(&err).into_owned();
    if timed_out {
        if !stderr.is_empty() && !stderr.ends_with('\n') {
            stderr.push('\n');
        }
        stderr.push_str(&format!(
            "Stopped after {} s (time limit).",
            opts.timeout.as_secs()
        ));
    }
    Ok(CodeResult {
        ok: !timed_out && exit_code == Some(0),
        stdout: String::from_utf8_lossy(&out).into_owned(),
        stderr,
        images: collect_images(&out_dir),
        ms,
        exit_code,
        timed_out,
        truncated: t1 || t2,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(timeout: u64) -> Option<RunOptions> {
        find_python(None).map(|python| RunOptions {
            python,
            timeout: Duration::from_secs(timeout),
            mpl_dir: None,
        })
    }

    #[test]
    fn languages_and_hash() {
        assert_eq!(language("Python"), Some("python"));
        assert_eq!(language("py"), Some("python"));
        assert_eq!(language("bash"), None);
        assert_eq!(cell_hash("py", "1"), cell_hash("python", "1"));
        assert_ne!(cell_hash("py", "1"), cell_hash("py", "2"));
    }

    #[test]
    fn configured_python_must_look_like_python() {
        assert!(find_python(Some("/bin/sh")).is_none());
        assert!(find_python(Some("relative/python3")).is_none());
    }

    #[tokio::test]
    async fn runs_python_when_available() {
        let Some(o) = opts(20) else { return };
        let r = run_python(
            "import os\nprint('hi', 6*7)\nprint(os.environ.get('SECRET_TOKEN'))",
            &o,
        )
        .await
        .unwrap();
        assert!(r.ok, "{r:?}");
        assert_eq!(r.stdout, "hi 42\nNone\n");
        let e = run_python("raise ValueError('boom')", &o).await.unwrap();
        assert!(!e.ok);
        assert!(e.stderr.contains("ValueError: boom"));
        assert!(!e.stderr.contains("_luau_runner"));
        let x = run_python("import sys; sys.exit(3)", &o).await.unwrap();
        assert_eq!(x.exit_code, Some(3));
    }

    #[tokio::test]
    async fn times_out_and_caps_output() {
        let Some(o) = opts(1) else { return };
        let r = run_python("import time\ntime.sleep(10)", &o).await.unwrap();
        assert!(r.timed_out && !r.ok);
        let Some(o) = opts(20) else { return };
        let big = run_python("print('x' * 600000)", &o).await.unwrap();
        assert!(big.truncated);
        assert_eq!(big.stdout.len(), MAX_STREAM_BYTES);
    }
}
