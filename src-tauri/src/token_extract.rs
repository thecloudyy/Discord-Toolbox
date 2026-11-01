//! Automatic Discord account detection.
//!
//! Reads the encrypted token the Discord desktop client stores in its Chromium
//! profile (Local State -> DPAPI-protected AES key, Local Storage/leveldb ->
//! AES-256-GCM token payloads), then resolves the profile through the Discord API.
//! This mirrors what the reference Discord Quest Helper does natively, so account
//! detection no longer depends on a Python interpreter being installed.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use serde::Serialize;

#[cfg(target_os = "windows")]
use std::collections::HashSet;
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};

const API_ME: &str = "https://discord.com/api/v9/users/@me";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) discord/1.0.9184 Chrome/134.0.6998.205 Electron/35.2.1 Safari/537.36";

/// Folder under %APPDATA% and the label shown in the account list.
#[cfg(target_os = "windows")]
const CLIENTS: [(&str, &str); 6] = [
    ("discord", "Discord"),
    ("discordcanary", "Discord Canary"),
    ("discordptb", "Discord PTB"),
    ("discorddevelopment", "Discord Development"),
    ("vesktop", "Vesktop"),
    ("Lightcord", "Lightcord"),
];

#[derive(Debug, Clone, Default, Serialize)]
pub struct Profile {
    pub id: Option<String>,
    pub username: Option<String>,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DetectedAccount {
    pub client: String,
    pub token: String,
    #[serde(flatten)]
    pub profile: Profile,
}

#[derive(Debug, Default, Serialize)]
pub struct ScanReport {
    pub accounts: Vec<DetectedAccount>,
    pub notes: Vec<String>,
}

/// Scan every known Discord profile and attach the matching Discord profile.
///
/// Tokens Discord will not resolve (expired entries left in leveldb) are dropped,
/// and two tokens pointing at the same account are collapsed into one.
pub async fn scan_with_profiles() -> ScanReport {
    let (found, mut notes) = scan_local_profiles();
    let mut accounts = Vec::new();
    if found.is_empty() {
        return ScanReport { accounts, notes };
    }

    let client = match build_http_client() {
        Ok(client) => client,
        Err(error) => {
            notes.push(error);
            return ScanReport { accounts, notes };
        }
    };

    let mut seen_ids: Vec<String> = Vec::new();
    for (label, token) in found {
        let profile = match fetch_profile_with(&client, &token).await {
            Ok(profile) => profile,
            Err(error) => {
                notes.push(format!(
                    "{label}: token found, but the profile lookup failed ({error})"
                ));
                continue;
            }
        };
        if let Some(id) = profile.id.as_deref() {
            if seen_ids.iter().any(|seen| seen == id) {
                continue;
            }
            seen_ids.push(id.to_string());
        }
        accounts.push(DetectedAccount { client: label, token, profile });
    }

    if accounts.is_empty() {
        notes.push(
            "No saved account could be verified with Discord. Sign in to Discord, then detect again."
                .to_string(),
        );
    }

    ScanReport { accounts, notes }
}

/// Fetch `GET /users/@me` for a token.
pub async fn fetch_profile(token: &str) -> Result<Profile, String> {
    let client = build_http_client()?;
    fetch_profile_with(&client, token).await
}

fn build_http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| format!("could not create HTTP client: {error}"))
}

struct ProfileError {
    message: String,
    retryable: bool,
}

async fn fetch_profile_with(client: &reqwest::Client, token: &str) -> Result<Profile, String> {
    let mut last_message = String::from("profile lookup failed");
    for attempt in 0..3u32 {
        match fetch_profile_once(client, token).await {
            Ok(profile) => return Ok(profile),
            Err(error) => {
                last_message = error.message;
                if !error.retryable || attempt == 2 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(700 * u64::from(attempt + 1))).await;
            }
        }
    }
    Err(last_message)
}

async fn fetch_profile_once(client: &reqwest::Client, token: &str) -> Result<Profile, ProfileError> {
    let response = client
        .get(API_ME)
        .header("Authorization", token)
        .send()
        .await
        .map_err(|error| ProfileError {
            message: format!("network error: {error}"),
            retryable: true,
        })?;

    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(ProfileError {
            message: "Discord rejected this token".to_string(),
            retryable: false,
        });
    }
    if !status.is_success() {
        return Err(ProfileError {
            message: format!("Discord replied with HTTP {}", status.as_u16()),
            retryable: status.as_u16() == 429 || status.is_server_error(),
        });
    }

    let body: serde_json::Value = response.json().await.map_err(|error| ProfileError {
        message: format!("could not read Discord's response: {error}"),
        retryable: true,
    })?;
    Ok(profile_from_json(&body))
}

