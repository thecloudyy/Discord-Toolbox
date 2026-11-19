//! Social features built on Discord's public REST API.
//!
//! Note: Discord does not expose friend-code creation or voice-channel
//! selection through its public REST API (those are Discord-client internals).
//! These commands keep a small local state file so the UI still works, and
//! they validate the user token against `/users/@me` before acting.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::config_dir;

const STATE_FILE: &str = ".social-state.json";
const API_ME: &str = "https://discord.com/api/v9/users/@me";
const API_USER: &str = "https://discord.com/api/v9/users/";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) discord/1.0.9253 Chrome/148.0.7778.280 Electron/42.7.1 Safari/537.36";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SocialState {
    pub friend_codes: Vec<FriendCode>,
    pub voice_follow: Option<VoiceFollow>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FriendCode {
    pub code: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub uses: u32,
    pub max_uses: Option<u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VoiceFollow {
    pub user_id: String,
    pub username: String,
    pub following: bool,
    pub followed_at: Option<String>,
}

fn state_path() -> PathBuf {
    config_dir().join(STATE_FILE)
}

fn load_state() -> SocialState {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_state(state: &SocialState) -> Result<(), String> {
    let text = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(state_path(), text).map_err(|e| e.to_string())
}

fn now_epoch() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

/// Verify a token resolves to a real Discord user before any social action.
pub async fn resolve_user(token: &str) -> Result<Value, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("no Discord token is available".into());
    }
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))?;
    let response = client
        .get(API_ME)
        .header("authorization", token)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
        return Err("Discord rejected the token".into());
    }
    if !response.status().is_success() {
        return Err(format!("Discord replied with HTTP {}", response.status().as_u16()));
    }
    response
        .json()
        .await
        .map_err(|e| format!("could not read Discord's response: {e}"))
}

// ------------------------------------------------------------------ commands

#[tauri::command]
pub async fn friend_codes_list(token: String) -> Result<Vec<FriendCode>, String> {
    let _ = resolve_user(&token).await?;
    Ok(load_state().friend_codes)
}

#[tauri::command]
pub async fn friend_code_create(token: String) -> Result<FriendCode, String> {
    let _ = resolve_user(&token).await?;
    let mut state = load_state();
    let code = nanoid();
    let entry = FriendCode {
        code: code.clone(),
        created_at: now_epoch(),
        expires_at: None,
        uses: 0,
        max_uses: None,
    };
    state.friend_codes.push(entry.clone());
    save_state(&state)?;
    Ok(entry)
}

#[tauri::command]
pub async fn friend_code_revoke_all(token: String) -> Result<(), String> {
    let _ = resolve_user(&token).await?;
    let mut state = load_state();
    state.friend_codes.clear();
    save_state(&state)
}

#[tauri::command]
pub async fn voice_follow_set(
    token: String,
    user_id: String,
    username: String,
    follow: bool,
) -> Result<Option<VoiceFollow>, String> {
    let _ = resolve_user(&token).await?;
    let mut state = load_state();
    if follow {
        let user_id = user_id.trim().to_string();
        if user_id.is_empty() {
            return Err("Enter a friend's Discord user ID first.".into());
        }
        if !is_snowflake(&user_id) {
            return Err("That does not look like a Discord user ID.".into());
        }
        // Never store a stale username for a new follow: resolve the followed
        // user's real name from Discord when none is provided.
        let username = if username.trim().is_empty() {
            lookup_user(&token, &user_id).await?
        } else {
            username.trim().to_string()
        };
        if username.is_empty() {
            return Err("Could not resolve that user's name from Discord.".into());
        }
        state.voice_follow = Some(VoiceFollow {
            user_id,
            username,
            following: true,
            followed_at: Some(now_epoch()),
        });
    } else {
        state.voice_follow = None;
    }
    save_state(&state)?;
    Ok(state.voice_follow.clone())
}

#[tauri::command]
pub async fn voice_follow_status(token: String) -> Result<Option<VoiceFollow>, String> {
    let _ = resolve_user(&token).await?;
    Ok(load_state().voice_follow)
}

/// Discord user IDs are numeric snowflakes.
fn is_snowflake(text: &str) -> bool {
    (17..=19).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// Resolve a Discord user's username from `/users/{user_id}` so a voice follow
/// is stored with a real name instead of a blank one.
async fn lookup_user(token: &str, user_id: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("could not create HTTP client: {e}"))?;
    let response = client
        .get(format!("{API_USER}{user_id}"))
        .header("authorization", token)
        .send()
        .await
        .map_err(|e| format!("network error: {e}"))?;
    if response.status().as_u16() == 401 || response.status().as_u16() == 403 {
        return Err("Discord rejected the token".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "Discord replied with HTTP {}",
            response.status().as_u16()
        ));
    }
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("could not read Discord's response: {e}"))?;
    let username = body
        .get("username")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if username.is_empty() {
        return Err("Discord did not return a username for that user ID".into());
    }
    Ok(username.to_string())
}

fn nanoid() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut out = String::with_capacity(10);
    let mut state: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
        ^ ((std::process::id() as u64) << 32);
    for _ in 0..10 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push(CHARS[(state as usize) % CHARS.len()] as char);
    }
    out
}
