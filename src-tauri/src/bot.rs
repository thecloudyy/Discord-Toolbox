//! Bot lifecycle and configuration commands.
//!
//! The engine is now native Rust (see `engine.rs`), so there is no Python child
//! process, no bundled bot executable and no state file to poll.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::State;
use tokio::task::JoinHandle;

use crate::engine::{self, Config, LogStore, Shared, Snapshot};

pub struct Running {
    cancel: Arc<AtomicBool>,
    task: JoinHandle<()>,
}

pub struct BotState {
    running: tokio::sync::Mutex<Option<Running>>,
    snapshot: Arc<RwLock<Snapshot>>,
    logs: Arc<Mutex<LogStore>>,
}

impl BotState {
    pub fn new() -> Self {
        Self {
            running: tokio::sync::Mutex::new(None),
            snapshot: Arc::new(RwLock::new(Snapshot::default())),
            logs: Arc::new(Mutex::new(LogStore::default())),
        }
    }
}

// ------------------------------------------------------------------ paths

/// Repository root: the checkout in debug builds, and the nearest ancestor of
/// the executable that has a Cargo.toml in release builds.
pub(crate) fn project_root() -> PathBuf {
    if cfg!(debug_assertions) {
        if let Some(dir) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
            return dir.to_path_buf();
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
        Ok(content) => engine::parse_env(&content),
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

/// Start the native gateway engine with the current .env.
#[tauri::command]
pub async fn start_bot(state: State<'_, BotState>) -> Result<(), String> {
    start_inner(&state).await
}

/// Stop the engine and clear its live state.
#[tauri::command]
pub async fn stop_bot(state: State<'_, BotState>) -> Result<(), String> {
    stop_inner(&state).await;
    Ok(())
}

/// Restart the engine with the current .env.
#[tauri::command]
pub async fn restart_bot(state: State<'_, BotState>) -> Result<(), String> {
    stop_inner(&state).await;
    start_inner(&state).await
}

/// Current engine state plus the live sessions it reports.
#[tauri::command]
pub async fn bot_status(state: State<'_, BotState>) -> Result<Value, String> {
    Ok(status_value(&state.snapshot))
}

/// Incremental engine log lines: `after` is the last sequence already shown.
#[tauri::command]
pub async fn bot_logs(state: State<'_, BotState>, after: u64) -> Result<Value, String> {
    let store = state.logs.lock().map_err(|_| "log store poisoned".to_string())?;
    let count = store.lines.len() as u64;
    let first = store.seq.saturating_sub(count) + 1;
    let lines: Vec<Value> = store
        .lines
        .iter()
        .enumerate()
        .map(|(index, line)| (first + index as u64, line))
        .filter(|(seq, _)| *seq > after)
        .map(|(seq, line)| json!({ "seq": seq, "line": line }))
        .collect();
    Ok(json!({ "lines": lines, "next": store.seq }))
}

#[tauri::command]
pub async fn clear_bot_logs(state: State<'_, BotState>) -> Result<(), String> {
    if let Ok(mut store) = state.logs.lock() {
        store.lines.clear();
    }
    Ok(())
}

async fn start_inner(state: &BotState) -> Result<(), String> {
    let mut guard = state.running.lock().await;
    if let Some(running) = guard.as_ref() {
        if !running.task.is_finished() {
            return Ok(());
        }
    }

    let env = app_config().await;
    let config = Config::from_map(&env)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Shared::new(
        state.snapshot.clone(),
        state.logs.clone(),
        config.log_level.clone(),
    ));
    let task = tokio::spawn(engine::run(
        config,
        shared,
        cancel.clone(),
        env_path(),
    ));

    *guard = Some(Running { cancel, task });
    Ok(())
}

async fn stop_inner(state: &BotState) {
    let mut guard = state.running.lock().await;
    if let Some(running) = guard.take() {
        running.cancel.store(true, Ordering::SeqCst);
        running.task.abort();
    }
    if let Ok(mut snapshot) = state.snapshot.write() {
        snapshot.running = false;
        snapshot.connected = false;
        snapshot.pid = None;
        snapshot.sessions.clear();
        snapshot.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs_f64())
            .unwrap_or(0.0);
    }
}

/// Used by the updater before replacing the running install.
pub(crate) async fn kill_child(state: &BotState) {
    stop_inner(state).await;
}

fn status_value(snapshot: &Arc<RwLock<Snapshot>>) -> Value {
    let snapshot = match snapshot.read() {
        Ok(snapshot) => snapshot.clone(),
        Err(_) => Snapshot::default(),
    };
    json!({
        "running": snapshot.running,
        "connected": snapshot.connected,
        "pid": snapshot.pid,
        "user": snapshot.user,
        "sessions": snapshot.sessions,
        "spoofed_platform": snapshot.spoofed_platform,
        "updated_at": snapshot.updated_at,
    })
}
