# Discord Toolbox

Keeps your Discord account online, with optional custom status/activity. Built on
discord.py-self. Automating a user account is against Discord's ToS: use it at your own risk.

## Features

- Keeps your account online 24/7, zero API polling
- Everything optional is off by default: you pick a status, an activity or a platform
- Optional custom status and activity (playing/listening/watching/competing)
- Status modes: online, idle, dnd, invisible
- Optional platform spoofer: Windows, macOS, Linux, web, Android, iOS, Xbox, PlayStation or VR
- Live platform indicators: one status-coloured icon per platform your account is online on, read from Discord's own session list
- Desktop app (Tauri + React) with account auto-detection and live log
- Live status, log lines and platform indicators pushed from the bot to the app, no polling in the UI
- Most settings apply to the running bot immediately; token and `PLATFORM` need a restart
- The app never starts the bot on its own: press **Start** to connect
- Token auto-detection from local Discord profiles (Windows)
- Multi-account selection
- Game badge forger: credits game time and game variety through Discord's own telemetry
- HypeSquad manager: switch house, attempt removal, and view profile badges
- Built-in updater fed by GitHub releases
- Exit codes: 0 stopped, 1 crashed (auto-restarts), 78 bad config/token

## Disclaimer

By using this code, you are automating your Discord account. This is against Discord's Terms of Service and Community Guidelines. Your account might get suspended or terminated. The developer is not responsible for any consequences. Use at your own risk.

**DO NOT GIVE YOUR DISCORD TOKEN TO ANYONE.**

## Requirements

- Python 3.12+
- Node.js 18+ and npm (desktop app)
- Rust toolchain and the Tauri prerequisites (only needed to build the desktop app)

## Installation

1. Install the Python engine:

   ```
   python install.py
   ```

   On Windows you can also double-click `install.bat`. The installer creates a virtual
   environment, installs the engine, and prompts for your Discord token on first run (saved to
   `.env`).

2. Install the desktop app dependencies and start it:

   ```
   npm install
   npm run tauri:dev
   ```

   To build the release app and installer:

   ```
   npm run build
   cd src-tauri
   cargo build --release --features custom-protocol
   iscc installer.iss
   ```

   The installer lands in `src-tauri/Output/Discord-Toolbox-setup.exe`. It installs per user
   and bundles the standalone bot executable from `src-tauri/bin`.

The app detects your Discord accounts automatically and stores the chosen token in `.env`.
It starts the Python engine as a child process when you press **Start**, streams its log, and
reads the live state it publishes (`.vc-keeper.state.json`) to draw the platform indicators.
Closing the window leaves the bot running; reopening the app attaches to it.

## Headless / server

```
.venv/bin/discord-toolbox            # Linux/macOS
.venv\Scripts\discord-toolbox.exe    # Windows
```

The engine reads `.env` from the current directory, or from `%LOCALAPPDATA%\Discord Toolbox`
when the project directory is not writable.

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

Status, activity and custom status are picked up by the running bot within about a second, no
restart needed. Token and `PLATFORM` are only read at login, so those need a restart (the app's
**Restart** button). The UI says so in the log when that happens.

## Platforms

`PLATFORM` picks what Discord thinks you are running. It defaults to `off`, which sends the
identity the bot normally uses:

| Value | Shows as |
| --- | --- |
| `off` | No spoofing (default) |
| `windows`, `mac`, `linux` | Desktop on that OS (full desktop client identity) |
| `web` | Browser |
| `android`, `ios` | Mobile on that OS |
| `xbox`, `playstation` | Embedded (console or game) |
| `vr` | VR |

Desktop values send a matching desktop client identity. The rest override the `browser` entry of
the gateway IDENTIFY payload (the same mechanism Equicord's PlatformSpoofer uses), which is what
Discord turns into a session's client type; Android and iOS also send `os` so the session is
reported as `mobile` on the right OS. Changes apply on the next login; Xbox, PlayStation and VR
can make your client look out of date.

The header of the app shows one status-coloured icon per platform the account is actually online
on, merged from Discord's session list the way Vencord's PlatformIndicators does it.

## Obtaining your token

1. Log in to your Discord account
2. Press Ctrl+Shift+I to open DevTools
3. Go to the Network tab, refresh the page
4. Filter by `/api`, click the `science` entry
5. Under Headers, find `Authorization` — that is your token

Or use the app's auto-detect (Windows).

## Layout

```
src/vc_keeper/app.py            the bot: config, gateway identity, keep-online loop, live state
src/vc_keeper/envfile.py        shared .env reading/writing and app directory
src/vc_keeper/token_detect.py   token discovery from local Discord profiles (Windows)
src/vc_keeper/profile.py        profile lookup for the account picker
src/App.tsx, src/components/    React app (Vite + Tailwind)
src-tauri/src/main.rs           Tauri entry point and window lifecycle
src-tauri/src/bot.rs            Python process control, log streaming, live state bridge
src-tauri/src/badges.rs         game badge forger
src-tauri/src/hypesquad.rs      HypeSquad house manager
src-tauri/src/updater.rs        GitHub release updater
src-tauri/capabilities/         window permissions granted to the webview
install.py, *.bat               source install and start helpers
discord-toolbox-bot.spec        PyInstaller spec for the standalone bot executable
```

## License

MIT
