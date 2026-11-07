# Discord Toolbox

Keeps your Discord account online, with optional custom status/activity. The
desktop app is Tauri, and both the UI and the Discord gateway engine are written
in Rust. There is no Python runtime, no Node build step and no bundled bot
executable. Automating a user account is against Discord's ToS: use it at your
own risk.

## Features

- Keeps your account online 24/7 from a native Rust gateway client
- Everything optional is off by default: you pick a status, an activity or a platform
- Optional custom status and activity (playing/listening/watching/competing)
- Status modes: online, idle, dnd, invisible
- Optional platform spoofer: Windows, macOS, Linux, web, Android, iOS, Xbox, PlayStation or VR
- Live platform indicators: one status-coloured icon per platform your account is online on
- Desktop app (Tauri + Leptos/WASM) with account auto-detection and a live log
- Logs, sessions and run state are read from the Rust engine directly, no Python state file
- Most settings apply to the running engine immediately; token and `PLATFORM` need a restart
- The app never starts the engine on its own: press **Start** to connect
- Token auto-detection from local Discord profiles (Windows)
- Multi-account selection
- Game badge forger: credits game time and game variety through Discord's own telemetry
- Single forge pass by default, with an optional loop that keeps forging until you press Stop
- HypeSquad manager: switch house, attempt removal, and view profile badges
- Built-in updater fed by GitHub releases (portable package contains only the app executable)

## Disclaimer

By using this code, you are automating your Discord account. This is against
Discord's Terms of Service and Community Guidelines. Your account might get
suspended or terminated. The developer is not responsible for any consequences.
Use at your own risk.

**DO NOT GIVE YOUR DISCORD TOKEN TO ANYONE.**

## Requirements

- Rust 1.80+ and the Tauri prerequisites for your OS
- The `wasm32-unknown-unknown` target
- [Trunk](https://trunkrs.dev/) for building the Leptos UI

```
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
```

## Build

Build the UI first, then the app:

```
cd ui
trunk build --release

cd ../src-tauri
cargo build --release --features custom-protocol
```

The executable lands in `src-tauri/target/release/discord-toolbox.exe`. For a
development loop, run `trunk serve` in `ui/` and `cargo run` in `src-tauri/`
(or install `tauri-cli` and run `cargo tauri dev`).

To build the installer after the executable exists:

```
cd src-tauri
iscc installer.iss
```

The installer lands in `src-tauri/Output/Discord-Toolbox-setup.exe`. It installs
per user and contains no separate bot binary.

## Headless / server

The gateway is also available without a window:

```
cargo run --release --bin discord-toolbox-engine
```

It reads `.env` from the current directory, or from
`%LOCALAPPDATA%\Discord Toolbox` when run from an installed app directory.

## Configuration (.env)

```
DISCORD_TOKEN=
LOG_LEVEL=INFO
# presence is off by default: leave these empty to keep what your account already shows
STATUS=
ACTIVITY_TYPE=
ACTIVITY_TEXT=
CUSTOM_STATUS=
PLATFORM=off
```

Status, activity and custom status are picked up by the running engine within
about a second, no restart needed. Token and `PLATFORM` are only read at login,
so those need a restart (the app's **Restart** button). The log says so when that
happens.

## Platforms

`PLATFORM` picks what Discord thinks you are running. It defaults to `off`, which
sends the identity the client normally uses:

| Value | Shows as |
| --- | --- |
| `off` | No spoofing (default) |
| `windows`, `mac`, `linux` | Desktop on that OS |
| `web` | Browser |
| `android`, `ios` | Mobile on that OS |
| `xbox`, `playstation` | Embedded (console or game) |
| `vr` | VR |

Client properties are fetched from the same client-info endpoint the previous
engine used, with a static Chrome/Windows fallback if it is unavailable. Mobile,
embedded and VR values override the `browser` entry of the gateway IDENTIFY
payload, which is what Discord turns into a session's client type.

The header of the app shows one status-coloured icon per platform the account is
actually online on, merged from Discord's own session list.

## Obtaining your token

1. Log in to your Discord account
2. Press Ctrl+Shift+I to open DevTools
3. Go to the Network tab, refresh the page
4. Filter by `/api`, click the `science` entry
5. Under Headers, find `Authorization` — that is your token

Or use the app's auto-detect (Windows).

## Layout

```
src-tauri/src/engine.rs                    native Discord gateway: identify, heartbeat,
                                           presence, sessions, config watching
src-tauri/src/bin/discord-toolbox-engine.rs headless engine binary
src-tauri/src/bot.rs                       engine lifecycle and Tauri commands
src-tauri/src/badges.rs                    game badge forger
src-tauri/src/hypesquad.rs                 HypeSquad house manager
src-tauri/src/token_extract.rs             token discovery and profiles
src-tauri/src/updater.rs                   GitHub release updater
src-tauri/src/main.rs                      Tauri entry point and window commands
ui/src/app.rs                              Leptos application, all five pages
ui/src/api.rs                              Tauri IPC bindings
ui/src/icons.rs                            inline SVG icons
ui/app.css                                 dark theme styling
```

## License

MIT
