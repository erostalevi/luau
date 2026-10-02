//! "Set up local AI": get Ollama running and a model downloaded.
//!
//! 1. **check**: a server already answering on `localhost:11434` is used as is.
//! 2. **start**: an existing install (the macOS app, `ollama` on `PATH`, or a
//!    copy this setup installed before) is started.
//! 3. **download**: otherwise the official release asset for this platform is
//!    fetched from `github.com/ollama/ollama` (latest release, HTTPS only).
//! 4. **verify**: the file's SHA-256 must match the digest GitHub publishes
//!    for that asset (or the release's `sha256sum.txt`); on macOS the app must
//!    also pass `codesign --verify` and Gatekeeper (`spctl`), i.e. be signed
//!    and notarized. Nothing is installed when any check fails.
//! 5. **install**: macOS → `~/Applications/Ollama.app`; Linux → the app data
//!    folder; Windows → the official installer is opened.
//! 6. **model**: the chosen model is pulled with progress.
//!
//! Progress goes out as `ai.setup` events; the shell shows a notification at
//! the end. Cancellable between and during downloads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::llm::{self, Endpoint, Provider};
use crate::error::{Error, Result};

pub const EV_SETUP: &str = "ai.setup";
const RELEASE_API: &str = "https://api.github.com/repos/ollama/ollama/releases/latest";
/// Only assets under this prefix are accepted.
const OFFICIAL_PREFIX: &str = "https://github.com/ollama/ollama/releases/download/";
const MAX_DOWNLOAD: u64 = 4 * 1024 * 1024 * 1024;

static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn check_cancel() -> Result<()> {
    if CANCEL.load(Ordering::SeqCst) {
        Err(Error::Other("cancelled".into()))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SetupRequest {
    /// Model to download (e.g. `gemma3:4b`); none = just get the server running.
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgress {
    /// `check` | `start` | `download` | `verify` | `install` | `connect` | `model` | `done`
    pub step: String,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupResult {
    pub endpoint: String,
    pub model: Option<String>,
    /// Ollama was installed by this run (vs. found already).
    pub installed: bool,
}

/// Release asset for this platform.
pub fn asset_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", _) => Some("Ollama-darwin.zip"),
        ("linux", "x86_64") => Some("ollama-linux-amd64.tgz"),
        ("linux", "aarch64") => Some("ollama-linux-arm64.tgz"),
        ("windows", _) => Some("OllamaSetup.exe"),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    pub sha256: Option<String>,
}

/// Pick `name` from a GitHub release JSON; only official download URLs pass.
pub fn find_asset(release: &Value, name: &str) -> Result<Asset> {
    let assets = release
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Other("unexpected answer from GitHub".into()))?;
    let a = assets
        .iter()
        .find(|a| a.get("name").and_then(Value::as_str) == Some(name))
        .ok_or_else(|| Error::Other(format!("the latest Ollama release has no {name}")))?;
    let url = a
        .get("browser_download_url")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !url.starts_with(OFFICIAL_PREFIX) {
        return Err(Error::Other(
            "refusing a download outside the official Ollama releases".into(),
        ));
    }
    let sha256 = a
        .get("digest")
        .and_then(Value::as_str)
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| is_hex64(h))
        .map(str::to_ascii_lowercase);
    Ok(Asset {
        url: url.to_string(),
        size: a.get("size").and_then(Value::as_u64).unwrap_or(0),
        sha256,
    })
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Find `name`'s digest in a `sha256sum.txt` (`<hex>  [./]name` lines).
pub fn digest_from_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut it = l.split_whitespace();
        let (h, f) = (it.next()?, it.next()?);
        let f = f.trim_start_matches('*').trim_start_matches("./");
        (f == name && is_hex64(h)).then(|| h.to_ascii_lowercase())
    })
}

