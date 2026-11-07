//! Headless Discord Toolbox engine.
//!
//! Same gateway implementation as the desktop app, without a window. Reads
//! `.env` from the current directory, or from `%LOCALAPPDATA%\Discord Toolbox`.

#[path = "../engine.rs"]
mod engine;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

const EXIT_CONFIG: i32 = 78;

fn env_path() -> Option<std::path::PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let local = cwd.join(".env");
    if local.is_file() {
        return Some(local);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))?;
    let path = base.join("Discord Toolbox").join(".env");
    path.is_file().then_some(path)
}

fn load_env(path: &std::path::Path) -> HashMap<String, String> {
    std::fs::read_to_string(path)
        .map(|content| engine::parse_env(&content))
        .unwrap_or_default()
}

#[tokio::main]
async fn main() {
    let Some(path) = env_path() else {
        eprintln!("ERROR   discord-toolbox: no .env found in the current directory");
        std::process::exit(EXIT_CONFIG);
    };
    let config = match engine::Config::from_map(&load_env(&path)) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("ERROR   discord-toolbox: config error: {error}");
            std::process::exit(EXIT_CONFIG);
        }
    };

    let shared = Arc::new(engine::Shared::new(
        Arc::new(RwLock::new(engine::Snapshot::default())),
        Arc::new(Mutex::new(engine::LogStore::default())),
        config.log_level.clone(),
    ));
    let cancel = Arc::new(AtomicBool::new(false));

    tokio::select! {
        _ = engine::run(config, shared, cancel.clone(), path) => {}
        _ = tokio::signal::ctrl_c() => {
            cancel.store(true, Ordering::SeqCst);
            eprintln!("INFO    discord-toolbox: shutting down");
        }
    }
}
