//! Automatic game badge forging.
//!
//! Credits game playtime to the signed-in account by replaying the telemetry
//! batches the desktop client sends to Discord's science endpoint: a launch
//! event plus an initial/final heartbeat per game, all tied to one session id.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

const API_ME_ANALYTICS: &str = "https://discord.com/api/v9/users/@me?with_analytics_token=true";
const SCIENCE: &str = "https://discord.com/api/v9/science";
const GAMES_CATALOG: &str = "https://cdn.discordapp.com/detectables/games.json";
const PROPERTIES_URL: &str = "https://cordapi.dolfi.es/api/v2/properties/windows";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) discord/1.0.9253 Chrome/148.0.7778.280 Electron/42.7.1 Safari/537.36";
const LOCALE: &str = "en-US";
const TIMEZONE: &str = "UTC";
const CLIENT_VERSION: &str = "1.0.9253";
const CLIENT_BUILD_NUMBER: u64 = 594_031;
const NATIVE_BUILD_NUMBER: u64 = 88_414;
const OS_VERSION: &str = "10.0.26200";
const OS_SDK_VERSION: &str = "26200";
const BATCH_SIZE: usize = 25;
const BATCH_DELAY_MS: u64 = 450;
const MAX_RATE_LIMIT_RETRIES: usize = 10;
/// Ceiling for one run's total playtime budget. Discord's Game Time badge
/// maxes out at 5,000 cumulative hours, so anything here is already past the
/// final tier; more only makes the session less plausible.
const MAX_SAFE_HOURS: f64 = 10_000.0;
/// Lowest plausible playtime credited to a single title.
const MIN_GAME_HOURS: f64 = 0.1;
/// Upper bound on a run when "all games" is requested.
const MAX_GAMES: usize = 100_000;
const DISCORD_EPOCH_MS: u64 = 1_420_070_400_000;

/// Forge run state shared with the UI.
pub struct BadgeState {
    running: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

impl BadgeState {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Default for BadgeState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Serialize)]
pub struct BadgeStatus {
    pub running: bool,
}

#[tauri::command]
pub fn badge_status(state: State<'_, BadgeState>) -> BadgeStatus {
    BadgeStatus { running: state.running.load(Ordering::SeqCst) }
}

#[tauri::command]
pub fn stop_badges(state: State<'_, BadgeState>) {
    if state.running.load(Ordering::SeqCst) {
        state.cancel.store(true, Ordering::SeqCst);
    }
}

/// Kick off a forge run and return immediately; progress arrives through events.
#[tauri::command]
pub async fn forge_badges(
    app: AppHandle,
    state: State<'_, BadgeState>,
    token: String,
    hours: f64,
    games: usize,
) -> Result<(), String> {
    // Every input is checked up front so a run never starts half-configured.
    let account_token = resolve_token(&token).await?;
    let cookie = resolve_cookie().await?;

    if !hours.is_finite() || hours <= 0.0 {
        return Err("hours must be a positive number".into());
    }
    if hours > MAX_SAFE_HOURS {
        return Err(format!(
            "hours must be at most {MAX_SAFE_HOURS} — that is the maximum safe total per run"
        ));
    }
    // 0 means "every game in the catalog"; the catalog is about 10,000 titles.
    let requested = if games == 0 { MAX_GAMES } else { games.min(MAX_GAMES) };

    #[cfg(target_os = "windows")]
    if find_reference_exe().is_none() {
        return Err("no executable is available to derive a fingerprint from".into());
    }

    if state.running.swap(true, Ordering::SeqCst) {
        return Err("badge forging is already running".into());
    }
    state.cancel.store(false, Ordering::SeqCst);

    let running = state.running.clone();
    let cancel = state.cancel.clone();

    emit_state(&app, true, requested, hours, None, None);
    let scope = if games == 0 {
        "every detectable game".to_string()
    } else {
        format!("up to {requested} game(s)")
    };
    log_line(
        &app,
        format!("Preparing to forge {scope} with {hours} hours in total."),
    );

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let outcome =
            forge_inner(&app_handle, &account_token, &cookie, hours, requested, &cancel).await;
        running.store(false, Ordering::SeqCst);
        match outcome {
            Ok(summary) => {
                if summary.stopped {
                    log_line(&app_handle, format!("Stopped after {} game(s).", summary.processed));
                }
                emit_state(
                    &app_handle,
                    false,
                    summary.processed,
                    hours,
                    summary.username.as_deref(),
                    None,
                );
            }
            Err(error) => {
                log_line(&app_handle, format!("Error: {error}"));
                emit_state(&app_handle, false, 0, hours, None, Some(&error));
            }
        }
    });

    Ok(())
}

pub(crate) async fn resolve_token(token: &str) -> Result<String, String> {
    let direct = token.trim();
    if !direct.is_empty() {
        return Ok(direct.to_string());
    }
    let stored = crate::bot::app_config()
        .await
        .get("DISCORD_TOKEN")
        .cloned()
        .unwrap_or_default();
    let stored = stored.trim();
    if stored.is_empty() {
        Err("no Discord token is available — sign in first".into())
    } else {
        Ok(stored.to_string())
    }
}

struct RunSummary {
    username: Option<String>,
    processed: usize,
    stopped: bool,
}

#[derive(Clone)]
struct Game {
    id: u64,
    name: String,
    executable: String,
}