fn profile_from_json(value: &serde_json::Value) -> Profile {
    let id = value
        .get("id")
        .and_then(|field| field.as_str())
        .unwrap_or_default()
        .to_string();
    let username = value
        .get("username")
        .and_then(|field| field.as_str())
        .map(str::to_string);
    let global_name = value
        .get("global_name")
        .and_then(|field| field.as_str())
        .map(str::to_string);
    let avatar = value
        .get("avatar")
        .and_then(|field| field.as_str())
        .filter(|hash| !hash.is_empty());

    let avatar_url = if id.is_empty() {
        None
    } else {
        Some(match avatar {
            Some(hash) => format!("https://cdn.discordapp.com/avatars/{id}/{hash}.png?size=128"),
            None => {
                let index = id.parse::<u64>().map(|snowflake| (snowflake >> 22) % 6).unwrap_or(0);
                format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
            }
        })
    };

    Profile {
        id: (!id.is_empty()).then_some(id),
        username,
        global_name,
        avatar_url,
    }
}

#[cfg(target_os = "windows")]
fn scan_local_profiles() -> (Vec<(String, String)>, Vec<String>) {
    let mut found = Vec::new();
    let mut notes = Vec::new();
    let mut seen = HashSet::new();

    let Some(appdata) = std::env::var_os("APPDATA").map(PathBuf::from) else {
        notes.push("APPDATA is not set, so Discord profiles cannot be scanned.".to_string());
        return (found, notes);
    };

    let mut any_profile = false;
    for (folder, label) in CLIENTS {
        let profile_dir = appdata.join(folder);
        if !profile_dir.is_dir() {
            continue;
        }
        any_profile = true;
        match tokens_from_profile(&profile_dir) {
            Ok(tokens) => {
                if tokens.is_empty() {
                    notes.push(format!(
                        "{label}: no saved token found. Open Discord, sign in, then detect again."
                    ));
                }
                for token in tokens {
                    if seen.insert(token.clone()) {
                        found.push((label.to_string(), token));
                    }
                }
            }
            Err(reason) => notes.push(format!("{label}: {reason}")),
        }
    }

    if !any_profile {
        notes.push(
            "No Discord desktop profile found in %APPDATA%. Install Discord and sign in first."
                .to_string(),
        );
    }

    (found, notes)
}

#[cfg(not(target_os = "windows"))]
fn scan_local_profiles() -> (Vec<(String, String)>, Vec<String>) {
    (
        Vec::new(),
        vec!["Automatic token detection is only available on Windows.".to_string()],
    )
}

#[cfg(target_os = "windows")]
fn profile_master_key(profile_dir: &Path) -> Result<Vec<u8>, String> {
    let state_path = profile_dir.join("Local State");
    if !state_path.is_file() {
        return Err("Local State file is missing".to_string());
    }

    let state_text = std::fs::read_to_string(&state_path)
        .map_err(|error| format!("could not read Local State ({error})"))?;
    let state: serde_json::Value = serde_json::from_str(&state_text)
        .map_err(|_| "Local State is not valid JSON".to_string())?;
    let encoded_key = state
        .pointer("/os_crypt/encrypted_key")
        .and_then(|value| value.as_str())
        .ok_or("Local State has no os_crypt.encrypted_key")?;
    let key_blob = BASE64
        .decode(encoded_key)
        .map_err(|_| "encrypted_key is not valid base64".to_string())?;
    let key_blob = key_blob
        .strip_prefix(b"DPAPI")
        .ok_or("encrypted_key is not DPAPI-protected")?;
    unprotect(key_blob)
}

