use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        build_apple_helper();
    }
    tauri_build::build()
}

/// Build the `apple-llm` sidecar (Apple on-device model bridge, see
/// `helpers/apple-llm/main.swift`) into `binaries/apple-llm-<target>`, where
/// Tauri's `externalBin` (tauri.macos.conf.json) picks it up: tauri-build copies
/// it next to the app binary for `cargo run`, the bundler into `Contents/MacOS`.
///
/// Apple Silicon only. Deployment target macOS 13 with FoundationModels weak-linked, so the helper
/// starts on any macOS and reports `unsupportedOs` below macOS 26. Without a
/// Swift toolchain a stub is written that reports `unsupportedOs`, so the app
/// still builds (and simply falls back to the other AI providers).
fn build_apple_helper() {
    let src = PathBuf::from("helpers/apple-llm/main.swift");
    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-env-changed=LUAU_SKIP_APPLE_HELPER");
    let target = std::env::var("TARGET").unwrap_or_default();
    let out = PathBuf::from(format!("binaries/apple-llm-{target}"));
    println!("cargo:rerun-if-changed={}", out.display());
    if up_to_date(&src, &out) {
        return;
    }
    let _ = std::fs::create_dir_all("binaries");
    // Apple Intelligence only runs on Apple Silicon: Intel builds get the stub.
    let apple_silicon = target.starts_with("aarch64");
    let skip = std::env::var_os("LUAU_SKIP_APPLE_HELPER").is_some();
    let ok = !skip
        && apple_silicon
        && Command::new("xcrun")
            .args(["swiftc", "-O", "-target", "arm64-apple-macos13.0"])
            .args([
                "-Xlinker",
                "-weak_framework",
                "-Xlinker",
                "FoundationModels",
            ])
            .arg(&src)
            .arg("-o")
            .arg(&out)
            .status()
            .is_ok_and(|s| s.success());
    if !ok {
        println!(
            "cargo:warning=apple-llm helper not built (Intel target, no Swift toolchain or LUAU_SKIP_APPLE_HELPER); Apple on-device AI disabled in this build"
        );
        let stub = "#!/bin/sh\n# Stub: built without a Swift toolchain.\nif [ \"$1\" = availability ]; then echo '{\"status\":\"unsupportedOs\",\"contextSize\":0}'; exit 0; fi\necho '{\"error\":\"unavailable\"}'; exit 1\n";
        let _ = std::fs::write(&out, stub);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o755));
        }
    }
}

fn up_to_date(src: &Path, out: &Path) -> bool {
    let m = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    // A stub (shell script) is always rebuilt: the toolchain may be there now.
    let stub = std::fs::read(out).is_ok_and(|b| b.starts_with(b"#!"));
    !stub && matches!((m(src), m(out)), (Some(s), Some(o)) if o >= s)
}