fn http() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .user_agent(concat!("Luau/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Other(format!("http client: {e}")))
}

fn net(e: reqwest::Error) -> Error {
    Error::Other(format!("download failed: {}", e.without_url()))
}

async fn ollama_up() -> bool {
    match llm::parse_endpoint(llm::DEFAULT_ENDPOINT) {
        Ok(ep) => llm::list_models(Provider::Ollama, &ep).await.is_ok(),
        Err(_) => false,
    }
}

async fn wait_up(secs: u64) -> bool {
    for _ in 0..secs {
        if ollama_up().await {
            return true;
        }
        if CANCEL.load(Ordering::SeqCst) {
            return false;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    false
}

fn home() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Our own Linux install location.
fn local_bin(data: &Path) -> PathBuf {
    data.join("ollama").join("bin").join("ollama")
}

fn mac_app_candidates() -> Vec<PathBuf> {
    let mut v = vec![PathBuf::from("/Applications/Ollama.app")];
    if let Some(h) = home() {
        v.push(h.join("Applications/Ollama.app"));
    }
    v
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(bin))
        .find(|p| p.is_file())
}

fn spawn_serve(bin: &Path) -> Result<()> {
    std::process::Command::new(bin)
        .arg("serve")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| Error::Other(format!("could not start Ollama: {e}")))
}

fn open_mac_app(app: &Path) -> Result<()> {
    let ok = std::process::Command::new("open")
        .arg("-a")
        .arg(app)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(Error::Other("could not open Ollama".into()))
    }
}

/// Start an Ollama that is installed but not running. Returns whether one was found.
fn start_existing(data: &Path) -> Result<bool> {
    if cfg!(target_os = "macos")
        && let Some(app) = mac_app_candidates().into_iter().find(|p| p.exists())
    {
        open_mac_app(&app)?;
        return Ok(true);
    }
    let bin = which(if cfg!(windows) {
        "ollama.exe"
    } else {
        "ollama"
    })
    .or_else(|| Some(local_bin(data)).filter(|p| p.is_file()));
    if let Some(b) = bin {
        spawn_serve(&b)?;
        return Ok(true);
    }
    Ok(false)
}

async fn download(
    c: &reqwest::Client,
    a: &Asset,
    dest: &Path,
    on: &mut (dyn FnMut(SetupProgress) + Send),
) -> Result<String> {
    if a.size > MAX_DOWNLOAD {
        return Err(Error::Other("the download is unexpectedly large".into()));
    }
    let resp = c.get(&a.url).send().await.map_err(net)?;
    // Redirects are followed, but must stay on HTTPS (https_only) and end at GitHub's CDN.
    let host = resp.url().host_str().unwrap_or_default().to_string();
    if !(host == "github.com" || host.ends_with(".githubusercontent.com")) {
        return Err(Error::Other(
            "refusing a download served from an unexpected host".into(),
        ));
    }
    if !resp.status().is_success() {
        return Err(Error::Other(format!(
            "download failed: HTTP {}",
            resp.status().as_u16()
        )));
    }
    let total = resp.content_length().or(Some(a.size)).filter(|n| *n > 0);
    let mut f = tokio::fs::File::create(dest)
        .await
        .map_err(|e| Error::io(dest, e))?;
    let mut hasher = Sha256::new();
    let mut done: u64 = 0;
    let mut last = std::time::Instant::now();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        check_cancel()?;
        let chunk = chunk.map_err(net)?;
        done += chunk.len() as u64;
        if done > MAX_DOWNLOAD {
            return Err(Error::Other("the download is unexpectedly large".into()));
        }
        hasher.update(&chunk);
        tokio::io::AsyncWriteExt::write_all(&mut f, &chunk)
            .await
            .map_err(|e| Error::io(dest, e))?;
        if last.elapsed() > Duration::from_millis(200) {
            last = std::time::Instant::now();
            on(SetupProgress {
                step: "download".into(),
                completed: Some(done),
                total,
                detail: None,
            });
        }
    }
    tokio::io::AsyncWriteExt::flush(&mut f)
        .await
        .map_err(|e| Error::io(dest, e))?;
    Ok(hex::encode(hasher.finalize()))
}

