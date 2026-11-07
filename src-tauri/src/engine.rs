//! Native Discord user-gateway engine.
//!
//! Replaces the Python `vc_keeper` process: it keeps a user account online,
//! applies status/activity/platform settings, tracks the account's sessions and
//! publishes a live snapshot for the Tauri UI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::client_async_tls_with_config;
use tokio_tungstenite::tungstenite::Message;

const GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const PROPERTIES_API: &str = "https://cordapi.dolfi.es/api/v2/properties";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36";
const CHROME_VERSION: &str = "148.0.0.0";
const BUILD_NUMBER: u64 = 444_000;

const MAX_BACKOFF_S: u64 = 60;

// ---------------------------------------------------------------------- config

#[derive(Clone, Debug)]
pub struct Config {
    pub token: String,
    pub log_level: String,
    pub status: String,
    pub activity_type: String,
    pub activity_text: String,
    pub custom_status: String,
    pub platform: String,
}

impl Config {
    pub fn from_map(env: &std::collections::HashMap<String, String>) -> Result<Self, String> {
        let get = |key: &str| env.get(key).map(String::as_str).unwrap_or("").trim().to_string();
        let mut problems: Vec<String> = Vec::new();

        let token = get("DISCORD_TOKEN");
        if token.is_empty() {
            problems.push("DISCORD_TOKEN is missing".into());
        }

        let log_level = {
            let value = get("LOG_LEVEL").to_uppercase();
            if value.is_empty() {
                "INFO".to_string()
            } else if !matches!(value.as_str(), "DEBUG" | "INFO" | "WARNING" | "ERROR") {
                problems.push("LOG_LEVEL must be DEBUG, INFO, WARNING or ERROR".into());
                value
            } else {
                value
            }
        };

        let status = get("STATUS").to_lowercase();
        if !status.is_empty() && !matches!(status.as_str(), "online" | "idle" | "dnd" | "invisible")
        {
            problems.push("STATUS must be empty, online, idle, dnd or invisible".into());
        }

        let activity_type = get("ACTIVITY_TYPE").to_lowercase();
        if !activity_type.is_empty()
            && !matches!(
                activity_type.as_str(),
                "playing" | "listening" | "watching" | "competing" | "custom"
            )
        {
            problems.push(
                "ACTIVITY_TYPE must be playing, listening, watching, competing or custom".into(),
            );
        }

        let platform = {
            let value = get("PLATFORM").to_lowercase();
            let value = match value.as_str() {
                "" => "off".to_string(),
                "desktop" => "windows".to_string(),
                "macos" => "mac".to_string(),
                "mobile" => "ios".to_string(),
                other => other.to_string(),
            };
            if !matches!(
                value.as_str(),
                "off" | "windows" | "mac" | "linux" | "web" | "android" | "ios" | "xbox"
                    | "playstation" | "vr"
            ) {
                problems.push(
                    "PLATFORM must be off, windows, mac, linux, web, android, ios, xbox, playstation or vr"
                        .into(),
                );
            }
            value
        };

        if !problems.is_empty() {
            return Err(problems.join("; "));
        }

        Ok(Self {
            token,
            log_level,
            status,
            activity_type,
            activity_text: get("ACTIVITY_TEXT"),
            custom_status: get("CUSTOM_STATUS"),
            platform,
        })
    }

    fn presence_key(&self) -> (String, String, String, String) {
        (
            self.status.clone(),
            self.activity_type.clone(),
            self.activity_text.clone(),
            self.custom_status.clone(),
        )
    }
}

// ---------------------------------------------------------------------- shared

#[derive(Clone, Serialize)]
pub struct Session {
    pub client: String,
    pub os: String,
    pub status: String,
    pub active: bool,
}

#[derive(Clone)]
pub struct Snapshot {
    pub running: bool,
    pub connected: bool,
    pub pid: Option<u32>,
    pub user: Option<Value>,
    pub sessions: Vec<Session>,
    pub spoofed_platform: Option<String>,
    pub updated_at: f64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            running: false,
            connected: false,
            pid: None,
            user: None,
            sessions: Vec::new(),
            spoofed_platform: None,
            updated_at: now(),
        }
    }
}

#[derive(Default)]
pub struct LogStore {
    pub seq: u64,
    pub lines: std::collections::VecDeque<String>,
}

