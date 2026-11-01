//! GitHub-release updater: downloads the portable package, swaps the app and
//! bot executables in place, then restarts without running the installer.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const DEFAULT_REPO: &str = "thecloudyy/Discord-Toolbox";
const USER_AGENT: &str = "Discord-Toolbox-Updater";
const UPDATE_TIMEOUT_SECS: u64 = 300;

/// App executable name inside the portable package.
const APP_EXE: &str = "discord-toolbox.exe";
const BOT_EXE: &str = "discord-toolbox-bot.exe";

static DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub current: String,
    pub version: Option<String>,
    pub url: Option<String>,
    pub asset_name: Option<String>,
    pub asset_size: Option<u64>,
}

#[derive(Serialize, Clone)]
struct UpdateProgress {
    phase: &'static str,
    received: u64,
    total: Option<u64>,
    percent: u8,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: Option<u64>,
    browser_download_url: String,
    digest: Option<String>,
}

/// Repository that hosts the release feed. `UPDATE_REPO` in `.env`
/// (`owner/repository`) overrides the built-in default.
async fn update_repo() -> String {
    let configured = crate::bot::app_config()
        .await
        .get("UPDATE_REPO")
        .cloned()
        .unwrap_or_default();
    let trimmed = configured.trim().trim_end_matches('/');
    if trimmed.split('/').count() == 2
        && trimmed.split('/').all(|part| !part.is_empty())
    {
        trimmed.to_string()
    } else {
        DEFAULT_REPO.to_string()
    }
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(UPDATE_TIMEOUT_SECS))
        .build()
        .map_err(|error| format!("could not create the update client: {error}"))
}

async fn fetch_release(client: &reqwest::Client, repo: &str) -> Result<Release, String> {
    let response = client
        .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|error| format!("could not reach GitHub: {error}"))?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("no public release was found for this repository".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "GitHub returned HTTP {}",
            response.status().as_u16()
        ));
    }
    response
        .json::<Release>()
        .await
        .map_err(|error| format!("could not read the GitHub response: {error}"))
}

fn parse_versions(version: &str) -> [u64; 3] {
    let cleaned = version.trim().trim_start_matches('v');
    let core = cleaned.split(['-', '+']).next().unwrap_or(cleaned);
    let mut numbers = [0u64; 3];
    for (index, part) in core.split('.').take(3).enumerate() {
        numbers[index] = part.parse().unwrap_or(0);
    }
    numbers
}

fn is_newer(latest: &str, current: &str) -> bool {
    parse_versions(latest) > parse_versions(current)
}

/// The portable archive carries the app and bot executables for an in-place update.
fn pick_package(assets: &[Asset]) -> Option<&Asset> {
    let find = |predicate: fn(&str) -> bool| {
        assets.iter().find(|asset| predicate(&asset.name.to_ascii_lowercase()))
    };
    find(|name| name.contains("portable") && name.ends_with(".zip"))
        .or_else(|| find(|name| name.ends_with(".zip")))
}

/// Ask the release feed whether a newer build exists.
#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let repo = update_repo().await;
    let client = client()?;
    let release = fetch_release(&client, &repo).await?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let version = release.tag_name.trim_start_matches('v').to_string();
    let package = pick_package(&release.assets);

    Ok(UpdateInfo {
        available: package.is_some() && is_newer(&version, &current),
        current,
        version: Some(version),
        url: release.html_url,
        asset_name: package.map(|asset| asset.name.clone()),
        asset_size: package.and_then(|asset| asset.size),
    })
}

/// Download the newest package, replace this install in place and restart.
/// Progress arrives through the `update-progress` event.
#[tauri::command]
pub async fn download_update(
    app: AppHandle,
    state: State<'_, crate::bot::BotState>,
) -> Result<(), String> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("an update download is already running".into());
    }
    let outcome = download_inner(&app, &state).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    outcome
}

async fn download_inner(
    app: &AppHandle,
    state: &crate::bot::BotState,
) -> Result<(), String> {
    let repo = update_repo().await;
    let client = client()?;
    let release = fetch_release(&client, &repo).await?;
    let current = env!("CARGO_PKG_VERSION");
    let version = release.tag_name.trim_start_matches('v').to_string();
    if !is_newer(&version, current) {
        return Err("this build is already up to date".into());
    }
    let asset = pick_package(&release.assets)
        .ok_or("this release has no portable update package")?;

    let mut response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .map_err(|error| format!("could not download the update: {error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "the update download failed with HTTP {}",
            response.status().as_u16()
        ));
    }

    let total = response.content_length().or(asset.size);
    let mut data: Vec<u8> = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut last_percent = u8::MAX;
    emit_progress(app, "downloading", 0, total, 0);

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("the update download was interrupted: {error}"))?
    {
        data.extend_from_slice(&chunk);
        let received = data.len() as u64;
        let percent = match total {
            Some(total) if total > 0 => ((received.saturating_mul(100) / total).min(100)) as u8,
            _ => 0,
        };
        if percent != last_percent {
            last_percent = percent;
            emit_progress(app, "downloading", received, total, percent);
        }
    }

    if let Some(expected) = asset.digest.as_deref().and_then(|value| value.strip_prefix("sha256:"))
    {
        let actual = hex_digest(&crate::badges::sha256(&data));
        if !actual.eq_ignore_ascii_case(expected) {
            return Err("the downloaded update is corrupted — checksum mismatch".into());
        }
    }
    if !data.starts_with(b"PK\x03\x04") {
        return Err("the downloaded update is not a valid package".into());
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let directory = std::env::temp_dir().join(format!("discord-toolbox-update-{stamp}"));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not prepare the update folder: {error}"))?;
    let package = directory.join("update.zip");
    let mut file = std::fs::File::create(&package)
        .map_err(|error| format!("could not write the update: {error}"))?;
    file.write_all(&data)
        .map_err(|error| format!("could not write the update: {error}"))?;
    drop(file);

    emit_progress(app, "installing", data.len() as u64, total, 100);
    let installed = stage_and_install(&package, &directory, state).await;
    let _ = std::fs::remove_dir_all(&directory);
    let exe = installed?;
    relaunch_after_exit(&exe)?;
    app.exit(0);
    Ok(())
}

