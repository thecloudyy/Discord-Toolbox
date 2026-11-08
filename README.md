# Discord Toolbox

A small desktop companion for Discord game badge forging and HypeSquad
management. The app is Tauri with a Leptos/WASM UI, all written in Rust. There is
no Python runtime, no Node build step and no background gateway client.
Automating a user account is against Discord's ToS: use it at your own risk.

## Features

- Token auto-detection from local Discord profiles (Windows)
- Multi-account selection
- Game badge forger: credits game time and game variety through Discord's own telemetry
- Single forge pass by default, with an optional loop that keeps forging until you press Stop
- Live batch progress and a forge log inside the app
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
per user and contains only the app executable.

## Configuration

The chosen token is stored in `.env` in the project folder, or in
`%LOCALAPPDATA%\Discord Toolbox` for an installed build:

```
DISCORD_TOKEN=
# optional: point the updater at a fork
UPDATE_REPO=thecloudyy/Discord-Toolbox
```

## Obtaining your token

1. Log in to your Discord account
2. Press Ctrl+Shift+I to open DevTools
3. Go to the Network tab, refresh the page
4. Filter by `/api`, click the `science` entry
5. Under Headers, find `Authorization` — that is your token

Or use the app's auto-detect (Windows).

## Layout

```
src-tauri/src/config.rs                    .env handling, token detection, profiles
src-tauri/src/badges.rs                    game badge forger
src-tauri/src/hypesquad.rs                 HypeSquad house manager
src-tauri/src/token_extract.rs             token discovery and profiles
src-tauri/src/updater.rs                   GitHub release updater
src-tauri/src/main.rs                      Tauri entry point and window commands
ui/src/app.rs                              Leptos application, all pages
ui/src/api.rs                              Tauri IPC bindings
ui/src/icons.rs                            inline SVG icons
ui/app.css                                 dark theme styling
```

## License

MIT