async fn forge_inner(
    app: &AppHandle,
    account_token: &str,
    cookie: &str,
    hours: f64,
    requested: usize,
    cancel: &AtomicBool,
) -> Result<RunSummary, String> {
    if is_cancelled(cancel) {
        return Ok(RunSummary { username: None, processed: 0, stopped: true });
    }

    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|error| format!("could not create HTTP client: {error}"))?;

    log_line(app, "Opening a fresh client session…".to_string());
    let session = Session::new(&client).await;
    log_line(app, "Using the local Discord cookie for the telemetry headers.".to_string());

    log_line(app, "Resolving the account…".to_string());
    let (user_id, username, analytics_token) =
        fetch_account(&client, account_token, &session.super_props).await?;
    log_line(app, format!("Signed in as @{username}."));
    emit_state(app, true, requested, hours, Some(&username), None);

    if is_cancelled(cancel) {
        return Ok(RunSummary { username: Some(username), processed: 0, stopped: true });
    }

    log_line(app, "Detecting an executable fingerprint on this machine…".to_string());
    let (fingerprint_path, fingerprint) = detect_fingerprint(&user_id).ok_or(
        "could not generate an executable fingerprint — no PE32+ executable was found to fingerprint",
    )?;
    log_line(
        app,
        format!("Fingerprint generated from {}.", fingerprint_path.display()),
    );

    log_line(app, "Loading the game catalog…".to_string());
    let catalog = fetch_catalog(&client).await?;
    let target = requested.min(catalog.len());
    if target == 0 {
        return Err("no supported games were found in the catalog".into());
    }

    let order: Vec<String> = catalog.iter().map(|game| game.id.to_string()).collect();
    let (selected_ids, next_used, fresh) = select_rotation(&order, &load_used_games(), target);
    save_used_games(&next_used);
    let by_id: HashMap<String, Game> =
        catalog.into_iter().map(|game| (game.id.to_string(), game)).collect();
    let selected: Vec<Game> =
        selected_ids.iter().filter_map(|id| by_id.get(id).cloned()).collect();
    let total = selected.len();

    log_line(
        app,
        format!("Catalog ready: {total} game(s) selected ({fresh} never claimed before)."),
    );
    if fresh < total {
        log_line(
            app,
            format!(
                "{} game(s) rotate from earlier runs — no fresh catalog entries left.",
                total - fresh
            ),
        );
    }
    emit_state(app, true, total, hours, Some(&username), None);

    let mut rng = Rng::new();
    let requested_per_game = hours / total as f64;
    let per_game_hours = hours_per_game(hours, total, max_safe_hours(&user_id));
    let credited_hours = per_game_hours * total as f64;
    if per_game_hours < requested_per_game {
        log_line(
            app,
            format!(
                "Capped at {per_game_hours:.1} hours per game (account age limit), {credited_hours:.0} hours in total."
            ),
        );
    }
    let duration_ms = (per_game_hours * 3_600_000.0) as u64;
    log_line(
        app,
        format!(
            "Crediting {per_game_hours:.1} hours to each of {total} game(s), {credited_hours:.0} hours in total."
        ),
    );
    let mut sequence = 0u64;
    let mut processed = 0usize;
    let mut batch_index = 0usize;
    let total_batches = total.div_ceil(BATCH_SIZE);

    for batch in selected.chunks(BATCH_SIZE) {
        if is_cancelled(cancel) {
            return Ok(RunSummary { username: Some(username), processed, stopped: true });
        }
        batch_index += 1;

        let mut events = Vec::with_capacity(batch.len() * 3);
        for game in batch {
            events.extend(game_events(
                game,
                &fingerprint,
                duration_ms,
                &mut sequence,
                &session,
                &mut rng,
            ));
        }

        log_line(
            app,
            format!("Batch {batch_index}/{total_batches}: sending {} game(s)…", batch.len()),
        );
        send_batch(
            &client,
            account_token,
            &analytics_token,
            Some(cookie),
            &session.super_props,
            &events,
        )
        .await?;
        processed += batch.len();
        log_line(
            app,
            format!(
                "Batch {batch_index}/{total_batches} accepted ({processed}/{total})."
            ),
        );

        if batch_index < total_batches {
            tokio::time::sleep(Duration::from_millis(BATCH_DELAY_MS)).await;
        }
    }

    log_line(
        app,
        format!(
            "Finished: {processed} game(s) credited, {:.0} hours in total.",
            per_game_hours * processed as f64
        ),
    );

    Ok(RunSummary { username: Some(username), processed, stopped: false })
}

async fn fetch_account(
    client: &reqwest::Client,
    token: &str,
    super_props: &str,
) -> Result<(String, String, String), String> {
    let response = client
        .get(API_ME_ANALYTICS)
        .header("authorization", token)
        .header("user-agent", USER_AGENT)
        .header("x-super-properties", super_props)
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

    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("could not read Discord's response: {error}"))?;

    let user_id = body.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
    let username = body.get("username").and_then(Value::as_str).unwrap_or_default().to_string();
    let analytics_token = body
        .get("analytics_token")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if user_id.is_empty() || analytics_token.is_empty() {
        return Err("Discord did not return an analytics token".into());
    }

    let username = if username.is_empty() { "unknown".to_string() } else { username };
    Ok((user_id, username, analytics_token))
}