pub struct Shared {
    pub snapshot: Arc<RwLock<Snapshot>>,
    pub logs: Arc<Mutex<LogStore>>,
    pub platform: RwLock<String>,
    log_level: String,
}

impl Shared {
    pub fn new(
        snapshot: Arc<RwLock<Snapshot>>,
        logs: Arc<Mutex<LogStore>>,
        log_level: impl Into<String>,
    ) -> Self {
        Self {
            snapshot,
            logs,
            platform: RwLock::new("off".into()),
            log_level: log_level.into(),
        }
    }

    pub fn log(&self, level: &str, message: impl AsRef<str>) {
        if level == "DEBUG" && self.log_level != "DEBUG" {
            return;
        }
        let line = format!("{} {:<7} discord-toolbox: {}", clock(), level, message.as_ref());
        eprintln!("{line}");
        if let Ok(mut store) = self.logs.lock() {
            store.seq = store.seq.wrapping_add(1);
            store.lines.push_back(line);
            while store.lines.len() > 500 {
                store.lines.pop_front();
            }
        }
    }

    fn publish(&self) {
        if let Ok(mut snapshot) = self.snapshot.write() {
            snapshot.updated_at = now();
        }
    }

    fn set_running(&self, running: bool) {
        if let Ok(mut snapshot) = self.snapshot.write() {
            snapshot.running = running;
            if !running {
                snapshot.connected = false;
            }
            snapshot.pid = if running {
                Some(std::process::id())
            } else {
                None
            };
            snapshot.updated_at = now();
            if !running {
                snapshot.sessions.clear();
            }
        }
    }

    fn set_connected(&self, connected: bool) {
        if let Ok(mut snapshot) = self.snapshot.write() {
            snapshot.connected = connected;
            snapshot.updated_at = now();
        }
    }

    fn set_user(&self, user: Value) {
        if let Ok(mut snapshot) = self.snapshot.write() {
            snapshot.user = Some(user);
            snapshot.updated_at = now();
        }
    }

    fn set_sessions(&self, sessions: Vec<Session>) {
        if let Ok(mut snapshot) = self.snapshot.write() {
            snapshot.sessions = sessions;
            snapshot.updated_at = now();
        }
    }

    fn set_platform(&self, platform: &str) {
        if let Ok(mut platform_guard) = self.platform.write() {
            *platform_guard = platform.to_string();
        }
        if let Ok(mut snapshot) = self.snapshot.write() {
            snapshot.spoofed_platform = Some(platform.to_string());
            snapshot.updated_at = now();
        }
    }
}

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs_f64())
        .unwrap_or(0.0)
}

fn clock() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    let day = seconds % 86_400;
    format!("{:02}:{:02}:{:02}", day / 3600, (day % 3600) / 60, day % 60)
}

// ---------------------------------------------------------------------- identity

/// The `properties` object Discord turns into a gateway session's client/os.
async fn identity(client: &reqwest::Client, config: &Config) -> (Value, Value) {
    let flavor = match config.platform.as_str() {
        "windows" => "windows",
        "mac" => "mac",
        "linux" => "linux",
        "android" | "ios" | "xbox" | "playstation" | "vr" => "web",
        _ => "web",
    };

    if let Some(data) = fetch_properties(client, flavor).await {
        let properties = data.get("properties").cloned().unwrap_or_else(|| fallback_properties());
        let extra = data
            .get("extra_gateway_properties")
            .cloned()
            .unwrap_or_else(|| json!({}));
        return (properties, extra);
    }

    (fallback_properties(), json!({}))
}

async fn fetch_properties(client: &reqwest::Client, flavor: &str) -> Option<Value> {
    let response = client
        .post(format!("{PROPERTIES_API}/{flavor}"))
        .header("User-Agent", "discord-toolbox/1.0.8")
        .timeout(Duration::from_secs(4))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok()
}

fn fallback_properties() -> Value {
    json!({
        "os": "Windows",
        "browser": "Chrome",
        "device": "",
        "system_locale": "en-US",
        "browser_user_agent": USER_AGENT,
        "browser_version": CHROME_VERSION,
        "os_version": "10",
        "referrer": "",
        "referring_domain": "",
        "referrer_current": "",
        "referring_domain_current": "",
        "release_channel": "stable",
        "client_build_number": BUILD_NUMBER,
        "client_event_source": null,
        "has_client_mods": false,
        "client_app_state": "unfocused",
    })
}

