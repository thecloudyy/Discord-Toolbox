# Discord Toolbox

Keeps your Discord account in a voice channel 24/7, online with optional custom status/activity.
Built on discord.py-self. Automating a user account is against Discord's ToS: use at your own risk.

## Features

- Keeps your account in a target voice channel 24/7 with auto-rejoin
- Randomized rejoin cooldowns, zero API polling
- Optional custom status and activity (playing/listening/watching/competing)
- Status modes: online, idle, dnd, invisible
- Platform spoofer: Windows, macOS, Linux, web, Android, iOS, Xbox, PlayStation or VR
- Live platform indicators: one status-coloured icon per platform your account is online on, read from Discord's own session list
- Self-deafen / self-mute support
- Desktop app (Tauri + React) with account auto-detection and live log
- Live status, log lines and platform indicators pushed from the bot to the app, no polling in the UI
- Most settings apply to the running bot immediately; token, `PLATFORM` and self-deafen need a restart
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
   environment, installs the engine, and prompts for your Discord token / guild / channel on
   first run (saved to `.env`).

2. Install the desktop app dependencies and start it:

   ```
   npm install
   npm run tauri:dev
   ```

   Build installers with `npm run tauri:build` (output in `src-tauri/target/release/bundle`).

The app detects your Discord accounts automatically and stores the chosen token in `.env`.
It spawns the Python engine as a child process, streams its log, and reads the live state it
publishes (`.vc-keeper.state.json`) to draw the platform indicators.

## Headless / server

```
.venv/bin/vc-keeper            # Linux/macOS
.venv\Scripts\vc-keeper.exe    # Windows
```

See `deploy/vc-keeper.service` for a systemd unit pointing at the repo's virtualenv.

## Configuration (.env)

```
DISCORD_TOKEN=
DISCORD_GUILD_ID=
DISCORD_CHANNEL_ID=
LOG_LEVEL=INFO
SELF_DEAF=false
STATUS=online
ACTIVITY_TYPE=
ACTIVITY_TEXT=
CUSTOM_STATUS=
PLATFORM=windows
REJOIN_COOLDOWN_MIN=10
REJOIN_COOLDOWN_MAX=120
```

Status, activity, custom status and the guild/channel target are picked up by the running bot
within about a second, no restart needed. Token, `PLATFORM` and self-deafen are only read at
login, so those need a restart (the app's **Restart** button). The UI says so in the log when
that happens.

## Platforms

`PLATFORM` picks what Discord thinks you are running:

| Value | Shows as |
| --- | --- |
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
src/vc_keeper/app.py            the bot: config, gateway identity, voice keeper, live state
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
deploy/vc-keeper.service        systemd unit for source installs on Linux
```

## License

MIT
