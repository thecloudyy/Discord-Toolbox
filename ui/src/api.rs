use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::types::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI_INTERNALS__"], js_name = invoke, catch)]
    fn invoke_raw(cmd: &str, args: JsValue) -> Result<js_sys::Promise, JsValue>;
}

fn js_error(value: JsValue) -> String {
    value
        .as_string()
        .or_else(|| js_sys::JSON::stringify(&value).ok().map(String::from))
        .unwrap_or_else(|| "request failed".into())
}

pub async fn call(cmd: &str, args: impl Serialize) -> Result<Value, String> {
    let payload = serde_wasm_bindgen::to_value(&args).map_err(|error| error.to_string())?;
    let promise = invoke_raw(cmd, payload).map_err(js_error)?;
    let value = JsFuture::from(promise).await.map_err(js_error)?;
    if value.is_null() || value.is_undefined() {
        return Ok(Value::Null);
    }
    let text = js_sys::JSON::stringify(&value)
        .map_err(js_error)?
        .as_string()
        .unwrap_or_else(|| "null".into());
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

async fn typed<T: DeserializeOwned>(cmd: &str, args: impl Serialize) -> Result<T, String> {
    let value = call(cmd, args).await?;
    serde_json::from_value(value).map_err(|error| error.to_string())
}

pub async fn app_config() -> Config {
    typed("app_config", json!({})).await.unwrap_or_default()
}

pub async fn write_env(entries: Config) -> Result<(), String> {
    call("write_env_multi", json!({ "entries": entries })).await.map(|_| ())
}

pub async fn account_profile(token: String) -> Result<Profile, String> {
    typed("account_profile", json!({ "token": token })).await
}

pub async fn scan_tokens() -> Result<ScanReport, String> {
    typed("scan_tokens", json!({})).await
}

pub async fn badge_status() -> BadgeStatus {
    typed("badge_status", json!({})).await.unwrap_or_default()
}

pub async fn badge_logs(after: u64) -> (String, u64) {
    let value = call("badge_logs", json!({ "after": after })).await.unwrap_or(Value::Null);
    read_log_chunk(value, after)
}

pub async fn forge_badges(
    token: String,
    hours: f64,
    games: usize,
    repeat: bool,
) -> Result<(), String> {
    call(
        "forge_badges",
        json!({ "token": token, "hours": hours, "games": games, "repeat": repeat }),
    )
    .await
    .map(|_| ())
}

pub async fn stop_badges() {
    let _ = call("stop_badges", json!({})).await;
}

pub async fn hypesquad_status(token: String) -> Result<HypeSquadStatus, String> {
    typed("hypesquad_status", json!({ "token": token })).await
}

pub async fn set_hypesquad(token: String, target: String) -> Result<HypeSquadStatus, String> {
    typed("set_hypesquad", json!({ "token": token, "target": target })).await
}

pub async fn remove_hypesquad(token: String) -> Result<HypeSquadStatus, String> {
    typed("remove_hypesquad", json!({ "token": token })).await
}

pub async fn quit_discord() -> Result<u64, String> {
    typed("quit_discord", json!({})).await
}

pub async fn check_update() -> Result<UpdateInfo, String> {
    typed("check_update", json!({})).await
}

pub async fn download_update() -> Result<(), String> {
    call("download_update", json!({})).await.map(|_| ())
}

pub async fn update_progress() -> Option<UpdateProgress> {
    typed("update_progress", json!({})).await.ok()
}

pub async fn friend_codes_list(token: String) -> Result<Vec<FriendCode>, String> {
    typed("friend_codes_list", json!({ "token": token })).await
}

pub async fn friend_code_create(token: String) -> Result<FriendCode, String> {
    typed("friend_code_create", json!({ "token": token })).await
}

pub async fn friend_code_revoke_all(token: String) -> Result<(), String> {
    call("friend_code_revoke_all", json!({ "token": token }))
        .await
        .map(|_| ())
}

pub async fn voice_follow_set(
    token: String,
    user_id: String,
    username: String,
    follow: bool,
) -> Result<Option<VoiceFollow>, String> {
    typed(
        "voice_follow_set",
        json!({ "token": token, "user_id": user_id, "username": username, "follow": follow }),
    )
    .await
}

pub async fn voice_follow_status(token: String) -> Result<Option<VoiceFollow>, String> {
    typed("voice_follow_status", json!({ "token": token })).await
}

pub async fn window_minimize() {
    let _ = call("window_minimize", json!({})).await;
}

pub async fn window_toggle_maximize() {
    let _ = call("window_toggle_maximize", json!({})).await;
}

pub async fn window_close() {
    let _ = call("window_close", json!({})).await;
}

pub async fn open_url(url: String) {
    let _ = call("open_url", json!({ "url": url })).await;
}

fn read_log_chunk(value: Value, after: u64) -> (String, u64) {
    let next = value
        .get("next")
        .and_then(Value::as_u64)
        .unwrap_or(after);
    let mut text = String::new();
    if let Some(lines) = value.get("lines").and_then(Value::as_array) {
        for line in lines {
            if let Some(line) = line.get("line").and_then(Value::as_str) {
                text.push_str(line);
                text.push('\n');
            }
        }
    }
    (text, next)
}

/// Copy text through the native Tauri command, falling back to the webview's
/// async clipboard API when that is unavailable.
pub async fn copy_text(text: String) -> Result<(), String> {
    if call("copy_to_clipboard", json!({ "text": text.clone() })).await.is_ok() {
        return Ok(());
    }
    let window = web_sys::window().ok_or("no window")?;
    let navigator = window.navigator();
    let clipboard = navigator.clipboard();
    let promise = clipboard.write_text(&text);
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|_| "could not access the clipboard".into())
}