fn apply_platform_overrides(properties: &mut Value, extra: &mut Value, platform: &str) {
    let overrides = match platform {
        "android" => json!({ "browser": "Discord Android", "os": "Android" }),
        "ios" => json!({ "browser": "Discord iOS", "os": "iOS" }),
        "xbox" | "playstation" => json!({ "browser": "Discord Embedded" }),
        "vr" => json!({ "browser": "Discord VR" }),
        _ => Value::Null,
    };
    let Some(overrides) = overrides.as_object() else {
        return;
    };
    if let Some(properties) = properties.as_object_mut() {
        for (key, value) in overrides {
            properties.insert(key.clone(), value.clone());
        }
    }
    if let Some(extra) = extra.as_object_mut() {
        for (key, value) in overrides {
            extra.insert(key.clone(), value.clone());
        }
    }
}

fn activity_payload(config: &Config) -> Option<Value> {
    if !config.activity_type.is_empty() && !config.activity_text.is_empty() {
        let activity_type = match config.activity_type.as_str() {
            "playing" => 0,
            "listening" => 2,
            "watching" => 3,
            "competing" => 5,
            "custom" => 4,
            _ => return None,
        };
        return Some(json!({ "type": activity_type, "name": config.activity_text }));
    }
    if !config.custom_status.is_empty() {
        return Some(json!({ "type": 4, "name": config.custom_status }));
    }
    None
}

fn presence_payload(config: &Config) -> Value {
    let status = if config.status.is_empty() {
        "online"
    } else {
        config.status.as_str()
    };
    let activities: Vec<Value> = activity_payload(config).into_iter().collect();
    json!({
        "status": status,
        "activities": activities,
        "afk": false,
        "since": 0,
    })
}

fn identify_payload(config: &Config, properties: Value, extra: Value) -> Value {
    let mut merged = properties;
    if let (Some(base), Some(extra)) = (merged.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            base.insert(key.clone(), value.clone());
        }
    }
    json!({
        "op": 2,
        "d": {
            "token": config.token,
            "capabilities": 30717,
            "properties": merged,
            "presence": presence_payload(config),
            "compress": false,
            "client_state": { "guild_versions": {} },
        }
    })
}

fn presence_update(config: &Config) -> Value {
    let mut presence = presence_payload(config);
    let empty = presence
        .get("activities")
        .and_then(Value::as_array)
        .is_some_and(|activities| activities.is_empty());
    if empty {
        if let Some(object) = presence.as_object_mut() {
            object.remove("activities");
        }
    }
    json!({ "op": 3, "d": presence })
}

// ---------------------------------------------------------------------- gateway

struct SessionEnd {
    code: Option<u16>,
    fatal: bool,
}

/// Run the engine until cancelled. Reconnects with exponential backoff.
pub async fn run(
    config: Config,
    shared: Arc<Shared>,
    cancel: Arc<AtomicBool>,
    env_path: std::path::PathBuf,
) {
    shared.set_platform(&config.platform);
    shared.log("INFO", "starting discord-toolbox");
    shared.set_running(true);
    shared.set_connected(false);

    let client = match reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            shared.log("ERROR", format!("could not build HTTP client: {error}"));
            shared.set_running(false);
            return;
        }
    };

    let config = Arc::new(Mutex::new(config));
    let (presence_tx, mut presence_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    {
        let config = config.clone();
        let shared = shared.clone();
        let cancel = cancel.clone();
        tokio::spawn(watch_config(env_path, config, shared, presence_tx, cancel));
    }

    let mut backoff: u64 = 5;
    loop {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        let current = config.lock().map(|value| value.clone()).unwrap_or_else(|_| unreachable!());

        match connect_once(&client, &current, &shared, &cancel, &mut presence_rx).await {
            Ok(end) => {
                if end.fatal {
                    shared.log(
                        "ERROR",
                        "discord rejected the token. not restarting. get a fresh token and press Start.",
                    );
                    break;
                }
                if cancel.load(Ordering::SeqCst) {
                    break;
                }
                match end.code {
                    Some(code) => {
                        shared.log("WARNING", format!("gateway closed (code {code}), reconnecting in {backoff}s"))
                    }
                    None => shared.log("WARNING", format!("gateway closed, reconnecting in {backoff}s")),
                }
            }
            Err(error) => {
                shared.log("ERROR", format!("gateway error: {error}, reconnecting in {backoff}s"));
            }
        }

        shared.set_connected(false);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(MAX_BACKOFF_S);
    }

    shared.set_connected(false);
    shared.set_running(false);
    shared.log("INFO", "stopped");
}