fn run_ok(cmd: &str, args: &[&std::ffi::OsStr]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Install the verified download. Returns where Ollama now lives.
fn install(file: &Path, data: &Path) -> Result<()> {
    if cfg!(target_os = "macos") {
        let staging = data.join("ai-setup").join("unpacked");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|e| Error::io(&staging, e))?;
        // `ditto` keeps the bundle's symlinks, permissions and signature intact.
        if !run_ok(
            "ditto",
            &[
                "-x".as_ref(),
                "-k".as_ref(),
                file.as_os_str(),
                staging.as_os_str(),
            ],
        ) {
            return Err(Error::Other("could not unpack Ollama".into()));
        }
        let app = staging.join("Ollama.app");
        // Genuine: signed, untampered and notarized by Apple.
        if !run_ok(
            "codesign",
            &[
                "--verify".as_ref(),
                "--deep".as_ref(),
                "--strict".as_ref(),
                app.as_os_str(),
            ],
        ) || !run_ok(
            "spctl",
            &[
                "--assess".as_ref(),
                "--type".as_ref(),
                "execute".as_ref(),
                app.as_os_str(),
            ],
        ) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(Error::Other(
                "the downloaded Ollama failed the signature check; nothing was installed".into(),
            ));
        }
        let apps = home()
            .ok_or_else(|| Error::Other("no home folder".into()))?
            .join("Applications");
        std::fs::create_dir_all(&apps).map_err(|e| Error::io(&apps, e))?;
        let dest = apps.join("Ollama.app");
        if dest.exists() {
            return Err(Error::Other(
                "Ollama.app already exists in ~/Applications; open it and try again".into(),
            ));
        }
        std::fs::rename(&app, &dest).map_err(|e| Error::io(&dest, e))?;
        let _ = std::fs::remove_dir_all(&staging);
        open_mac_app(&dest)
    } else if cfg!(target_os = "linux") {
        let dir = data.join("ollama");
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        if !run_ok(
            "tar",
            &[
                "-xzf".as_ref(),
                file.as_os_str(),
                "-C".as_ref(),
                dir.as_os_str(),
            ],
        ) {
            return Err(Error::Other("could not unpack Ollama".into()));
        }
        spawn_serve(&local_bin(data))
    } else {
        // Windows: the official installer (signed by Ollama) runs with its own UI.
        let ok = std::process::Command::new(file)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err(Error::Other("the Ollama installer did not finish".into()))
        }
    }
}