async fn fetch_catalog(client: &reqwest::Client) -> Result<Vec<Game>, String> {
    let response = client
        .get(GAMES_CATALOG)
        .send()
        .await
        .map_err(|error| format!("could not download the game catalog: {error}"))?;

    if !response.status().is_success() {
        return Err(format!("game catalog replied with HTTP {}", response.status().as_u16()));
    }

    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("could not read the game catalog: {error}"))?;
    let entries = body
        .as_array()
        .ok_or("unexpected game catalog format")?;

    let mut games = Vec::new();
    for entry in entries {
        let Some(id) = entry
            .get("id")
            .and_then(|value| value.as_u64().or_else(|| value.as_str().and_then(|text| text.parse().ok())))
        else {
            continue;
        };
        let Some(name) = entry.get("name").and_then(Value::as_str).filter(|name| !name.is_empty())
        else {
            continue;
        };
        let Some(executable) = entry.get("executables").and_then(Value::as_array).and_then(|list| {
            list.iter().find_map(|executable| {
                let win32 = executable.get("os").and_then(Value::as_str) == Some("win32");
                let name = executable.get("name").and_then(Value::as_str).filter(|name| !name.is_empty());
                (win32 && name.is_some()).then(|| name.unwrap().to_string())
            })
        })
        else {
            continue;
        };
        games.push(Game { id, name: name.to_string(), executable });
    }

    Ok(games)
}

/// Claimed game ids survive across runs so every forge picks games the account
/// has not been credited for yet.
fn badge_games_path() -> PathBuf {
    crate::bot::config_dir().join(".badge-games.json")
}

fn load_used_games() -> Vec<String> {
    let parsed = std::fs::read_to_string(badge_games_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
        .unwrap_or_default();
    let mut seen = HashSet::new();
    parsed.into_iter().filter(|id| seen.insert(id.clone())).collect()
}

fn save_used_games(ids: &[String]) {
    if let Ok(text) = serde_json::to_string(ids) {
        let _ = std::fs::write(badge_games_path(), text);
    }
}

/// Pick `count` games, preferring never-claimed ones and rotating through the
/// claimed list once the catalog runs out. Returns the selection, the updated
/// claimed list and how many picks are fresh.
fn select_rotation(
    order: &[String],
    used: &[String],
    count: usize,
) -> (Vec<String>, Vec<String>, usize) {
    let available: HashSet<&str> = order.iter().map(String::as_str).collect();
    let used_set: HashSet<&str> = used.iter().map(String::as_str).collect();

    let mut selected: Vec<String> = order
        .iter()
        .filter(|id| !used_set.contains(id.as_str()))
        .take(count)
        .cloned()
        .collect();
    let fresh = selected.len();

    if selected.len() < count {
        let reusable: Vec<&String> = used
            .iter()
            .filter(|id| available.contains(id.as_str()))
            .collect();
        if !reusable.is_empty() {
            let remaining = count - selected.len();
            let split = remaining % reusable.len();
            selected.extend(
                reusable[split..]
                    .iter()
                    .chain(reusable[..split].iter())
                    .take(remaining)
                    .map(|id| (*id).clone()),
            );
        }
    }

    let selected_set: HashSet<&str> = selected.iter().map(String::as_str).collect();
    let mut next_used: Vec<String> = used
        .iter()
        .filter(|id| available.contains(id.as_str()) && !selected_set.contains(id.as_str()))
        .cloned()
        .collect();
    next_used.extend(selected.iter().cloned());

    (selected, next_used, fresh)
}

fn game_events(
    game: &Game,
    fingerprint: &str,
    duration_ms: u64,
    sequence: &mut u64,
    session: &Session,
    rng: &mut Rng,
) -> Vec<Value> {
    let session_id = uuid4(rng);
    let now = now_ms();
    let started = now
        .checked_sub(duration_ms)
        .filter(|timestamp| *timestamp > 0)
        .unwrap_or(now);
    let game_id = game.id.to_string();

    let next = |sequence: &mut u64| {
        *sequence += 1;
        *sequence
    };

    let initial = json!({
        "type": "running_game_heartbeat",
        "properties": {
            "client_track_timestamp": started,
            "client_heartbeat_session_id": session.heartbeat_session.as_str(),
            "event_sequence_number": next(sequence),
            "game_id": game_id.as_str(),
            "game_name": game.name.as_str(),
            "game_metadata": null,
            "game_executable": game.executable.as_str(),
            "game_detection_enabled": true,
            "initial_heartbeat": true,
            "final_heartbeat": false,
            "game_session_id": session_id.as_str(),
            "duration_tracked_ms": 0,
            "rtc_connection_id": null,
            "media_session_id": null,
            "launch_signature": session.launch_signature.as_str(),
            "client_app_state": "focused",
            "client_send_timestamp": started,
        },
    });

    let launch_properties = json!({
        "client_track_timestamp": now,
        "client_heartbeat_session_id": session.heartbeat_session.as_str(),
        "event_sequence_number": next(sequence),
        "game": game.name.as_str(),
        "game_id": game_id.as_str(),
        "verified": true,
        "elevated": false,
        "is_launcher": false,
        "game_platform": "desktop",
        "detection_method": "verified_game",
        "is_overlay_enabled": false,
        "is_overlay_game_enabled": true,
        "is_overlay_game_source": "OOP_DEFAULT_DATABASE",
        "fullscreen_type": "UNKNOWN",
        "hardware_display_count": 1,
        "overlay_method": "Disabled",
        "activity_status_enabled": true,
        "activity_status_shared_guilds": [],
        "current_user_status": "online",
        "game_detection_enabled": true,
        "executable_path": game.executable.as_str(),
        "voice_channel_id": null,
        "voice_channel_type": null,
        "voice_channel_bitrate": null,
        "voice_channel_guild_id": null,
        "hidden_by_distributor": false,
        "game_metadata": null,
        "executable_fingerprint": fingerprint,
        "client_performance_cpu": null,
        "client_performance_memory": null,
        "cpu_core_count": null,
        "accessibility_features": 0,
        "rendered_locale": LOCALE,
        "launch_signature": session.launch_signature.as_str(),
        "client_rtc_state": null,
        "client_app_state": "focused",
        "client_send_timestamp": now,
    });
    let launch = json!({
        "type": "launch_game",
        "properties": launch_properties,
    });

    let final_heartbeat = json!({
        "type": "running_game_heartbeat",
        "properties": {
            "client_track_timestamp": now,
            "client_heartbeat_session_id": session.heartbeat_session.as_str(),
            "event_sequence_number": next(sequence),
            "game_id": game_id.as_str(),
            "game_name": game.name.as_str(),
            "game_metadata": null,
            "game_executable": game.executable.as_str(),
            "game_detection_enabled": true,
            "initial_heartbeat": false,
            "final_heartbeat": true,
            "game_session_id": session_id.as_str(),
            "duration_tracked_ms": duration_ms,
            "rtc_connection_id": null,
            "media_session_id": null,
            "launch_signature": session.launch_signature.as_str(),
            "client_app_state": "focused",
            "client_send_timestamp": now,
        },
    });

    vec![initial, launch, final_heartbeat]
}

async fn send_batch(
    client: &reqwest::Client,
    account_token: &str,
    analytics_token: &str,
    cookie: Option<&str>,
    super_props: &str,
    events: &[Value],
) -> Result<(), String> {
    let body = json!({ "token": analytics_token, "events": events });
    let mut attempt = 0usize;

    loop {
        let mut request = client
            .post(SCIENCE)
            .header("accept", "*/*")
            .header("accept-language", LOCALE)
            .header("authorization", account_token)
            .header("origin", "https://discord.com")
            .header("referer", "https://discord.com/channels/@me")
            .header("x-debug-options", "bugReporterEnabled")
            .header("x-discord-locale", LOCALE)
            .header("x-discord-timezone", TIMEZONE)
            .header("x-super-properties", super_props);
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }

        let response = request
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("network error while sending telemetry: {error}"))?;

        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        if status.as_u16() == 429 {
            attempt += 1;
            if attempt > MAX_RATE_LIMIT_RETRIES {
                return Err("Discord kept rate limiting the telemetry batches".into());
            }
            let retry_after = response
                .json::<Value>()
                .await
                .ok()
                .and_then(|body| body.get("retry_after").and_then(Value::as_f64))
                .unwrap_or(2.0)
                .clamp(0.5, 60.0);
            tokio::time::sleep(Duration::from_secs_f64(retry_after)).await;
            continue;
        }

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err("Discord rejected the token".into());
        }

        return Err(format!("science endpoint replied with HTTP {}", status.as_u16()));
    }
}

