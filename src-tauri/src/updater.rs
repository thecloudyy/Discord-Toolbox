//! GitHub-release updater: checks the latest release, then downloads the
//! Windows installer and launches it.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

const DEFAULT_REPO: &str = "thecloudyy/Discord-Toolbox";
const USER_AGENT: &str = "Discord-Toolbox-Updater";
const UPDATE_TIMEOUT_SECS: u64 = 300;

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

fn is_windows_installer(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    (lower.ends_with(".msi") || lower.ends_with(".exe")) && !lower.starts_with("unins")
}

fn pick_installer(assets: &[Asset]) -> Option<&Asset> {
    let find = |predicate: fn(&str) -> bool| {
        assets.iter().find(|asset| {
            is_windows_installer(&asset.name) && predicate(&asset.name.to_ascii_lowercase())
        })
    };
    // Regular setup executables are the shipped format; MSI is only a fallback.
    find(|name| name.contains("setup") && name.ends_with(".exe"))
        .or_else(|| find(|name| name.contains("setup") && name.ends_with(".msi")))
        .or_else(|| find(|name| name.ends_with(".exe")))
        .or_else(|| find(|name| name.ends_with(".msi")))
}

/// Ask the release feed whether a newer build exists.
#[tauri::command]
pub async fn check_update() -> Result<UpdateInfo, String> {
    let repo = update_repo().await;
    let client = client()?;
    let release = fetch_release(&client, &repo).await?;
    let current = env!("CARGO_PKG_VERSION").to_string();
    let version = release.tag_name.trim_start_matches('v').to_string();
    let installer = pick_installer(&release.assets);

    Ok(UpdateInfo {
        available: installer.is_some() && is_newer(&version, &current),
        current,
        version: Some(version),
        url: release.html_url,
        asset_name: installer.map(|asset| asset.name.clone()),
        asset_size: installer.and_then(|asset| asset.size),
    })
}

/// Download the newest installer, launch it and exit so the files can be
/// replaced. Progress arrives through the `update-progress` event.
#[tauri::command]
pub async fn download_update(app: AppHandle) -> Result<(), String> {
    if DOWNLOADING.swap(true, Ordering::SeqCst) {
        return Err("an update download is already running".into());
    }
    let outcome = download_inner(&app).await;
    DOWNLOADING.store(false, Ordering::SeqCst);
    outcome
}

async fn download_inner(app: &AppHandle) -> Result<(), String> {
    let repo = update_repo().await;
    let client = client()?;
    let release = fetch_release(&client, &repo).await?;
    let current = env!("CARGO_PKG_VERSION");
    let version = release.tag_name.trim_start_matches('v').to_string();
    if !is_newer(&version, current) {
        return Err("this build is already up to date".into());
    }
    let asset = pick_installer(&release.assets)
        .ok_or("this release has no Windows installer")?;

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
    let is_msi = asset.name.to_ascii_lowercase().ends_with(".msi");
    let looks_valid = if is_msi {
        data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
    } else {
        data.starts_with(b"MZ")
    };
    if !looks_valid {
        return Err("the downloaded update is not a valid installer".into());
    }

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    let directory = std::env::temp_dir().join(format!("discord-toolbox-update-{stamp}"));
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("could not prepare the update folder: {error}"))?;
    let file_name = if is_msi { "Discord-Toolbox-setup.msi" } else { "Discord-Toolbox-setup.exe" };
    let path = directory.join(file_name);
    let mut file = std::fs::File::create(&path)
        .map_err(|error| format!("could not write the update: {error}"))?;
    file.write_all(&data)
        .map_err(|error| format!("could not write the update: {error}"))?;
    drop(file);

    emit_progress(app, "installing", data.len() as u64, total, 100);
    let mut command = if is_msi {
        let mut command = std::process::Command::new("msiexec");
        command.arg("/i").arg(&path);
        command
    } else {
        std::process::Command::new(&path)
    };
    command
        .spawn()
        .map_err(|error| format!("could not start the installer: {error}"))?;
    app.exit(0);
    Ok(())
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