/// Run the whole setup. `data` is the app data folder.
pub async fn run(
    data: &Path,
    req: &SetupRequest,
    on: &mut (dyn FnMut(SetupProgress) + Send),
) -> Result<SetupResult> {
    CANCEL.store(false, Ordering::SeqCst);
    if let Some(m) = &req.model
        && !llm::valid_model_name(m)
    {
        return Err(Error::invalid("invalid model name"));
    }
    let step = |s: &str| SetupProgress {
        step: s.into(),
        ..Default::default()
    };
    on(step("check"));
    let mut installed = false;
    if !ollama_up().await {
        on(step("start"));
        let started = start_existing(data)?;
        if !(started && wait_up(30).await) {
            check_cancel()?;
            let name = asset_name(std::env::consts::OS, std::env::consts::ARCH)
                .ok_or_else(|| Error::Other("automatic setup isn't available on this system; install Ollama from ollama.com".into()))?;
            let c = http()?;
            let release: Value = c
                .get(RELEASE_API)
                .header("Accept", "application/vnd.github+json")
                .send()
                .await
                .map_err(net)?
                .json()
                .await
                .map_err(net)?;
            let mut asset = find_asset(&release, name)?;
            if asset.sha256.is_none()
                && let Ok(sums) = find_asset(&release, "sha256sum.txt")
            {
                let text = c
                    .get(&sums.url)
                    .send()
                    .await
                    .map_err(net)?
                    .text()
                    .await
                    .map_err(net)?;
                asset.sha256 = digest_from_sums(&text, name);
            }
            let expected = asset.sha256.clone().ok_or_else(|| {
                Error::Other("the release publishes no checksum; nothing was downloaded".into())
            })?;
            let dir = data.join("ai-setup");
            std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
            let file = dir.join(name);
            on(SetupProgress {
                step: "download".into(),
                completed: Some(0),
                total: Some(asset.size),
                detail: Some(name.into()),
            });
            let got = download(&c, &asset, &file, on).await;
            let got = match got {
                Ok(h) => h,
                Err(e) => {
                    let _ = std::fs::remove_file(&file);
                    return Err(e);
                }
            };
            on(step("verify"));
            if got != expected {
                let _ = std::fs::remove_file(&file);
                return Err(Error::Other(
                    "the download doesn't match the official checksum; nothing was installed"
                        .into(),
                ));
            }
            check_cancel()?;
            on(step("install"));
            let res = install(&file, data);
            let _ = std::fs::remove_file(&file);
            res?;
            installed = true;
        }
        on(step("connect"));
        if !wait_up(60).await {
            check_cancel()?;
            return Err(Error::Other(
                "Ollama didn't start; open it once and try again".into(),
            ));
        }
    }
    let ep: Endpoint = llm::parse_endpoint(llm::DEFAULT_ENDPOINT)?;
    if let Some(m) = &req.model {
        on(SetupProgress {
            step: "model".into(),
            detail: Some(m.clone()),
            ..Default::default()
        });
        let already = llm::list_models(Provider::Ollama, &ep)
            .await
            .map(|l| {
                l.iter()
                    .any(|x| x.name == *m || x.name == format!("{m}:latest"))
            })
            .unwrap_or(false);
        if !already {
            llm::pull_model(&ep, m, &mut |p| {
                on(SetupProgress {
                    step: "model".into(),
                    completed: p.completed,
                    total: p.total,
                    detail: Some(m.clone()),
                });
            })
            .await?;
        }
    }
    on(step("done"));
    Ok(SetupResult {
        endpoint: ep.base,
        model: req.model.clone(),
        installed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn assets_and_checksums() {
        assert_eq!(asset_name("macos", "aarch64"), Some("Ollama-darwin.zip"));
        assert_eq!(
            asset_name("linux", "x86_64"),
            Some("ollama-linux-amd64.tgz")
        );
        assert_eq!(asset_name("freebsd", "x86_64"), None);
        let h = "a".repeat(64);
        let rel = json!({"assets": [
            {"name": "Ollama-darwin.zip", "size": 10, "digest": format!("sha256:{h}"),
             "browser_download_url": "https://github.com/ollama/ollama/releases/download/v1/Ollama-darwin.zip"},
            {"name": "evil.zip", "browser_download_url": "https://example.com/evil.zip"}
        ]});
        let a = find_asset(&rel, "Ollama-darwin.zip").unwrap();
        assert_eq!(a.sha256.as_deref(), Some(h.as_str()));
        assert!(find_asset(&rel, "evil.zip").is_err(), "only official URLs");
        assert!(find_asset(&rel, "missing").is_err());
        let sums = format!(
            "{h}  ./ollama-linux-amd64.tgz\n{}  OllamaSetup.exe\n",
            "b".repeat(64)
        );
        assert_eq!(
            digest_from_sums(&sums, "ollama-linux-amd64.tgz").as_deref(),
            Some(h.as_str())
        );
        assert_eq!(
            digest_from_sums(&sums, "OllamaSetup.exe"),
            Some("b".repeat(64))
        );
        assert_eq!(digest_from_sums("nothing", "x"), None);
    }
}
