#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod badges;
mod bot;
mod hypesquad;
mod processes;
mod token_extract;
mod updater;

use badges::BadgeState;
use bot::BotState;
use tauri::Manager;

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            updater::cleanup_stale_files();
            app.manage(BotState::new(app.handle().clone()));
            app.manage(BadgeState::new());
            let handle = app.handle().clone();
            let child = {
                let state = app.state::<BotState>();
                state.child.clone()
            };
            let adopted = {
                let state = app.state::<BotState>();
                state.adopted.clone()
            };
            tauri::async_runtime::spawn(bot::watch_state(handle, child.clone(), adopted.clone()));
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                bot::resume_or_start(child, adopted, app_handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bot::app_config,
            bot::write_env_multi,
            bot::account_profile,
            bot::scan_tokens,
            bot::start_bot,
            bot::restart_bot,
            bot::stop_bot,
            bot::bot_status,
            badges::forge_badges,
            badges::badge_status,
            badges::stop_badges,
            hypesquad::hypesquad_status,
            hypesquad::set_hypesquad,
            hypesquad::remove_hypesquad,
            processes::quit_discord,
            updater::check_update,
            updater::download_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|_, _| {});
}
