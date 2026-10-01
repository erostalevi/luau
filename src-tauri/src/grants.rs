//! Path grants. The webview is not trusted with arbitrary file paths: RPCs that
//! read or write user-chosen locations only accept paths that came back from a
//! native dialog opened by the backend (`dialog.*`), plus the app's own data
//! directories and the folders of known boards. A picked folder grants its
//! subtree; a picked file (open or save) grants that file only.

use std::path::{Component, Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use luau_core::app::Core;

use crate::rpc::RpcError;

static GRANTS: LazyLock<Mutex<Vec<(PathBuf, bool)>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Remember a path the user picked. `tree` = folder (subtree allowed).
pub fn grant(path: &Path, tree: bool) {
    let p = normalize(path);
    let mut g = GRANTS.lock().unwrap_or_else(|e| e.into_inner());
    if !g.iter().any(|(q, t)| q == &p && *t == tree) {
        g.push((p, tree));
    }
}

/// Lexical normalization: absolute, no `.`/`..` (a `..` that escapes is kept
/// so the path never matches a grant).
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn under(p: &Path, root: &Path) -> bool {
    p.starts_with(normalize(root))
}

/// Allowed when the path was granted, lives in the app's data/config/logs
/// directories, or (with `boards`) inside a registered board.
pub fn allowed(core: &Core, path: &Path, boards: bool) -> bool {
    if !path.is_absolute() {
        return false;
    }
    let p = normalize(path);
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return false;
    }
    let g = GRANTS.lock().unwrap_or_else(|e| e.into_inner());
    if g.iter()
        .any(|(q, tree)| if *tree { p.starts_with(q) } else { &p == q })
    {
        return true;
    }
    drop(g);
    let paths = &core.paths;
    if [&paths.data, &paths.config, &paths.logs]
        .iter()
        .any(|d| under(&p, d))
    {
        return true;
    }
    boards
        && core
            .registry()
            .boards
            .iter()
            .any(|b| under(&p, Path::new(&b.path)))
}

pub fn require(core: &Core, path: &Path, boards: bool) -> Result<(), RpcError> {
    if allowed(core, path, boards) {
        Ok(())
    } else {
        tracing::warn!("rejected a path that was not picked by the user");
        Err(RpcError {
            code: "forbidden_path".into(),
            message: "This location was not chosen in a file dialog".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_and_subtrees() {
        assert_eq!(
            normalize(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        grant(Path::new("/tmp/luau-grant-test"), true);
        grant(Path::new("/tmp/luau-file.md"), false);
        let g = GRANTS.lock().unwrap();
        let ok = |p: &str| {
            let p = normalize(Path::new(p));
            g.iter()
                .any(|(q, t)| if *t { p.starts_with(q) } else { &p == q })
        };
        assert!(ok("/tmp/luau-grant-test/x/y.md"));
        assert!(!ok("/tmp/luau-grant-test-other/x"));
        assert!(ok("/tmp/luau-file.md"));
        assert!(!ok("/tmp/luau-file.md.bak"));
        assert!(!ok("/tmp/luau-grant-test/../../etc/passwd"));
    }
}