#[cfg(target_os = "windows")]
fn tokens_from_profile(profile_dir: &Path) -> Result<Vec<String>, String> {
    let leveldb_path = profile_dir.join("Local Storage").join("leveldb");
    let master_key = profile_master_key(profile_dir)?;

    if !leveldb_path.is_dir() {
        return Err("Local Storage/leveldb folder is missing".to_string());
    }

    let pattern = regex::Regex::new(r"dQw4w9WgXcQ:([A-Za-z0-9+/=]+)")
        .expect("token regex is valid");
    let mut tokens: Vec<String> = Vec::new();
    let mut saw_payload = false;

    let entries = std::fs::read_dir(&leveldb_path)
        .map_err(|error| format!("could not read leveldb ({error})"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_data = matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("ldb") | Some("log")
        );
        if !is_data {
            continue;
        }
        let Ok(data) = std::fs::read(&path) else {
            continue;
        };
        let content = String::from_utf8_lossy(&data);
        for capture in pattern.captures_iter(&content) {
            let Some(payload_match) = capture.get(1) else {
                continue;
            };
            let Ok(payload) = BASE64.decode(payload_match.as_str()) else {
                continue;
            };
            saw_payload = true;
            if let Some(token) = decrypt_token(&payload, &master_key) {
                if !tokens.contains(&token) {
                    tokens.push(token);
                }
            }
        }
    }

    if tokens.is_empty() && saw_payload {
        return Err("found an encrypted token but could not decrypt it".to_string());
    }

    Ok(tokens)
}

/// Cookie names that the Discord client sends with API calls.
#[cfg(target_os = "windows")]
const COOKIE_NAMES: [&str; 6] = [
    "__dcfduid",
    "__sdcfduid",
    "dbind",
    "__stripe_mid",
    "cf_clearance",
    "_cfuvid",
];

