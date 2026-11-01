use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// A published state file older than this is treated as a dead bot.
const STATE_FRESH_S: f64 = 60.0;
/// Windows: CREATE_NO_WINDOW, so the Python engine never flashes a console.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct BotState {
    pub child: Arc<Mutex<Option<Child>>>,
    pub app: AppHandle,
}

impl BotState {
    pub fn new(app: AppHandle) -> Self {
        Self { child: Arc::new(Mutex::new(None)), app }
    }
}

// ------------------------------------------------------------------ paths

/// Repository root: the checkout in debug builds, and the nearest ancestor of
/// the executable that has a pyproject.toml in release builds.
pub(crate) fn project_root() -> PathBuf {
    if cfg!(debug_assertions) {
        if let Some(dir) = Path::new(env!("CARGO_MANIFEST_DIR")).parent() {
            return dir.to_path_buf();
        }
    }

    let exe = std::env::current_exe().unwrap_or_default();
    let start = exe.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    for dir in start.ancestors() {
        if dir.join("pyproject.toml").is_file() {
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

fn state_path() -> PathBuf {
    config_dir().join(".vc-keeper.state.json")
}

fn venv_python(root: &Path) -> PathBuf {
    let candidates = if cfg!(windows) {
        [".venv/Scripts/python.exe", ".venv/bin/python"]
    } else {
        [".venv/bin/python", ".venv/Scripts/python.exe"]
    };
    for candidate in candidates {
        let path = root.join(candidate);
        if path.is_file() {
            return path;
        }
    }
    PathBuf::from("python")
}

/// Locate the standalone, self-contained bot executable (built with PyInstaller).
/// Returns None if not found, in which case the caller falls back to `python -m vc_keeper.app`.
fn bot_exe(root: &Path) -> Option<PathBuf> {
    // Next to the Tauri executable (release bundles place resources alongside the exe).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let candidate = if cfg!(windows) {
                parent.join("discord-toolbox-bot.exe")
            } else {
                parent.join("discord-toolbox-bot")
            };
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    // Next to the project root's dist/ directory (dev builds).
    let candidate = if cfg!(windows) {
        root.join("dist").join("discord-toolbox-bot.exe")
    } else {
        root.join("dist").join("discord-toolbox-bot")
    };
    if candidate.is_file() {
        return Some(candidate);
    }
    None
}

fn python_command(root: &Path) -> Command {
    let mut cmd = Command::new(venv_python(root));
    cmd.current_dir(root)
        .env("PYTHONPATH", root.join("src"))
        .env("PYTHONUNBUFFERED", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Build the command to start the bot. Uses the standalone PyInstaller bundle if
/// available (no Python required on the user's machine), otherwise falls back to
/// `python -m vc_keeper.app` from the venv.
fn bot_command(root: &Path) -> Command {
    if let Some(exe) = bot_exe(root) {
        let mut cmd = Command::new(exe);
        cmd.current_dir(config_dir())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd
    } else {
        let mut cmd = python_command(root);
        cmd.args(["-m", "vc_keeper.app"]);
        cmd
    }
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
    let mut out = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(env_path()) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                out.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    out
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

/// Spawn the Python voice keeper and stream its output to the UI.
///
/// Extracted so it can be called both from the Tauri command and from
/// `main.rs` setup for auto-start-on-launch.
pub async fn start_bot_inner(
    child: Arc<Mutex<Option<Child>>>,
    app: AppHandle,
) -> Result<(), String> {
    let mut guard = child.lock().await;
    if let Some(c) = guard.as_mut() {
        if c.try_wait().ok().flatten().is_none() {
            return Ok(());
        }
    }

    let root = project_root();
    let mut cmd = bot_command(&root);

    let mut child = cmd.spawn().map_err(|e| format!("failed to start the bot: {e}"))?;
    let pid = child.id();
    if let Some(stdout) = child.stdout.take() {
        stream_lines(stdout, "", app.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        stream_lines(stderr, "[stderr] ", app.clone());
    }
    *guard = Some(child);
    drop(guard);

    let _ = app.emit("bot-state", json!({ "running": true, "pid": pid, "sessions": [] }));
    Ok(())
}

#[tauri::command]
pub async fn start_bot(state: State<'_, BotState>) -> Result<(), String> {
    start_bot_inner(state.child.clone(), state.app.clone()).await
}

/// Stop the bot gracefully, force-killing it if it does not exit in time.
#[tauri::command]
pub async fn stop_bot(state: State<'_, BotState>) -> Result<(), String> {
    kill_child(&state).await;
    Ok(())
}

/// Restart the bot with the current .env.
#[tauri::command]
pub async fn restart_bot(state: State<'_, BotState>) -> Result<(), String> {
    kill_child(&state).await;
    start_bot(state).await
}

/// Current process state plus the live data the bot publishes while running.
#[tauri::command]
pub async fn bot_status(state: State<'_, BotState>) -> Result<Value, String> {
    Ok(status_for(&state.child).await)
}

// ------------------------------------------------------------------ bot process

fn stream_lines<R>(reader: R, prefix: &'static str, app: AppHandle)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(mut line)) = lines.next_line().await {
            if !prefix.is_empty() {
                line.insert_str(0, prefix);
            }
            let _ = app.emit("bot-log", line);
        }
    });
}

pub(crate) async fn kill_child(state: &BotState) {
    let mut guard = state.child.lock().await;
    let Some(mut child) = guard.take() else {
        return;
    };

    if let Some(pid) = child.id() {
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
        #[cfg(unix)]
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
    }

    if tokio::time::timeout(Duration::from_secs(5), child.wait()).await.is_err() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    let _ = state.app.emit("bot-state", json!({ "running": false, "sessions": [] }));
}

/// Kill the bot synchronously when the window closes, so it is never orphaned.
pub fn kill_on_exit(state: &BotState) {
    let Ok(mut guard) = state.child.try_lock() else {
        return;
    };
    let Some(mut child) = guard.take() else {
        return;
    };
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
    let _ = child.start_kill();
}

// ------------------------------------------------------------------ live state

async fn current_pid(child: &Arc<Mutex<Option<Child>>>) -> Option<u32> {
    let mut guard = child.lock().await;
    let mut exited = false;
    let pid = match guard.as_mut() {
        Some(running) => match running.try_wait() {
            Ok(Some(_)) => {
                exited = true;
                None
            }
            Ok(None) => running.id(),
            Err(_) => None,
        },
        None => None,
    };
    if exited {
        *guard = None;
    }
    pid
}

fn read_state() -> Option<Value> {
    let raw = std::fs::read_to_string(state_path()).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    let updated = value.get("updated_at")?.as_f64()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs_f64();
    (now - updated <= STATE_FRESH_S).then_some(value)
}

async fn status_for(child: &Arc<Mutex<Option<Child>>>) -> Value {
    let pid = current_pid(child).await;
    // Only trust state published by the child we actually spawned. A onefile
    // PyInstaller bundle restarts itself, so accept its launcher pid too.
    let live = match pid {
        Some(pid) => read_state().filter(|state| {
            let matches = |key: &str| state.get(key).and_then(Value::as_u64) == Some(pid as u64);
            matches("pid") || matches("launcher_pid")
        }),
        None => None,
    };
    let field = |key: &str| live.as_ref().and_then(|state| state.get(key)).cloned();
    let sessions = if pid.is_some() {
        field("sessions").unwrap_or_else(|| json!([]))
    } else {
        json!([])
    };

    json!({
        "running": pid.is_some(),
        "pid": pid,
        "updated_at": field("updated_at"),
        "user": field("user"),
        "sessions": sessions,
        "spoofed_platform": field("spoofed_platform"),
    })
}

/// Poll the child process and its published state, emitting only on changes.
pub async fn watch_state(app: AppHandle, child: Arc<Mutex<Option<Child>>>) {
    let mut last_payload: Option<String> = None;
    let mut was_running = false;

    loop {
        let status = status_for(&child).await;
        let running = status["running"].as_bool().unwrap_or(false);
        if was_running && !running {
            let _ = app.emit("bot-log", "bot process exited");
        }
        was_running = running;

        let payload = status.to_string();
        if last_payload.as_deref() != Some(payload.as_str()) {
            let _ = app.emit("bot-state", &status);
            last_payload = Some(payload);
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
}