fn emit_state(
    app: &AppHandle,
    running: bool,
    games: usize,
    hours: f64,
    username: Option<&str>,
    error: Option<&str>,
) {
    let mut payload = json!({ "running": running, "games": games, "hours": hours });
    if let Some(username) = username {
        payload["username"] = json!(username);
    }
    if let Some(error) = error {
        payload["error"] = json!(error);
    }
    let _ = app.emit("badge-state", payload);
}

fn log_line(app: &AppHandle, line: String) {
    let _ = app.emit("badge-log", line);
}

fn is_cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::SeqCst)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Split one run's total playtime budget across the selected games, keeping
/// each title above the plausibility floor and under the account-age cap.
fn hours_per_game(total_hours: f64, games: usize, cap: f64) -> f64 {
    if games == 0 {
        return MIN_GAME_HOURS;
    }
    (total_hours / games as f64).clamp(MIN_GAME_HOURS, cap)
}

/// The account cannot have played longer than it has existed, so its age in
/// hours is the highest per-game claim that stays plausible.
fn max_safe_hours(user_id: &str) -> f64 {
    let created = user_id
        .parse::<u64>()
        .ok()
        .map(|snowflake| (snowflake >> 22).saturating_add(DISCORD_EPOCH_MS));
    let age_hours = created
        .map(|created| now_ms().saturating_sub(created) as f64 / 3_600_000.0)
        .unwrap_or(MAX_SAFE_HOURS);
    age_hours.clamp(1.0, MAX_SAFE_HOURS)
}

/// Per-run client identity: fresh UUIDs plus the matching super-properties
/// header, created at the start of every forge run.
struct Session {
    heartbeat_session: String,
    launch_signature: String,
    super_props: String,
}

impl Session {
    async fn new(client: &reqwest::Client) -> Self {
        let mut rng = Rng::new();
        let heartbeat_session = uuid4(&mut rng);
        let launch_signature = uuid4(&mut rng);
        let super_props = build_super_props(client, &launch_signature, &heartbeat_session).await;
        Self { heartbeat_session, launch_signature, super_props }
    }
}