async fn stage_and_install(
    zip: &Path,
    directory: &Path,
    state: &crate::bot::BotState,
) -> Result<PathBuf, String> {
    extract_package(zip, directory)?;

    let new_app = directory.join(APP_EXE);
    let new_bot = directory.join(BOT_EXE);
    if !starts_with_pe(&new_app) || !starts_with_pe(&new_bot) {
        return Err("the update package is missing the app files".into());
    }

    crate::bot::kill_child(state).await;
    install_in_place(&new_app, &new_bot)
}

/// PowerShell by its absolute path; PATH is not guaranteed to include System32
/// for a desktop app started from an unusual shell.
fn powershell() -> Command {
    let absolute = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .map(|root| root.join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe"))
        .filter(|path| path.is_file());
    match absolute {
        Some(path) => Command::new(path),
        None => Command::new("powershell.exe"),
    }
}

fn starts_with_pe(path: &Path) -> bool {
    std::fs::read(path)
        .map(|bytes| bytes.starts_with(b"MZ"))
        .unwrap_or(false)
}

fn ps_path(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

/// Unpack the portable zip with the PowerShell that ships with Windows.
fn extract_package(zip: &Path, directory: &Path) -> Result<(), String> {
    let command = format!(
        "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        ps_path(zip),
        ps_path(directory)
    );
    let mut shell = powershell();
    shell
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &command,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    shell.creation_flags(CREATE_NO_WINDOW);
    let status = shell
        .status()
        .map_err(|error| format!("could not unpack the update: {error}"))?;
    if !status.success() {
        return Err("could not unpack the update package".into());
    }
    Ok(())
}

fn copy_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut last = None;
    for attempt in 0..4u64 {
        match std::fs::copy(from, to) {
            Ok(_) => return Ok(()),
            Err(error) => {
                last = Some(error);
                std::thread::sleep(Duration::from_millis(150 * (attempt + 1)));
            }
        }
    }
    Err(last.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::Other, "could not copy the file")
    }))
}

/// Rename the running executables aside and copy the new ones into their place.
/// Windows allows renaming a running image, which is what makes this work.
fn install_in_place(new_app: &Path, new_bot: &Path) -> Result<PathBuf, String> {
    let exe = std::env::current_exe()
        .map_err(|error| format!("could not locate the running app: {error}"))?;
    let directory = exe
        .parent()
        .ok_or("the app has no install folder")?
        .to_path_buf();

    let app_old = directory.join("discord-toolbox.exe.old");
    let bot_target = directory.join(BOT_EXE);
    let bot_old = directory.join("discord-toolbox-bot.exe.old");
    let _ = std::fs::remove_file(&app_old);
    let _ = std::fs::remove_file(&bot_old);

    std::fs::rename(&exe, &app_old).map_err(|error| {
        format!(
            "could not replace the app in {} — close other copies and try again ({error})",
            directory.display()
        )
    })?;
    if let Err(error) = copy_with_retry(new_app, &exe) {
        let _ = std::fs::rename(&app_old, &exe);
        return Err(format!("could not write the new app ({error})"));
    }

    if bot_target.is_file() {
        if let Err(error) = std::fs::rename(&bot_target, &bot_old) {
            let _ = std::fs::rename(&app_old, &exe);
            return Err(format!("could not replace the bots ({error})"));
        }
        if let Err(error) = copy_with_retry(new_bot, &bot_target) {
            let _ = std::fs::rename(&bot_old, &bot_target);
            let _ = std::fs::rename(&app_old, &exe);
            return Err(format!("could not write the new bot ({error})"));
        }
    } else if let Err(error) = copy_with_retry(new_bot, &bot_target) {
        let _ = std::fs::rename(&app_old, &exe);
        return Err(format!("could not write the new bot ({error})"));
    }

    Ok(exe)
}

/// Start the new build once this process has fully exited, so the second
/// instance never races the first for the WebView2 profile.
fn relaunch_after_exit(exe: &Path) -> Result<(), String> {
    let directory = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let command = format!(
        "Wait-Process -Id {} -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 400; Start-Process -FilePath '{}' -WorkingDirectory '{}'",
        std::process::id(),
        ps_path(exe),
        ps_path(&directory)
    );
    let mut shell = powershell();
    shell
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &command,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    shell.creation_flags(CREATE_NO_WINDOW);
    shell
        .spawn()
        .map_err(|error| format!("could not restart the app: {error}"))?;
    Ok(())
}

/// Remove the renamed previous build once Windows has released its image.
pub fn cleanup_stale_files() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(directory) = exe.parent().map(Path::to_path_buf) else {
        return;
    };
    std::thread::spawn(move || {
        for attempt in 0..10u64 {
            let mut pending = false;
            for name in ["discord-toolbox.exe.old", "discord-toolbox-bot.exe.old"] {
                let path = directory.join(name);
                if path.exists() && std::fs::remove_file(&path).is_err() {
                    pending = true;
                }
            }
            if !pending {
                return;
            }
            std::thread::sleep(Duration::from_millis(400 * (attempt + 1)));
        }
    });
}

fn emit_progress(app: &AppHandle, phase: &'static str, received: u64, total: Option<u64>, percent: u8) {
    let _ = app.emit(
        "update-progress",
        UpdateProgress { phase, received, total, percent },
    );
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
