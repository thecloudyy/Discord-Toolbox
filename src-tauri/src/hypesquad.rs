//! HypeSquad house switching and profile badge display.
//!
//! Uses the same REST endpoints the desktop client calls: the account flags
//! carry the HypeSquad house and the profile endpoint lists visible badges.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};

const API_ME: &str = "https://discord.com/api/v9/users/@me";
const API_PROFILE: &str = "https://discord.com/api/v9/users";
const HYPESQUAD_ONLINE: &str = "https://discord.com/api/v9/hypesquad/online";
const BADGE_ICON_CDN: &str = "https://cdn.discordapp.com/badge-icons";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

const FLAG_BRAVERY: u64 = 64;
const FLAG_BRILLIANCE: u64 = 128;
const FLAG_BALANCE: u64 = 256;

#[derive(Serialize)]
pub struct ProfileBadge {
    pub id: String,
    pub description: String,
    pub icon_url: Option<String>,
}

#[derive(Serialize)]
pub struct HypeSquadStatus {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub house: Option<String>,
    pub flags: u64,
    pub badges: Vec<ProfileBadge>,
}

#[tauri::command]
pub async fn hypesquad_status(token: String) -> Result<HypeSquadStatus, String> {
    let token = crate::badges::resolve_token(&token).await?;
    let client = http_client()?;
    fetch_status_with(&client, &token).await
}

#[tauri::command]
pub async fn set_hypesquad(token: String, target: String) -> Result<HypeSquadStatus, String> {
    let token = crate::badges::resolve_token(&token).await?;
    let client = http_client()?;

    let house_id = match target.as_str() {
        "bravery" => 1,
        "brilliance" => 2,
        "balance" => 3,
        _ => return Err("unknown HypeSquad house".into()),
    };
    let response = with_headers(
        client.post(HYPESQUAD_ONLINE).json(&json!({ "house_id": house_id })),
        &token,
    )
    .send()
    .await
    .map_err(|error| format!("network error: {error}"))?;
    check(response, "switch HypeSquad house", false).await?;

    refresh_after_action(&client, &token).await
}

#[tauri::command]
pub async fn remove_hypesquad(token: String) -> Result<HypeSquadStatus, String> {
    let token = crate::badges::resolve_token(&token).await?;
    let client = http_client()?;

    let response = with_headers(client.delete(HYPESQUAD_ONLINE), &token)
        .send()
        .await
        .map_err(|error| format!("network error: {error}"))?;
    check(response, "remove the HypeSquad badge", true).await?;

    refresh_after_action(&client, &token).await
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| format!("could not create HTTP client: {error}"))
}

fn with_headers(
    request: reqwest::RequestBuilder,
    token: &str,
) -> reqwest::RequestBuilder {
    request
        .header("authorization", token)
        .header("user-agent", USER_AGENT)
        .header("origin", "https://discord.com")
        .header("referer", "https://discord.com/")
        .header("x-discord-locale", "en-US")
}

async fn refresh_after_action(
    client: &reqwest::Client,
    token: &str,
) -> Result<HypeSquadStatus, String> {
    // Discord applies the change immediately but the account flags can lag a beat.
    tokio::time::sleep(Duration::from_millis(800)).await;
    fetch_status_with(client, token).await
}

async fn check(
    response: reqwest::Response,
    action: &str,
    house_removal: bool,
) -> Result<(), String> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err("Discord rejected the token".into());
    }

    let body: Value = response.json().await.unwrap_or(Value::Null);
    let message = body
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if status.as_u16() == 429 {
        let wait = body
            .get("retry_after")
            .and_then(Value::as_f64)
            .unwrap_or(1.0)
            .clamp(1.0, 3600.0);
        return Err(format!(
            "Discord is rate limiting this action — try again in {}s.",
            wait.ceil() as u64
        ));
    }

    if house_removal && status.as_u16() == 500 {
        return Err(
            "Discord no longer allows removing HypeSquad badges — switch to another house instead."
                .into(),
        );
    }

    if message.is_empty() {
        Err(format!("Could not {action} (HTTP {}).", status.as_u16()))
    } else {
        Err(format!("Could not {action}: {message}"))
    }
}