/// Fetch current client properties and inject this run's session identifiers.
async fn build_super_props(
    client: &reqwest::Client,
    launch_signature: &str,
    heartbeat_session: &str,
) -> String {
    let mut properties = match client
        .post(PROPERTIES_URL)
        .header("content-type", "application/json")
        .body("{}")
        .timeout(Duration::from_secs(10))
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => response
            .json::<Value>()
            .await
            .ok()
            .and_then(|body| body.get("properties").cloned())
            .unwrap_or_else(local_super_props),
        _ => local_super_props(),
    };
    if !properties.is_object() {
        properties = local_super_props();
    }

    let mut rng = Rng::new();
    properties["client_launch_id"] = json!(uuid4(&mut rng));
    properties["launch_signature"] = json!(launch_signature);
    properties["client_heartbeat_session_id"] = json!(heartbeat_session);
    encode_super_props(&properties)
}

/// Client properties used when the remote source is unreachable.
fn local_super_props() -> Value {
    json!({
        "os": "Windows",
        "browser": "Discord Client",
        "release_channel": "stable",
        "client_version": CLIENT_VERSION,
        "os_version": OS_VERSION,
        "os_arch": "x64",
        "app_arch": "x64",
        "system_locale": LOCALE,
        "has_client_mods": false,
        "browser_user_agent": USER_AGENT,
        "browser_version": "42.7.1",
        "os_sdk_version": OS_SDK_VERSION,
        "client_build_number": CLIENT_BUILD_NUMBER,
        "native_build_number": NATIVE_BUILD_NUMBER,
        "client_event_source": null,
        "client_app_state": "focused",
    })
}

fn encode_super_props(properties: &Value) -> String {
    let raw = serde_json::to_vec(properties).unwrap_or_else(|_| b"{}".to_vec());
    BASE64.encode(raw)
}

/// Cookie header for the telemetry requests: an explicit value from the app
/// config first, otherwise the one stored by the local Discord client.
async fn resolve_cookie() -> Result<String, String> {
    let configured = crate::bot::app_config()
        .await
        .get("DISCORD_COOKIE")
        .cloned()
        .unwrap_or_default();
    let configured = configured.trim();
    if !configured.is_empty() {
        return Ok(configured.to_string());
    }
    match crate::token_extract::cookie_header_from_profiles() {
        Some(cookie) => Ok(cookie),
        None if crate::processes::discord_running() => Err(
            "COOKIE_LOCKED:Discord is running and keeps its cookie store locked, so the session \
             cookie cannot be read."
                .to_string(),
        ),
        None => Err(
            "no Discord cookie could be read — sign in to Discord once, then try again, or set \
             DISCORD_COOKIE in .env"
                .to_string(),
        ),
    }
}

/// Auto-detect a reference executable and derive Discord's executable fingerprint.
#[cfg(target_os = "windows")]
fn detect_fingerprint(user_id: &str) -> Option<(PathBuf, String)> {
    let path = find_reference_exe()?;
    let value = real_fingerprint(&path, user_id)?;
    Some((path, value))
}

#[cfg(not(target_os = "windows"))]
fn detect_fingerprint(_user_id: &str) -> Option<(PathBuf, String)> {
    None
}

/// Prefer a locally installed signed executable, then fall back to the OS and
/// finally this binary. Any PE32+ image yields an authentic fingerprint.
fn find_reference_exe() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let discord = PathBuf::from(local).join("Discord");
        if let Ok(entries) = std::fs::read_dir(&discord) {
            let mut versions: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.starts_with("app-"))
                        .unwrap_or(false)
                })
                .collect();
            versions.sort();
            candidates.extend(versions.into_iter().rev().map(|dir| dir.join("Discord.exe")));
        }
    }

    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(dir) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(dir).join("Discord").join("Discord.exe"));
        }
    }
    if let Some(windows) = std::env::var_os("WINDIR") {
        candidates.push(PathBuf::from(windows).join("explorer.exe"));
    }
    if let Ok(current) = std::env::current_exe() {
        candidates.push(current);
    }

    candidates.into_iter().find(|path| path.is_file())
}

