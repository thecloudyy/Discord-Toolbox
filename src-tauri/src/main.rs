#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod badges;
mod config;
mod hypesquad;
mod processes;
mod token_extract;
mod updater;

use badges::BadgeState;
use tauri::Manager;

#[tauri::command]
fn window_minimize(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn window_toggle_maximize(window: tauri::Window) -> bool {
    if window.is_maximized().unwrap_or(false) {
        let _ = window.unmaximize();
        false
    } else {
        let _ = window.maximize();
        true
    }
}

#[tauri::command]
fn window_close(window: tauri::Window) {
    let _ = window.close();
}

#[tauri::command]
fn copy_to_clipboard(text: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::io::Write;
        use std::os::windows::process::CommandExt;
        use std::process::Stdio;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;

        let mut child = std::process::Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "$input | Set-Clipboard",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|error| error.to_string())?;
        }
        let status = child.wait().map_err(|error| error.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("could not access the clipboard".into())
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = text;
        Err("native clipboard copy is only implemented on Windows".into())
    }
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links can be opened".into());
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            updater::cleanup_stale_files();
            app.manage(BadgeState::new());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::app_config,
            config::write_env_multi,
            config::account_profile,
            config::scan_tokens,
            window_minimize,
            window_toggle_maximize,
            window_close,
            copy_to_clipboard,
            open_url,
            badges::forge_badges,
            badges::badge_status,
            badges::badge_logs,
            badges::stop_badges,
            hypesquad::hypesquad_status,
            hypesquad::set_hypesquad,
            hypesquad::remove_hypesquad,
            processes::quit_discord,
            updater::check_update,
            updater::download_update,
            updater::update_progress,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_, _| {});
}