/// Open a TCP stream to Discord, honoring the usual HTTP proxy environment
/// variables. The previous direct connection was reset on networks that
/// require the proxy, which left the engine stuck in "Stopped".
async fn open_gateway_tcp(shared: &Shared) -> Result<TcpStream, String> {
    if let Some(proxy) = proxy_env() {
        match proxy_connect(&proxy).await {
            Ok(stream) => {
                shared.log("DEBUG", "gateway connected through proxy");
                return Ok(stream);
            }
            Err(error) => {
                shared.log("WARNING", format!("proxy connection failed: {error}; trying direct"));
            }
        }
    }

    tokio::time::timeout(
        Duration::from_secs(15),
        TcpStream::connect(("gateway.discord.gg", 443)),
    )
    .await
    .map_err(|_| "timed out opening gateway socket".to_string())?
    .map_err(|error| format!("could not open gateway socket: {error}"))
}

fn proxy_env() -> Option<String> {
    ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.trim().is_empty())
}

async fn proxy_connect(proxy: &str) -> Result<TcpStream, String> {
    let raw = if proxy.contains("://") {
        proxy.to_string()
    } else {
        format!("http://{proxy}")
    };
    let url = reqwest::Url::parse(&raw).map_err(|error| format!("invalid proxy URL: {error}"))?;
    if url.scheme() != "http" {
        return Err(format!("unsupported proxy scheme {}", url.scheme()));
    }
    let host = url.host_str().ok_or("proxy URL has no host")?;
    let port = url.port_or_known_default().unwrap_or(80);

    let mut stream = tokio::time::timeout(
        Duration::from_secs(15),
        TcpStream::connect((host, port)),
    )
    .await
    .map_err(|_| "timed out connecting to proxy".to_string())?
    .map_err(|error| format!("could not connect to proxy: {error}"))?;

    let mut auth = String::new();
    if !url.username().is_empty() {
        let credentials = format!("{}:{}", url.username(), url.password().unwrap_or(""));
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        auth = format!("Proxy-Authorization: Basic {encoded}\r\n");
    }
    let request = format!(
        "CONNECT gateway.discord.gg:443 HTTP/1.1\r\nHost: gateway.discord.gg:443\r\nProxy-Connection: Keep-Alive\r\n{auth}\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|error| format!("proxy handshake write failed: {error}"))?;

    let mut response = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        stream
            .read_exact(&mut byte)
            .await
            .map_err(|error| format!("proxy handshake read failed: {error}"))?;
        response.push(byte[0]);
        if response.len() > 8192 {
            return Err("proxy sent an oversized response".into());
        }
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8_lossy(&response);
    if !head.starts_with("HTTP/1.0 200") && !head.starts_with("HTTP/1.1 200") {
        let status = head.lines().next().unwrap_or("unknown proxy response");
        return Err(format!("proxy rejected CONNECT: {status}"));
    }
    Ok(stream)
}

async fn connect_once(
    client: &reqwest::Client,
    config: &Config,
    shared: &Arc<Shared>,
    cancel: &Arc<AtomicBool>,
    presence_rx: &mut tokio::sync::mpsc::UnboundedReceiver<Value>,
) -> Result<SessionEnd, String> {
    let stream = open_gateway_tcp(shared).await?;
    let (mut socket, _) = client_async_tls_with_config(GATEWAY_URL, stream, None, None)
        .await
        .map_err(|error| format!("could not connect: {error}"))?;

    let hello = read_json(&mut socket, cancel).await?;
    if hello.get("op").and_then(Value::as_u64) != Some(10) {
        return Err("gateway did not send HELLO".into());
    }
    let heartbeat_ms = hello
        .pointer("/d/heartbeat_interval")
        .and_then(Value::as_u64)
        .unwrap_or(41_250);
    let mut heartbeat = tokio::time::interval(Duration::from_millis(heartbeat_ms.max(1_000)));
    heartbeat.tick().await;

    let (properties, extra) = identity(client, config).await;
    let mut properties = properties;
    let mut extra = extra;
    apply_platform_overrides(&mut properties, &mut extra, &config.platform);
    let identify = identify_payload(config, properties, extra);
    send_json(&mut socket, identify).await?;
    shared.log("DEBUG", "sent IDENTIFY");

    let mut sequence: Option<u64> = None;
    let mut session_id: Option<String> = None;
    let mut ready = false;

    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = socket.close(None).await;
            return Ok(SessionEnd { code: None, fatal: false });
        }

        tokio::select! {
            update = presence_rx.recv() => {
                if let Some(update) = update {
                    send_json(&mut socket, update).await?;
                    shared.publish();
                }
            }
            _ = heartbeat.tick() => {
                let payload = json!({ "op": 1, "d": sequence });
                if let Err(error) = send_json(&mut socket, payload).await {
                    return Err(format!("heartbeat failed: {error}"));
                }
            }
            message = socket.next() => {
                let Some(message) = message else {
                    return Ok(SessionEnd { code: None, fatal: false });
                };
                let message = message.map_err(|error| format!("socket error: {error}"))?;
                match message {
                    Message::Text(text) => {
                        let value: Value = serde_json::from_str(&text)
                            .map_err(|error| format!("invalid gateway JSON: {error}"))?;
                        if let Some(seq) = value.get("s").and_then(Value::as_u64) {
                            sequence = Some(seq);
                        }
                        let op = value.get("op").and_then(Value::as_u64).unwrap_or(0);
                        match op {
                            0 => {
                                let event = value.get("t").and_then(Value::as_str).unwrap_or("");
                                let data = value.get("d").cloned().unwrap_or(Value::Null);
                                match event {
                                    "READY" => {
                                        ready = true;
                                        shared.set_connected(true);
                                        session_id = data.get("session_id").and_then(Value::as_str).map(str::to_string);
                                        if let Some(user) = data.get("user") {
                                            shared.set_user(profile_from_user(user));
                                            let name = user.get("username").and_then(Value::as_str).unwrap_or("unknown");
                                            let id = user.get("id").and_then(Value::as_str).unwrap_or("");
                                            shared.log("INFO", format!("logged in as {name} ({id})"));
                                        }
                                        if let Some(sessions) = data.get("sessions").and_then(Value::as_array) {
                                            shared.set_sessions(parse_sessions(sessions));
                                        }
                                        if !config.status.is_empty() || activity_payload(config).is_some() {
                                            send_json(&mut socket, presence_update(config)).await?;
                                            shared.log("DEBUG", "presence updated");
                                        } else {
                                            shared.log("INFO", "no status or activity configured, leaving presence untouched");
                                        }
                                        shared.publish();
                                    }
                                    "RESUMED" => {
                                        ready = true;
                                        shared.set_connected(true);
                                        shared.log("INFO", "gateway session resumed");
                                    }
                                    "SESSIONS_REPLACE" => {
                                        if let Some(sessions) = data.as_array() {
                                            shared.set_sessions(parse_sessions(sessions));
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            1 => {
                                send_json(&mut socket, json!({ "op": 1, "d": sequence })).await?;
                            }
                            7 => return Ok(SessionEnd { code: None, fatal: false }),
                            9 => {
                                let resumable = value.get("d").and_then(Value::as_bool).unwrap_or(false);
                                if resumable {
                                    if let (Some(session_id), Some(sequence)) = (session_id.as_ref(), sequence) {
                                        send_json(&mut socket, json!({
                                            "op": 6,
                                            "d": { "token": config.token, "session_id": session_id, "seq": sequence }
                                        })).await?;
                                        shared.log("INFO", "session invalid, resuming");
                                        continue;
                                    }
                                }
                                session_id = None;
                                sequence = None;
                                ready = false;
                                send_json(&mut socket, identify_payload(config, fallback_properties(), json!({}))).await?;
                                shared.log("INFO", "session invalid, re-identifying");
                            }
                            _ => {}
                        }
                    }
                    Message::Close(frame) => {
                        let code = frame.as_ref().map(|frame| u16::from(frame.code));
                        let fatal = matches!(code, Some(4004) | Some(4010) | Some(4011) | Some(4012) | Some(4013) | Some(4014));
                        if !ready {
                            shared.log("WARNING", "gateway closed before READY (check the token)");
                        }
                        return Ok(SessionEnd { code, fatal });
                    }
                    Message::Ping(payload) => {
                        let _ = socket.send(Message::Pong(payload)).await;
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn read_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    cancel: &Arc<AtomicBool>,
) -> Result<Value, String> {
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("cancelled".into());
        }
        match tokio::time::timeout(Duration::from_secs(20), socket.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                return serde_json::from_str(&text).map_err(|error| error.to_string());
            }
            Ok(Some(Ok(Message::Ping(payload)))) => {
                let _ = socket.send(Message::Pong(payload)).await;
            }
            Ok(Some(Ok(_))) => continue,
            Ok(Some(Err(error))) => return Err(error.to_string()),
            Ok(None) => return Err("gateway closed".into()),
            Err(_) => return Err("timed out waiting for HELLO".into()),
        }
    }
}

async fn send_json(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    value: Value,
) -> Result<(), String> {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|error| error.to_string())
}

fn parse_sessions(sessions: &[Value]) -> Vec<Session> {
    sessions
        .iter()
        .filter(|session| session.get("session_id").and_then(Value::as_str) != Some("all"))
        .map(|session| {
            let info = session.get("client_info").cloned().unwrap_or(Value::Null);
            Session {
                client: info
                    .get("client")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                os: info
                    .get("os")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                status: session
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("offline")
                    .to_string(),
                active: session.get("active").and_then(Value::as_bool).unwrap_or(false),
            }
        })
        .collect()
}

fn profile_from_user(user: &Value) -> Value {
    let id = user.get("id").and_then(Value::as_str).unwrap_or("");
    let avatar = user.get("avatar").and_then(Value::as_str);
    let avatar_url = match avatar {
        Some(hash) if !hash.is_empty() => {
            format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128")
        }
        _ => {
            let discriminator = user.get("discriminator").and_then(Value::as_str).unwrap_or("0");
            if discriminator == "0" {
                let index = id.parse::<u64>().unwrap_or(0) >> 22;
                format!("https://cdn.discordapp.com/embed/avatars/{}.png", index % 6)
            } else {
                format!("https://cdn.discordapp.com/embed/avatars/{}.png", discriminator.parse::<u64>().unwrap_or(0) % 5)
            }
        }
    };
    json!({
        "id": id,
        "username": user.get("username").cloned().unwrap_or(Value::Null),
        "global_name": user.get("global_name").cloned().unwrap_or(Value::Null),
        "avatar_url": avatar_url,
    })
}

// ---------------------------------------------------------------------- watcher

/// Apply `.env` edits to the running gateway without a restart.
pub async fn watch_config(
    path: std::path::PathBuf,
    config: Arc<Mutex<Config>>,
    shared: Arc<Shared>,
    presence: tokio::sync::mpsc::UnboundedSender<Value>,
    cancel: Arc<AtomicBool>,
) {
    let mut last = file_stamp(&path);
    while !cancel.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let current = file_stamp(&path);
        if current.is_none() || current == last {
            continue;
        }
        last = current;

        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let env = parse_env(&content);
        let Ok(next) = Config::from_map(&env) else {
            shared.log("WARNING", "config change ignored: invalid values");
            continue;
        };
        let old = config.lock().map(|value| value.clone()).ok();
        let Some(old) = old else { continue };

        let presence_changed = old.presence_key() != next.presence_key();
        if presence_changed {
            if next.status.is_empty() && activity_payload(&next).is_none() {
                shared.log("INFO", "presence settings cleared; leaving the current presence untouched");
            } else {
                let detail = if !next.status.is_empty() {
                    next.status.clone()
                } else {
                    "keep current".to_string()
                };
                shared.log("INFO", format!("presence updated: {detail}"));
                let _ = presence.send(presence_update(&next));
            }
        }

        let mut needs_restart = Vec::new();
        if old.token != next.token {
            needs_restart.push("token");
        }
        if old.platform != next.platform {
            needs_restart.push("platform");
        }
        if !needs_restart.is_empty() {
            shared.log(
                "INFO",
                format!("{}, restart the bot to apply it", needs_restart.join(", ")),
            );
        }

        if let Ok(mut guard) = config.lock() {
            *guard = next;
        }
        shared.publish();
    }
}

fn file_stamp(path: &std::path::Path) -> Option<u128> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos())
}

pub fn parse_env(content: &str) -> std::collections::HashMap<String, String> {
    let mut values = std::collections::HashMap::new();
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