async fn fetch_status_with(
    client: &reqwest::Client,
    token: &str,
) -> Result<HypeSquadStatus, String> {
    let response = with_headers(
        client.get(format!("{API_ME}?_={}", now_ms())),
        token,
    )
    .send()
    .await
    .map_err(|error| format!("network error: {error}"))?;

    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err("Discord rejected the token".into());
    }
    if !status.is_success() {
        return Err(format!("Discord replied with HTTP {}", status.as_u16()));
    }

    let user: Value = response
        .json()
        .await
        .map_err(|error| format!("could not read Discord's response: {error}"))?;

    let user_id = user.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
    if user_id.is_empty() {
        return Err("Discord did not return an account id".into());
    }
    let username = user
        .get("username")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let display_name = user
        .get("global_name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .unwrap_or(&username)
        .to_string();

    let flags = user.get("flags").and_then(Value::as_u64).unwrap_or(0)
        | user.get("public_flags").and_then(Value::as_u64).unwrap_or(0);

    let avatar_url = avatar_url(
        &user_id,
        user.get("avatar").and_then(Value::as_str),
        user.get("discriminator").and_then(Value::as_str),
    );

    let profile = fetch_profile(client, token, &user_id).await;
    let badges = profile.as_ref().map(parse_badges).unwrap_or_default();

    Ok(HypeSquadStatus {
        user_id,
        username,
        display_name,
        avatar_url,
        house: house_from_flags(flags),
        flags,
        badges,
    })
}

async fn fetch_profile(
    client: &reqwest::Client,
    token: &str,
    user_id: &str,
) -> Option<Value> {
    let response = with_headers(
        client.get(format!("{API_PROFILE}/{user_id}/profile?_={}", now_ms())),
        token,
    )
    .send()
    .await
    .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json().await.ok()
}

fn parse_badges(profile: &Value) -> Vec<ProfileBadge> {
    profile
        .get("badges")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let id = entry.get("id").and_then(Value::as_str)?.to_string();
                    let description = entry
                        .get("description")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                        .unwrap_or(&id)
                        .to_string();
                    let icon_url = entry
                        .get("icon")
                        .and_then(Value::as_str)
                        .filter(|icon| !icon.is_empty())
                        .map(|icon| format!("{BADGE_ICON_CDN}/{icon}.png"));
                    Some(ProfileBadge { id, description, icon_url })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn house_from_flags(flags: u64) -> Option<String> {
    if flags & FLAG_BALANCE != 0 {
        Some("balance".into())
    } else if flags & FLAG_BRILLIANCE != 0 {
        Some("brilliance".into())
    } else if flags & FLAG_BRAVERY != 0 {
        Some("bravery".into())
    } else {
        None
    }
}

fn avatar_url(user_id: &str, avatar: Option<&str>, discriminator: Option<&str>) -> Option<String> {
    if let Some(hash) = avatar.filter(|hash| !hash.is_empty()) {
        return Some(format!(
            "https://cdn.discordapp.com/avatars/{user_id}/{hash}.png?size=160"
        ));
    }
    let index = match discriminator.and_then(|value| value.parse::<u64>().ok()) {
        Some(discriminator) if discriminator > 0 => discriminator % 5,
        _ => user_id
            .parse::<u64>()
            .map(|snowflake| (snowflake >> 22) % 6)
            .unwrap_or(0),
    };
    Some(format!("https://cdn.discordapp.com/embed/avatars/{index}.png"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn houses_map_from_account_flags() {
        assert_eq!(house_from_flags(FLAG_BRAVERY).as_deref(), Some("bravery"));
        assert_eq!(house_from_flags(FLAG_BRILLIANCE).as_deref(), Some("brilliance"));
        assert_eq!(house_from_flags(FLAG_BALANCE).as_deref(), Some("balance"));
        assert_eq!(house_from_flags(0), None);
        assert_eq!(
            house_from_flags(FLAG_BRAVERY | FLAG_BALANCE).as_deref(),
            Some("balance")
        );
    }

    #[test]
    fn new_usernames_use_the_six_default_avatars() {
        assert_eq!(
            avatar_url("123456789012345678", None, Some("0")).as_deref(),
            Some("https://cdn.discordapp.com/embed/avatars/0.png")
        );
        assert_eq!(
            avatar_url("123456789012345678", None, Some("7")).as_deref(),
            Some("https://cdn.discordapp.com/embed/avatars/2.png")
        );
        assert!(avatar_url("123456789012345678", Some("abc"), Some("0"))
            .unwrap()
            .contains("/avatars/123456789012345678/abc.png"));
    }

    #[test]
    #[ignore = "calls the Discord API with the token signed in on this machine"]
    fn reads_live_hypesquad_status() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let token = crate::bot::app_config()
                .await
                .get("DISCORD_TOKEN")
                .cloned()
                .unwrap_or_default();
            assert!(!token.trim().is_empty(), "no DISCORD_TOKEN in .env");
            let client = http_client().unwrap();
            let status = fetch_status_with(&client, token.trim()).await.unwrap();
            assert!(!status.user_id.is_empty());
            println!(
                "live status: @{} house={:?} badges={}",
                status.username,
                status.house,
                status.badges.len()
            );
        });
    }
}