/// Rebuild the 146-byte base64 fingerprint from a PE32+ image: file metadata,
/// per-machine GUID binding, certificate digest, XOR-masked with an account key.
#[cfg(target_os = "windows")]
fn real_fingerprint(path: &Path, user_id: &str) -> Option<String> {
    let file_size = std::fs::metadata(path).ok()?.len();
    let mut prefix = Vec::with_capacity(0x10000);
    std::fs::File::open(path).ok()?.take(0x10000).read_to_end(&mut prefix).ok()?;

    if prefix.len() < 0x40 {
        return None;
    }
    if u16::from_le_bytes(prefix[0..2].try_into().ok()?) != 0x5A4D {
        return None;
    }
    let pe_offset = u32::from_le_bytes(prefix[0x3C..0x40].try_into().ok()?) as usize;
    if pe_offset + 24 > prefix.len() {
        return None;
    }
    if u32::from_le_bytes(prefix[pe_offset..pe_offset + 4].try_into().ok()?) != 0x0000_4550 {
        return None;
    }

    let file_header = pe_offset + 4;
    let section_count =
        u16::from_le_bytes(prefix[file_header + 2..file_header + 4].try_into().ok()?) as usize;
    let timestamp =
        u32::from_le_bytes(prefix[file_header + 4..file_header + 8].try_into().ok()?);
    let optional_size =
        u16::from_le_bytes(prefix[file_header + 16..file_header + 18].try_into().ok()?) as usize;
    let optional = file_header + 20;
    if optional + 60 > prefix.len() {
        return None;
    }
    if u16::from_le_bytes(prefix[optional..optional + 2].try_into().ok()?) != 0x20B {
        return None; // only PE32+ images are fingerprinted
    }
    let size_of_image =
        u32::from_le_bytes(prefix[optional + 56..optional + 60].try_into().ok()?);

    let section_table = optional + optional_size;
    let sections_end = section_table.checked_add(section_count.checked_mul(40)?)?;
    if sections_end > prefix.len() {
        return None;
    }

    let header_hash = sha256(&prefix[..section_table]);
    let section_hash = sha256(&prefix[section_table..sections_end]);
    let machine_user = format!("{}{}", machine_guid()?, user_id);
    let machine_user_hash = sha256(machine_user.as_bytes());

    let mut flags = 2u8; // PE32+
    let mut cert_hash = [0u8; 32];
    if let Some((name, public_key_hash)) = signer_certificate(path) {
        flags |= 1;
        let mut material = name.into_bytes();
        material.extend_from_slice(&public_key_hash);
        cert_hash = sha256(&material);
    }

    let mut buffer = [0u8; 146];
    buffer[0] = 1;
    buffer[1] = flags;
    buffer[2..10].copy_from_slice(&file_size.to_le_bytes());
    buffer[10..14].copy_from_slice(&timestamp.to_le_bytes());
    buffer[14..18].copy_from_slice(&size_of_image.to_le_bytes());
    buffer[18..50].copy_from_slice(&header_hash);
    buffer[50..82].copy_from_slice(&machine_user_hash);
    buffer[82..114].copy_from_slice(&section_hash);
    buffer[114..146].copy_from_slice(&cert_hash);

    let key = sha256(format!("{user_id}https://discord.com/careers").as_bytes());
    for index in 1..buffer.len() {
        buffer[index] ^= key[(index - 1) & 31];
    }

    Some(BASE64.encode(buffer))
}