/// Best-effort cookie header from the local Discord clients. Chromium encrypts
/// cookie values with the same DPAPI-protected key as the account token, so the
/// profile master key is reused for decryption.
#[cfg(target_os = "windows")]
pub fn cookie_header_from_profiles() -> Option<String> {
    let appdata = std::env::var_os("APPDATA").map(PathBuf::from)?;
    let mut found: Vec<(&'static str, String)> = Vec::new();

    for (folder, _) in CLIENTS {
        let profile_dir = appdata.join(folder);
        if !profile_dir.is_dir() {
            continue;
        }
        let Ok(master_key) = profile_master_key(&profile_dir) else {
            continue;
        };
        let network = profile_dir.join("Network");
        for file in ["Cookies", "Cookies-wal"] {
            let Ok(bytes) = std::fs::read(network.join(file)) else {
                continue;
            };
            for name in COOKIE_NAMES {
                if found.iter().any(|(seen, _)| *seen == name) {
                    continue;
                }
                let value = sqlite_cookie_value(&bytes, name, &master_key)
                    .or_else(|| find_cookie(&bytes, name, &master_key));
                if let Some(value) = value {
                    found.push((name, value));
                }
            }
        }
        if found.len() == COOKIE_NAMES.len() {
            break;
        }
    }

    if found.is_empty() {
        return None;
    }
    Some(
        found
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

#[cfg(not(target_os = "windows"))]
pub fn cookie_header_from_profiles() -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn find_cookie(bytes: &[u8], name: &str, master_key: &[u8]) -> Option<String> {
    let needle = name.as_bytes();
    let mut from = 0;
    while from + needle.len() <= bytes.len() {
        let offset = find_bytes(&bytes[from..], needle)?;
        let after = from + offset + needle.len();
        let end = (after + 256).min(bytes.len());
        if let Some(value) = decrypt_cookie_in(&bytes[after..end], master_key) {
            return Some(value);
        }
        from = after;
    }
    None
}

#[cfg(target_os = "windows")]
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

/// Read the cookie straight out of the SQLite `cookies` table. The table stores
/// the value as text in `value` when the client does not encrypt it, and as an
/// AES-GCM blob in `encrypted_value` otherwise.
#[cfg(target_os = "windows")]
fn sqlite_cookie_value(bytes: &[u8], name: &str, master_key: &[u8]) -> Option<String> {
    if bytes.len() < 512 || &bytes[..16] != b"SQLite format 3\0" {
        return None;
    }
    let page_size = match u16::from_be_bytes([bytes[16], bytes[17]]) {
        1 => 65_536,
        size if size >= 512 => size as usize,
        _ => return None,
    };
    let reserved = bytes.get(20).copied().unwrap_or(0) as usize;
    let usable = page_size.checked_sub(reserved)?;
    if usable < 480 {
        return None;
    }

    for page in 0..bytes.len().div_ceil(page_size) {
        let page_start = page * page_size;
        let header_at = page_start + if page == 0 { 100 } else { 0 };
        if header_at + 8 > bytes.len() || bytes[header_at] != 0x0D {
            continue;
        }
        let cell_count =
            u16::from_be_bytes([bytes[header_at + 3], bytes[header_at + 4]]) as usize;
        for cell in 0..cell_count {
            let pointer_at = header_at + 8 + cell * 2;
            if pointer_at + 2 > bytes.len() {
                break;
            }
            let cell_at = page_start
                + u16::from_be_bytes([bytes[pointer_at], bytes[pointer_at + 1]]) as usize;
            let Some((payload_len, length_bytes)) = sqlite_varint(bytes, cell_at) else {
                continue;
            };
            let Some((_, rowid_bytes)) = sqlite_varint(bytes, cell_at + length_bytes) else {
                continue;
            };
            let payload_at = cell_at + length_bytes + rowid_bytes;
            let Some(payload) = sqlite_payload(
                bytes,
                page_size,
                usable,
                payload_at,
                payload_len as usize,
            ) else {
                continue;
            };
            if let Some(value) = cookie_from_record(&payload, name, master_key) {
                return Some(value);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn sqlite_varint(bytes: &[u8], at: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut offset = at;
    for index in 0..9 {
        let byte = *bytes.get(offset)?;
        offset += 1;
        if index == 8 {
            return Some(((value << 8) | byte as u64, offset - at));
        }
        value = (value << 7) | (byte & 0x7F) as u64;
        if byte & 0x80 == 0 {
            return Some((value, offset - at));
        }
    }
    None
}

/// A table-leaf cell may store only part of its record and continue on overflow
/// pages.
#[cfg(target_os = "windows")]
fn sqlite_payload(
    bytes: &[u8],
    page_size: usize,
    usable: usize,
    at: usize,
    length: usize,
) -> Option<Vec<u8>> {
    if length == 0 {
        return None;
    }
    let max_local = usable.saturating_sub(35);
    let min_local = ((usable.saturating_sub(12)) * 32 / 255).saturating_sub(23);
    let local = if length <= max_local {
        length
    } else {
        let candidate = min_local + (length - min_local) % (usable.saturating_sub(4));
        if candidate > max_local { min_local } else { candidate }
    };
    if at + local > bytes.len() {
        return None;
    }

    let mut payload = bytes[at..at + local].to_vec();
    if local == length {
        return Some(payload);
    }

    let mut next = u32::from_be_bytes([
        *bytes.get(at + local)?,
        *bytes.get(at + local + 1)?,
        *bytes.get(at + local + 2)?,
        *bytes.get(at + local + 3)?,
    ]) as usize;
    while payload.len() < length {
        if next == 0 {
            return None;
        }
        let page_start = (next - 1) * page_size;
        if page_start + 4 > bytes.len() {
            return None;
        }
        let next_ptr = u32::from_be_bytes([
            bytes[page_start],
            bytes[page_start + 1],
            bytes[page_start + 2],
            bytes[page_start + 3],
        ]) as usize;
        let take = (length - payload.len()).min(page_size - 4);
        let end = (page_start + 4 + take).min(bytes.len());
        payload.extend_from_slice(&bytes[page_start + 4..end]);
        next = next_ptr;
    }
    (payload.len() == length).then_some(payload)
}

/// Column 3 is the cookie name, 4 its plaintext value and 5 the encrypted blob.
#[cfg(target_os = "windows")]
fn cookie_from_record(record: &[u8], name: &str, master_key: &[u8]) -> Option<String> {
    let (header_len, mut offset) = sqlite_varint(record, 0)?;
    let header_len = header_len as usize;
    if header_len > record.len() || header_len < offset {
        return None;
    }

    let mut serials = Vec::new();
    while offset < header_len {
        let (serial, used) = sqlite_varint(record, offset)?;
        serials.push(serial);
        offset += used;
    }

    let mut data_at = header_len;
    let mut columns = Vec::with_capacity(serials.len());
    for serial in serials {
        let length = match serial {
            0 => 0,
            1 => 1,
            2 => 2,
            3 => 3,
            4 => 4,
            5 => 6,
            6 | 7 => 8,
            8..=11 => 0,
            text_or_blob => ((text_or_blob - 12) / 2) as usize,
        };
        if data_at + length > record.len() {
            return None;
        }
        columns.push((data_at, length));
        data_at += length;
    }

    let column = |index: usize| -> Option<&[u8]> {
        let &(start, length) = columns.get(index)?;
        Some(&record[start..start + length])
    };

    if column(3)? != name.as_bytes() {
        return None;
    }
    if let Some(value) = column(4) {
        if !value.is_empty() {
            if let Ok(text) = std::str::from_utf8(value) {
                if is_cookie_value(text) {
                    return Some(text.to_string());
                }
            }
        }
    }
    if let Some(encrypted) = column(5) {
        if let Some(plain) = decrypt_aes_gcm(encrypted, master_key) {
            if let Some(text) = cookie_text(&plain) {
                return Some(text);
            }
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn is_cookie_value(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 4096
        && text.chars().all(|character| character.is_ascii_graphic())
}

#[cfg(target_os = "windows")]
fn decrypt_aes_gcm(payload: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};

    if payload.len() <= 15 {
        return None;
    }
    if &payload[..3] != b"v10" && &payload[..3] != b"v11" {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let nonce = Nonce::from_slice(&payload[3..15]);
    cipher.decrypt(nonce, &payload[15..]).ok()
}

/// The encrypted value sits right after the cookie name in the SQLite row, but
/// the raw blob length is not visible without parsing the record. Try upcoming
/// v10/v11 blobs with candidate lengths until the AES-GCM tag validates.
#[cfg(target_os = "windows")]
fn decrypt_cookie_in(window: &[u8], master_key: &[u8]) -> Option<String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};

    let cipher = Aes256Gcm::new_from_slice(master_key).ok()?;
    let mut index = 0;
    while index + 3 <= window.len() {
        if &window[index..index + 3] == b"v10" || &window[index..index + 3] == b"v11" {
            let nonce_start = index + 3;
            if nonce_start + 12 + 16 <= window.len() {
                let nonce = Nonce::from_slice(&window[nonce_start..nonce_start + 12]);
                let body = &window[nonce_start + 12..];
                for end in 16..=body.len().min(600) {
                    if let Ok(plain) = cipher.decrypt(nonce, &body[..end]) {
                        if let Some(text) = cookie_text(&plain) {
                            return Some(text);
                        }
                        break;
                    }
                }
            }
        }
        index += 1;
    }
    None
}

#[cfg(target_os = "windows")]
fn cookie_text(plain: &[u8]) -> Option<String> {
    fn tidy(bytes: &[u8]) -> Option<String> {
        let end = bytes.iter().position(|&byte| byte == 0).unwrap_or(bytes.len());
        let text = std::str::from_utf8(&bytes[..end]).ok()?.to_string();
        (!text.is_empty()
            && text
                .chars()
                .all(|character| character.is_ascii_graphic() || character == ' '))
        .then_some(text)
    }

    if let Some(text) = tidy(plain) {
        return Some(text);
    }
    // Newer Chromium versions prefix the plaintext with the host hash.
    if plain.len() > 32 {
        if let Some(text) = tidy(&plain[32..]) {
            return Some(text);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn decrypt_token(payload: &[u8], key: &[u8]) -> Option<String> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};

    if payload.len() <= 15 {
        return None;
    }
    if &payload[..3] != b"v10" && &payload[..3] != b"v11" {
        return None;
    }

    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    let nonce = Nonce::from_slice(&payload[3..15]);
    let plaintext = cipher.decrypt(nonce, &payload[15..]).ok()?;
    let token = String::from_utf8(plaintext).ok()?;

    let looks_like_token = token.len() >= 40
        && token.len() <= 200
        && token.chars().all(|character| character.is_ascii_graphic());
    looks_like_token.then_some(token)
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

#[cfg(target_os = "windows")]
#[link(name = "crypt32")]
extern "system" {
    fn CryptUnprotectData(
        data_in: *const DataBlob,
        description: *mut *mut u16,
        entropy: *const DataBlob,
        reserved: *mut core::ffi::c_void,
        prompt: *mut core::ffi::c_void,
        flags: u32,
        data_out: *mut DataBlob,
    ) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(memory: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    #[test]
    fn decrypts_v10_cookie_blob() {
        use aes_gcm::aead::{Aead, KeyInit};
        use aes_gcm::{Aes256Gcm, Nonce};

        let key = [7u8; 32];
        let cipher = Aes256Gcm::new_from_slice(&key).expect("cipher");
        let nonce_bytes = [3u8; 12];
        let mut plain = [0u8; 32]; // Chromium host-hash prefix
        plain[..11].copy_from_slice(b"hello-world");
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plain.as_slice())
            .expect("encrypt");

        let mut row = b"__dcfduid".to_vec();
        row.extend_from_slice(b"v10");
        row.extend_from_slice(&nonce_bytes);
        row.extend_from_slice(&ciphertext);

        let found = super::find_cookie(&row, "__dcfduid", &key);
        assert_eq!(found.as_deref(), Some("hello-world"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn reads_plaintext_cookie_from_sqlite_page() {
        let name = "__dcfduid";
        let value = "abc123def456";

        // Record with six columns: NULL, NULL, NULL, name, value, empty blob.
        // Header length includes its own size byte, so serials start at offset 1.
        let mut payload = vec![0u8; 7];
        payload[0] = 7;
        payload[4] = 13 + 2 * name.len() as u8;
        payload[5] = 13 + 2 * value.len() as u8;
        payload[6] = 12;
        payload.extend_from_slice(name.as_bytes());
        payload.extend_from_slice(value.as_bytes());

        let page_size = 1024usize;
        let mut page = vec![0u8; page_size];
        page[..16].copy_from_slice(b"SQLite format 3\0");
        page[16..18].copy_from_slice(&(page_size as u16).to_be_bytes());
        page[20] = 0;

        page[100] = 0x0D; // table leaf
        page[103..105].copy_from_slice(&1u16.to_be_bytes()); // one cell
        page[105..107].copy_from_slice(&120u16.to_be_bytes()); // content start
        page[108..110].copy_from_slice(&120u16.to_be_bytes()); // cell pointer

        page[120] = payload.len() as u8;
        page[121] = 1; // rowid
        page[122..122 + payload.len()].copy_from_slice(&payload);

        assert_eq!(
            super::sqlite_cookie_value(&page, name, &[0u8; 32]).as_deref(),
            Some(value)
        );
        assert_eq!(super::sqlite_cookie_value(&page, "dbind", &[0u8; 32]), None);
    }

    /// Manual check against the Discord client signed in on this machine.
    /// Prints counts and token lengths only, never the tokens themselves.
    #[test]
    #[ignore = "reads the signed-in Discord client on this machine"]
    fn detects_local_accounts() {
        let (tokens, notes) = super::scan_local_profiles();
        println!("detected accounts: {}", tokens.len());
        for (client, token) in &tokens {
            println!("{client}: token length {}", token.len());
        }
        for note in notes {
            println!("note: {note}");
        }
    }

    /// Manual check of the cookie header assembled from the local client.
    #[test]
    #[ignore = "reads the signed-in Discord client on this machine"]
    fn detects_local_cookie_header() {
        #[cfg(target_os = "windows")]
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let probe = std::path::PathBuf::from(&appdata)
                .join("discord")
                .join("Network")
                .join("Cookies");
            match std::fs::File::open(&probe) {
                Ok(_) => println!("cookie store is readable"),
                Err(error) => println!("cookie store is locked ({error})"),
            }
        }

        match super::cookie_header_from_profiles() {
            Some(header) => {
                let names: Vec<&str> = header
                    .split("; ")
                    .filter_map(|pair| pair.split('=').next())
                    .collect();
                println!("cookie header found with {} pair(s): {}", names.len(), names.join(", "));
            }
            None => println!("no cookie header found"),
        }
    }

    /// Manual end-to-end check: detection plus the Discord profile lookup.
    #[test]
    #[ignore = "calls the Discord API with the token signed in on this machine"]
    fn resolves_local_account_profile() {
        let (tokens, _) = super::scan_local_profiles();
        let Some((_, token)) = tokens.first() else {
            println!("no local account to resolve");
            return;
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        match runtime.block_on(super::fetch_profile(token)) {
            Ok(profile) => println!(
                "profile lookup ok: id {}, username {}, avatar {}",
                profile.id.is_some(),
                profile.username.is_some(),
                profile.avatar_url.is_some()
            ),
            Err(error) => println!("profile lookup failed: {error}"),
        }
    }
}

#[cfg(target_os = "windows")]
fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.is_empty() {
        return Err("DPAPI blob is empty".to_string());
    }

    let input = DataBlob {
        cb_data: data.len() as u32,
        pb_data: data.as_ptr() as *mut u8,
    };
    let mut output = DataBlob { cb_data: 0, pb_data: std::ptr::null_mut() };

    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("DPAPI decryption failed".to_string());
    }

    let bytes = unsafe {
        std::slice::from_raw_parts(output.pb_data, output.cb_data as usize).to_vec()
    };
    unsafe {
        LocalFree(output.pb_data as *mut core::ffi::c_void);
    }
    Ok(bytes)
}
