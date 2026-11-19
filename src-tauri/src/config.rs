//! Configuration, token discovery and account commands.
//!
//! The app no longer runs a gateway client; it only needs a saved token for
//! badge forging and HypeSquad requests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Repository root: the checkout in debug builds, and the nearest ancestor of
/// the executable that has a Cargo.toml in release builds.
pub(crate) fn project_root() -> PathBuf {
    // In debug builds, CARGO_MANIFEST_DIR points to the crate directory (src-tauri).
    // The project root is its parent.
    if cfg!(debug_assertions) {
        if let Some(dir) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
            if dir.join("Cargo.toml").is_file() {
                return dir.to_path_buf();
            }
        }
    }

    let exe = std::env::current_exe().unwrap_or_default();
    let start = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    for dir in start.ancestors() {
        if dir.join("Cargo.toml").is_file() {
            return dir.to_path_buf();
        }
    }
    start
}

/// Writable directory for `.env` and runtime state. Installed builds may live
/// under Program Files, which a normal user cannot write to, so those fall back
/// to `%LOCALAPPDATA%\Discord Toolbox`.
pub(crate) fn config_dir() -> PathBuf {
    let root = project_root();
    if is_writable(&root) {
        return root;
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from));
    let Some(base) = base else {
        return root;
    };
    let dir = base.join("Discord Toolbox");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".write-test");
    let writable = std::fs::File::create(&probe).is_ok();
    let _ = std::fs::remove_file(&probe);
    writable
}

fn env_path() -> PathBuf {
    config_dir().join(".env")
}

// ------------------------------------------------------------------ commands

/// Scan local Discord clients for tokens and enrich them with their profile.
#[tauri::command]
pub async fn scan_tokens() -> crate::token_extract::ScanReport {
    crate::token_extract::scan_with_profiles().await
}

/// Fetch a token's Discord profile, so a manually entered token can be checked.
#[tauri::command]
pub async fn account_profile(token: String) -> Result<crate::token_extract::Profile, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("token is empty".into());
    }
    crate::token_extract::fetch_profile(&token).await
}

/// Load .env (KEY=value lines, ignoring blanks and comments).
#[tauri::command]
pub async fn app_config() -> HashMap<String, String> {
    match std::fs::read_to_string(env_path()) {
        Ok(content) => parse_env(&content),
        Err(_) => HashMap::new(),
    }
}

/// Write/update keys in .env without clobbering the rest of the file.
#[tauri::command]
pub async fn write_env_multi(entries: HashMap<String, String>) -> Result<(), String> {
    let path = env_path();
    let mut lines: Vec<String> = match std::fs::read_to_string(&path) {
        Ok(content) => content.lines().map(str::to_string).collect(),
        Err(_) => Vec::new(),
    };

    let mut seen: Vec<String> = Vec::new();
    for line in lines.iter_mut() {
        if let Some((key, _)) = line.split_once('=') {
            let key = key.trim().to_string();
            if let Some(value) = entries.get(&key) {
                *line = format!("{key}={value}");
                seen.push(key);
            }
        }
    }
    for (key, value) in &entries {
        if !seen.iter().any(|seen| seen == key) {
            lines.push(format!("{key}={value}"));
        }
    }

    std::fs::write(&path, lines.join("\n") + "\n").map_err(|e| e.to_string())
}

fn parse_env(content: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim();
        if value.len() >= 2 {
            let first = value.as_bytes()[0];
            let last = value.as_bytes()[value.len() - 1];
            if first == last && (first == b'"' || first == b'\'') {
                value = &value[1..value.len() - 1];
            }
        }
        if !key.is_empty() {
            values.insert(key.to_string(), value.to_string());
        }
    }
    values
}