#[cfg(target_os = "windows")]
fn machine_guid() -> Option<String> {
    #[link(name = "advapi32")]
    extern "system" {
        fn RegGetValueW(
            key: usize,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            value_type: *mut u32,
            data: *mut core::ffi::c_void,
            size: *mut u32,
        ) -> i32;
    }

    const HKEY_LOCAL_MACHINE: usize = 0x8000_0002;
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;
    const ERROR_MORE_DATA: i32 = 234;

    let sub_key: Vec<u16> = "SOFTWARE\\Microsoft\\Cryptography\0".encode_utf16().collect();
    let value: Vec<u16> = "MachineGuid\0".encode_utf16().collect();
    let mut size = 0u32;
    let first = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            sub_key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if first != 0 && first != ERROR_MORE_DATA {
        return None;
    }

    let mut buffer = vec![0u16; (size as usize / 2).max(1)];
    let second = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            sub_key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr() as *mut core::ffi::c_void,
            &mut size,
        )
    };
    if second != 0 {
        return None;
    }

    let length = buffer.iter().position(|&unit| unit == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..length]))
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct CryptBlob {
    size: u32,
    data: *mut u8,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct CryptAlgorithm {
    oid: *mut u16,
    parameters: CryptBlob,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct CryptBitBlob {
    size: u32,
    data: *mut u8,
    unused_bits: u32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct PublicKeyInfo {
    algorithm: CryptAlgorithm,
    public_key: CryptBitBlob,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct CertTime {
    low: u32,
    high: u32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct CertInfo {
    version: u32,
    serial_number: CryptBlob,
    signature_algorithm: CryptAlgorithm,
    issuer: CryptBlob,
    not_before: CertTime,
    not_after: CertTime,
    subject: CryptBlob,
    public_key: PublicKeyInfo,
    issuer_id: CryptBitBlob,
    subject_id: CryptBitBlob,
    extension_count: u32,
    extensions: *mut core::ffi::c_void,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct CertContext {
    encoding: u32,
    encoded: *mut u8,
    encoded_size: u32,
    info: *mut CertInfo,
    store: *mut core::ffi::c_void,
}

#[cfg(target_os = "windows")]
#[link(name = "crypt32")]
extern "system" {
    fn CryptQueryObject(
        object_type: u32,
        object: *const core::ffi::c_void,
        expected_content: u32,
        expected_format: u32,
        flags: u32,
        encoding: *mut u32,
        content_type: *mut u32,
        format_type: *mut u32,
        store: *mut *mut core::ffi::c_void,
        message: *mut *mut core::ffi::c_void,
        context: *mut *mut core::ffi::c_void,
    ) -> i32;
    fn CryptMsgGetParam(
        message: *mut core::ffi::c_void,
        param_type: u32,
        index: u32,
        data: *mut core::ffi::c_void,
        size: *mut u32,
    ) -> i32;
    fn CertCreateCertificateContext(
        encoding: u32,
        data: *const u8,
        size: u32,
    ) -> *mut CertContext;
    fn CertGetNameStringW(
        context: *const CertContext,
        name_type: u32,
        flags: u32,
        type_para: *const core::ffi::c_void,
        name: *mut u16,
        name_size: u32,
    ) -> u32;
    fn CryptHashPublicKeyInfo(
        provider: usize,
        algorithm: u32,
        flags: u32,
        encoding: u32,
        info: *const PublicKeyInfo,
        hash: *mut u8,
        hash_size: *mut u32,
    ) -> i32;
    fn CertFreeCertificateContext(context: *const CertContext) -> i32;
    fn CertCloseStore(store: *mut core::ffi::c_void, flags: u32) -> i32;
    fn CryptMsgClose(message: *mut core::ffi::c_void) -> i32;
}

/// Read the signer's simple name and public-key digest out of an embedded
/// Authenticode signature without validating the chain.
#[cfg(target_os = "windows")]
fn signer_certificate(path: &Path) -> Option<(String, [u8; 32])> {
    const CERT_QUERY_OBJECT_FILE: u32 = 0x0000_0001;
    const CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED: u32 = 0x0000_0400;
    const CERT_QUERY_FORMAT_FLAG_BINARY: u32 = 0x0000_0002;
    const CMSG_CERT_PARAM: u32 = 12;
    const ENCODINGS: u32 = 0x0001_0001; // X509_ASN_ENCODING | PKCS_7_ASN_ENCODING
    const CERT_NAME_SIMPLE_DISPLAY_TYPE: u32 = 4;
    const CALG_SHA_256: u32 = 0x0000_800C;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut encoding = 0u32;
    let mut content_type = 0u32;
    let mut format_type = 0u32;
    let mut store: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut message: *mut core::ffi::c_void = std::ptr::null_mut();
    let mut context: *mut core::ffi::c_void = std::ptr::null_mut();

    let queried = unsafe {
        CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide.as_ptr() as *const core::ffi::c_void,
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_BINARY,
            0,
            &mut encoding,
            &mut content_type,
            &mut format_type,
            &mut store,
            &mut message,
            &mut context,
        )
    };
    if queried == 0 {
        return None;
    }

    let outcome = (|| {
        let mut size = 0u32;
        unsafe { CryptMsgGetParam(message, CMSG_CERT_PARAM, 0, std::ptr::null_mut(), &mut size) };
        if size == 0 || size > 0x1_0000 {
            return None;
        }
        let mut buffer = vec![0u8; size as usize];
        let copied = unsafe {
            CryptMsgGetParam(
                message,
                CMSG_CERT_PARAM,
                0,
                buffer.as_mut_ptr() as *mut core::ffi::c_void,
                &mut size,
            )
        };
        if copied == 0 {
            return None;
        }

        let certificate = unsafe { CertCreateCertificateContext(ENCODINGS, buffer.as_ptr(), size) };
        if certificate.is_null() {
            return None;
        }

        let details = (|| {
            let name_length = unsafe {
                CertGetNameStringW(
                    certificate,
                    CERT_NAME_SIMPLE_DISPLAY_TYPE,
                    0,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    0,
                )
            };
            if name_length <= 1 {
                return None;
            }
            let mut name = vec![0u16; name_length as usize];
            let written = unsafe {
                CertGetNameStringW(
                    certificate,
                    CERT_NAME_SIMPLE_DISPLAY_TYPE,
                    0,
                    std::ptr::null(),
                    name.as_mut_ptr(),
                    name_length,
                )
            };
            if written == 0 {
                return None;
            }
            let length = name.iter().position(|&unit| unit == 0).unwrap_or(name.len());
            let name = String::from_utf16(&name[..length]).ok()?;

            let info = unsafe { (*certificate).info };
            if info.is_null() {
                return None;
            }
            let mut hash = [0u8; 32];
            let mut hash_size = 32u32;
            let hashed = unsafe {
                CryptHashPublicKeyInfo(
                    0,
                    CALG_SHA_256,
                    0,
                    ENCODINGS,
                    std::ptr::addr_of!((*info).public_key),
                    hash.as_mut_ptr(),
                    &mut hash_size,
                )
            };
            if hashed == 0 || hash_size != 32 {
                return None;
            }
            Some((name, hash))
        })();

        unsafe { CertFreeCertificateContext(certificate) };
        details
    })();

    unsafe {
        if !message.is_null() {
            CryptMsgClose(message);
        }
        if !store.is_null() {
            CertCloseStore(store, 0);
        }
    }
    outcome
}

/// Minimal xorshift64 PRNG, seeded from the clock and process id.
struct Rng {
    state: u64,
}

impl Rng {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(0x9e37_79b9_7f4a_7c15);
        let seed = nanos ^ ((std::process::id() as u64) << 32) ^ 0x2545_f491_4f6c_dd1d;
        Self { state: seed | 1 }
    }

    fn next_u64(&mut self) -> u64 {
        let mut value = self.state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.state = value;
        value
    }

    fn fill(&mut self, buffer: &mut [u8]) {
        for chunk in buffer.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}

fn uuid4(rng: &mut Rng) -> String {
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

/// Compact SHA-256 (FIPS 180-4), used for the fingerprint and update digests.
pub(crate) fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = data.len().wrapping_mul(8) as u64;
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (index, word) in chunk.chunks_exact(4).enumerate() {
            schedule[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let mut working = state;
        for index in 0..64 {
            let s1 = working[4].rotate_right(6)
                ^ working[4].rotate_right(11)
                ^ working[4].rotate_right(25);
            let choice = (working[4] & working[5]) ^ (!working[4] & working[6]);
            let temp1 = working[7]
                .wrapping_add(s1)
                .wrapping_add(choice)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = working[0].rotate_right(2)
                ^ working[0].rotate_right(13)
                ^ working[0].rotate_right(22);
            let majority = (working[0] & working[1]) ^ (working[0] & working[2]) ^ (working[1] & working[2]);
            let temp2 = s0.wrapping_add(majority);

            working[7] = working[6];
            working[6] = working[5];
            working[5] = working[4];
            working[4] = working[3].wrapping_add(temp1);
            working[3] = working[2];
            working[2] = working[1];
            working[1] = working[0];
            working[0] = temp1.wrapping_add(temp2);
        }

        for index in 0..8 {
            state[index] = state[index].wrapping_add(working[index]);
        }
    }

    let mut digest = [0u8; 32];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    #[test]
    fn sha256_matches_known_vector() {
        let digest = super::sha256(b"abc");
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn super_props_encode_as_base64_json() {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        let encoded = super::encode_super_props(&super::local_super_props());
        let raw = BASE64.decode(encoded).expect("base64");
        let value: serde_json::Value = serde_json::from_slice(&raw).expect("json");
        assert_eq!(value["os"], "Windows");
        assert_eq!(value["client_build_number"], super::CLIENT_BUILD_NUMBER);
    }

    /// Manual check against the executables installed on this machine.
    #[test]
    #[ignore = "reads a local executable and the registry"]
    fn detects_local_fingerprint() {
        use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
        let Some((path, value)) = super::detect_fingerprint("123456789012345678") else {
            println!("no fingerprint detected");
            return;
        };
        let mut decoded = BASE64.decode(&value).expect("base64");
        let key = super::sha256(b"123456789012345678https://discord.com/careers");
        for index in 1..decoded.len() {
            decoded[index] ^= key[(index - 1) & 31];
        }
        println!("source: {}", path.display());
        println!("length: {}", decoded.len());
        println!("flags: {} (1 = signed, 2 = PE32+)", decoded[1]);
    }

    #[test]
    fn each_game_gets_a_full_event_triplet() {
        let session = super::Session {
            heartbeat_session: "session".into(),
            launch_signature: "signature".into(),
            super_props: String::new(),
        };
        let game = super::Game { id: 42, name: "Test".into(), executable: "test.exe".into() };
        let mut rng = super::Rng::new();
        let mut sequence = 0u64;

        let events =
            super::game_events(&game, "fingerprint", 86_400_000, &mut sequence, &session, &mut rng);

        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["type"], "running_game_heartbeat");
        assert_eq!(events[0]["properties"]["initial_heartbeat"], true);
        assert_eq!(events[1]["type"], "launch_game");
        assert_eq!(events[1]["properties"]["executable_fingerprint"], "fingerprint");
        assert_eq!(events[2]["properties"]["final_heartbeat"], true);
        assert_eq!(events[2]["properties"]["duration_tracked_ms"], 86_400_000u64);
        assert_eq!(sequence, 3);
    }

    #[test]
    fn playtime_budget_is_split_across_games() {
        assert_eq!(super::hours_per_game(6000.0, 250, 10_000.0), 24.0);
        assert_eq!(super::hours_per_game(6000.0, 10_000, 10_000.0), 0.6);
        assert_eq!(super::hours_per_game(6000.0, 250, 12.0), 12.0);
        assert_eq!(super::hours_per_game(6000.0, 1_000_000, 10_000.0), super::MIN_GAME_HOURS);
    }

    #[test]
    fn max_safe_hours_follows_account_age() {
        let now = super::now_ms();
        let young_id =
            ((now - 10 * 3_600_000 - super::DISCORD_EPOCH_MS) << 22).to_string();
        let ten_hours = super::max_safe_hours(&young_id);
        assert!((ten_hours - 10.0).abs() < 0.1, "got {ten_hours}");

        let old_id =
            ((1_546_300_800_000u64 - super::DISCORD_EPOCH_MS) << 22).to_string();
        assert_eq!(super::max_safe_hours(&old_id), super::MAX_SAFE_HOURS);
    }

    #[test]
    fn rotation_prefers_unclaimed_games() {
        let order: Vec<String> = (1..=5).map(|id| id.to_string()).collect();

        let (selected, used, fresh) = super::select_rotation(&order, &[], 3);
        assert_eq!(selected, vec!["1", "2", "3"]);
        assert_eq!(fresh, 3);
        assert_eq!(used, vec!["1", "2", "3"]);

        let (selected, next_used, fresh) = super::select_rotation(&order, &used, 3);
        assert_eq!(selected, vec!["4", "5", "2"]);
        assert_eq!(fresh, 2);
        assert_eq!(next_used, vec!["1", "3", "4", "5", "2"]);
    }

    #[test]
    fn rotation_recycles_when_everything_is_claimed() {
        let order: Vec<String> = (1..=3).map(|id| id.to_string()).collect();
        let used = order.clone();

        let (selected, next_used, fresh) = super::select_rotation(&order, &used, 2);
        assert_eq!(selected, vec!["3", "1"]);
        assert_eq!(fresh, 0);
        assert_eq!(next_used, vec!["2", "3", "1"]);
    }

    #[test]
    fn rotation_drops_ids_missing_from_the_catalog() {
        let order: Vec<String> = (1..=3).map(|id| id.to_string()).collect();
        let used = vec!["99".to_string(), "1".to_string()];

        let (selected, next_used, fresh) = super::select_rotation(&order, &used, 2);
        assert_eq!(selected, vec!["2", "3"]);
        assert_eq!(fresh, 2);
        assert_eq!(next_used, vec!["1", "2", "3"]);
    }

    #[test]
    fn uuid_has_version_4_shape() {
        let mut rng = super::Rng::new();
        let value = super::uuid4(&mut rng);
        assert_eq!(value.len(), 36);
        assert_eq!(&value[14..15], "4");
        assert!(matches!(&value[19..20], "8" | "9" | "a" | "b"));
    }
}
