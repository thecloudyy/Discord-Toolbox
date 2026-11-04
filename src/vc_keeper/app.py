from __future__ import annotations

import asyncio
import json
import logging
import os
import signal
import sys
import time
from collections.abc import Callable, Mapping
from typing import Any
from dataclasses import dataclass
from pathlib import Path

if getattr(sys, "frozen", False):
    # The onefile bundle unpacks curl_cffi's DLLs next to itself.
    _base = getattr(sys, "_MEIPASS", "")
    for _dir in (_base, os.path.join(_base, "curl_cffi.libs")):
        if _dir and os.path.isdir(_dir):
            try:
                os.add_dll_directory(_dir)
            except (AttributeError, OSError):
                pass
            os.environ["PATH"] = _dir + os.pathsep + os.environ.get("PATH", "")


import discord

from .envfile import app_dir, env_path, read as read_env


def _mtime(path: Path | None) -> int | None:
    if path is None:
        return None
    try:
        return path.stat().st_mtime_ns
    except OSError:
        return None


def _load_dotenv() -> None:
    """Fill os.environ from .env, without overriding variables already set."""
    path = env_path()
    if path is None:
        return
    for key, value in read_env(path).items():
        os.environ.setdefault(key, value)

log = logging.getLogger("discord-toolbox")

EXIT_OK = 0
EXIT_CRASH = 1
EXIT_CONFIG = 78

MAX_BACKOFF_S = 60.0
STATE_PUBLISH_S = 5.0
CONFIG_POLL_S = 1.0

STATE_FILE = app_dir() / ".vc-keeper.state.json"

# Which platform to pretend to be. Desktop variants are spoofed with a full
# desktop client identity; the rest are spoofed the same way Equicord's
# PlatformSpoofer does it: by overriding the `browser` field of the gateway
# IDENTIFY properties, which is what Discord reads to fill in a session's
# client/os.
PLATFORM_CHOICES = (
    "off", "windows", "mac", "linux", "web", "android", "ios", "xbox", "playstation", "vr",
)
PLATFORM_ALIASES = {"desktop": "windows", "macos": "mac", "mobile": "ios"}
SPOOFED_PROPERTIES: dict[str, dict[str, str]] = {
    "android": {"browser": "Discord Android", "os": "Android"},
    "ios": {"browser": "Discord iOS", "os": "iOS"},
    "xbox": {"browser": "Discord Embedded"},
    "playstation": {"browser": "Discord Embedded"},
    "vr": {"browser": "Discord VR"},
}

# Desktop flavours fetched from Discord's client-info API, and the platform name
# each one reports.
ELECTRON_PLATFORMS = {"mac": "macOS", "linux": "Linux"}

ACTIVITY_FACTORIES: dict[str, Callable[[str], discord.BaseActivity]] = {
    "playing": lambda text: discord.Game(name=text),
    "listening": lambda text: discord.Activity(type=discord.ActivityType.listening, name=text),
    "watching": lambda text: discord.Activity(type=discord.ActivityType.watching, name=text),
    "competing": lambda text: discord.Activity(type=discord.ActivityType.competing, name=text),
    "custom": lambda text: discord.CustomActivity(name=text),
}


async def _electron_context(session: Any, flavour: str) -> discord.HeadersContext:
    """A full Electron desktop-client identity for macOS or Linux."""
    data = await discord.HeadersContext.fetch_api_properties(session, flavour)
    major = int(data["metadata"]["native_chrome_version"].split(".")[0])
    return discord.HeadersContext(
        platform=ELECTRON_PLATFORMS[flavour],
        browser_type="electron",
        browser_major_version=major,
        super_properties=data["properties"],
        encoded_super_properties=data.get("encoded"),
        extra_gateway_properties=data.get("extra_gateway_properties"),
    )


# --------------------------------------------------------------------------- config


class ConfigError(Exception):
    pass


@dataclass(frozen=True, slots=True)
class Config:
    token: str
    log_level: str
    status: str = ""
    activity_type: str = ""
    activity_text: str = ""
    custom_status: str = ""
    platform: str = "off"

    @classmethod
    def from_env(cls) -> Config:
        return cls.from_mapping(os.environ)

    @classmethod
    def from_mapping(cls, env: Mapping[str, str]) -> Config:
        problems: list[str] = []

        token = env.get("DISCORD_TOKEN", "").strip()
        if not token:
            problems.append("DISCORD_TOKEN is missing")

        log_level = env.get("LOG_LEVEL", "INFO").strip().upper()
        if log_level not in {"DEBUG", "INFO", "WARNING", "ERROR"}:
            problems.append("LOG_LEVEL must be DEBUG, INFO, WARNING or ERROR")

        status = env.get("STATUS", "").strip().lower()
        if status and status not in {"online", "idle", "dnd", "invisible"}:
            problems.append("STATUS must be empty, online, idle, dnd or invisible")

        activity_type = env.get("ACTIVITY_TYPE", "").strip().lower()
        if activity_type and activity_type not in {"playing", "listening", "watching", "competing", "custom"}:
            problems.append("ACTIVITY_TYPE must be playing, listening, watching, competing or custom")

        activity_text = env.get("ACTIVITY_TEXT", "").strip()
        custom_status = env.get("CUSTOM_STATUS", "").strip()

        platform = env.get("PLATFORM", "off").strip().lower() or "off"
        platform = PLATFORM_ALIASES.get(platform, platform)
        if platform not in PLATFORM_CHOICES:
            problems.append("PLATFORM must be one of " + ", ".join(PLATFORM_CHOICES))

        if problems:
            raise ConfigError("; ".join(problems))

        return cls(
            token, log_level, status, activity_type, activity_text, custom_status, platform,
        )

    def presence(self) -> tuple[str, str, str, str]:
        """The parts of the config that only affect presence, for change detection."""
        return (self.status, self.activity_type, self.activity_text, self.custom_status)


# --------------------------------------------------------------------------- client


class VcKeeper(discord.Client):
    def __init__(self, config: Config) -> None:
        super().__init__(
            sync_presence=False,
            max_messages=None,
            guild_subscriptions=False,
            member_cache_flags=discord.MemberCacheFlags(joined=False),
        )
        self.config = config
        self.started_at = time.time()
        self._state: asyncio.Task[None] | None = None
        self._config_task: asyncio.Task[None] | None = None
        self._shutdown: asyncio.Task[None] | None = None

    # ---- lifecycle --------------------------------------------------------

    async def setup_hook(self) -> None:
        self._state = asyncio.create_task(self._publish_loop(), name="toolbox-state")
        self._config_task = asyncio.create_task(self._watch_config(), name="toolbox-config")

    def request_shutdown(self) -> None:
        if self._shutdown is None:
            self._shutdown = asyncio.get_running_loop().create_task(self.close())

    async def on_ready(self) -> None:
        assert self.user is not None
        log.info("logged in as %s (%s)", self.user, self.user.id)
        await self._update_presence()
        self.publish_state()

    async def on_resumed(self) -> None:
        log.info("gateway session resumed")
        await self._update_presence()
        self.publish_state()

    async def on_session_create(self, session: discord.Session) -> None:
        self.publish_state()

    async def on_session_update(self, before: discord.Session, after: discord.Session) -> None:
        self.publish_state()

    async def on_session_delete(self, session: discord.Session) -> None:
        self.publish_state()

    async def headers_context(self) -> discord.HeadersContext:
        http = self.http
        session = http._HTTPClient__asession  # type: ignore
        configured = (self.config.platform or "off").lower()
        platform = PLATFORM_ALIASES.get(configured, configured)

        if platform == "off":
            log.debug("platform spoofing is off, sending the default identity")
            return await discord.HeadersContext.default(session, http.proxy, http.proxy_auth)

        try:
            if platform == "windows":
                return await discord.HeadersContext.desktop(session)
            if platform in ELECTRON_PLATFORMS:
                return await _electron_context(session, platform)
            context = await discord.HeadersContext.default(session, http.proxy, http.proxy_auth)
        except Exception as exc:
            log.warning("platform spoof failed, falling back to default: %s", exc)
            return await discord.HeadersContext.default(session, http.proxy, http.proxy_auth)

        overrides = SPOOFED_PROPERTIES.get(platform)
        if overrides:
            # Same trick as Equicord's PlatformSpoofer: the `browser` entry of the
            # gateway IDENTIFY properties is what Discord turns into a session's
            # client type, and `os` is what it reports as the session's OS.
            context.extra_gateway_properties = {**context.extra_gateway_properties, **overrides}
        log.info("spoofing platform %s%s", platform, f" (identify {overrides})" if overrides else "")
        return context

    async def _update_presence(self) -> None:
        status_map = {
            "online": discord.Status.online,
            "idle": discord.Status.idle,
            "dnd": discord.Status.dnd,
            "invisible": discord.Status.invisible,
        }
        status = status_map.get(self.config.status)

        config = self.config
        activity: discord.BaseActivity | None = None
        if config.activity_type and config.activity_text:
            factory = ACTIVITY_FACTORIES.get(config.activity_type)
            if factory is not None:
                activity = factory(config.activity_text)
        if activity is None and config.custom_status:
            activity = discord.CustomActivity(name=config.custom_status)

        # Nothing configured means nothing to override: leave the presence the
        # official client set alone instead of forcing a status onto the account.
        if status is None and activity is None:
            log.info("no status or activity configured, leaving presence untouched")
            return

        presence: dict[str, Any] = {}
        if status is not None:
            presence["status"] = status
        if activity is not None:
            presence["activity"] = activity

        try:
            await self.change_presence(**presence)
        except Exception as exc:
            log.warning("could not update presence: %s", exc)

    # ---- live config ---------------------------------------------------------

    async def _watch_config(self) -> None:
        """Apply .env edits to the running bot, without a restart."""
        path = env_path()
        last = _mtime(path)
        while not self.is_closed():
            await asyncio.sleep(CONFIG_POLL_S)
            if path is None:
                path = env_path()
                if path is not None:
                    last = _mtime(path)
                continue
            current = _mtime(path)
            if current is None or current == last:
                continue
            last = current
            try:
                config = Config.from_mapping(read_env(path))
            except ConfigError as exc:
                log.warning("config change ignored: %s", exc)
                continue
            try:
                await self._apply_config(config)
            except Exception:
                log.exception("could not apply config change")

    async def _apply_config(self, config: Config) -> None:
        old = self.config
        self.config = config

        if config.presence() != old.presence():
            detail = config.status or "keep current"
            if config.activity_type and config.activity_text:
                detail += f" ({config.activity_type} {config.activity_text})"
            elif config.custom_status:
                detail += f" ({config.custom_status})"
            if self.is_ready():
                log.info("presence updated: %s", detail)
                await self._update_presence()
            else:
                log.info("presence %s saved, applies on login", detail)

        needs_restart = [
            name
            for name, changed in (
                ("token", config.token != old.token),
                ("platform", config.platform != old.platform),
            )
            if changed
        ]
        if needs_restart:
            log.info("%s changed, restart the bot to apply it", ", ".join(needs_restart))

        self.publish_state()

    # ---- live state for the UI ----------------------------------------------

    def _session_rows(self) -> list[dict[str, object]]:
        """Every session Discord reports for this account (the platforms it's online on)."""
        rows: list[dict[str, object]] = []
        for session in self.sessions:
            if getattr(session, "session_id", None) == "all":
                continue  # synthetic aggregate session, not a platform
            rows.append(
                {
                    "client": str(getattr(session, "client", "unknown")),
                    "os": getattr(getattr(session, "os", None), "name", "unknown"),
                    "status": getattr(getattr(session, "status", None), "name", "offline"),
                    "active": bool(getattr(session, "active", False)),
                }
            )
        return rows

    def publish_state(self) -> None:
        user = None
        if self.user is not None:
            avatar = getattr(self.user, "display_avatar", None)
            user = {
                "id": str(self.user.id),
                "username": self.user.name,
                "global_name": getattr(self.user, "global_name", None),
                "avatar_url": getattr(avatar, "url", None),
            }
        payload = {
            "pid": os.getpid(),
            # PyInstaller onefile relaunches itself, so the process the app
            # spawned is this process's parent.
            "launcher_pid": os.getppid(),
            "started_at": self.started_at,
            "updated_at": time.time(),
            "user": user,
            "spoofed_platform": self.config.platform,
            "sessions": self._session_rows(),
        }
        tmp = STATE_FILE.with_name(STATE_FILE.name + ".tmp")
        try:
            tmp.write_text(json.dumps(payload), encoding="utf-8")
            os.replace(tmp, STATE_FILE)
        except OSError as exc:
            log.debug("could not publish state: %s", exc)

    async def _publish_loop(self) -> None:
        while not self.is_closed():
            self.publish_state()
            await asyncio.sleep(STATE_PUBLISH_S)


# --------------------------------------------------------------------------- entrypoint


def _setup_logging(level: str) -> None:
    fmt = "%(levelname)-7s %(name)s: %(message)s"
    if "INVOCATION_ID" not in os.environ:
        fmt = "%(asctime)s " + fmt
    logging.basicConfig(level=level, format=fmt, stream=sys.stdout)
    logging.getLogger("discord").setLevel(level if level == "DEBUG" else "WARNING")


async def _run(config: Config) -> None:
    async with VcKeeper(config) as client:
        loop = asyncio.get_running_loop()

        def shutdown(sig: signal.Signals) -> None:
            log.info("got %s, shutting down and logging out", sig.name)
            client.request_shutdown()

        for sig in (signal.SIGINT, signal.SIGTERM):
            try:
                loop.add_signal_handler(sig, shutdown, sig)
            except NotImplementedError:
                pass
        await client.start(config.token)


def main() -> None:
    _load_dotenv()
    try:
        config = Config.from_env()
    except ConfigError as exc:
        logging.basicConfig(format="%(levelname)s %(message)s")
        log.error("config error: %s", exc)
        sys.exit(EXIT_CONFIG)

    _setup_logging(config.log_level)
    log.info("starting discord-toolbox")

    backoff = 5.0
    while True:
        try:
            asyncio.run(_run(config))
            log.info("stopped")
            sys.exit(EXIT_OK)
        except KeyboardInterrupt:
            log.info("stopped")
            sys.exit(EXIT_OK)
        except discord.LoginFailure:
            log.error("discord rejected the token. not restarting (exit 78). get a fresh token and restart.")
            sys.exit(EXIT_CONFIG)
        except discord.ConnectionClosed as exc:
            if exc.code == 4004:
                log.error("gateway says the token is invalid (4004). not restarting (exit 78).")
                sys.exit(EXIT_CONFIG)
            log.error("gateway closed (code %s), restarting in %.0fs", exc.code, backoff)
        except SystemExit:
            raise
        except Exception:
            log.exception("crashed, restarting in %.0fs", backoff)

        time.sleep(backoff)
        backoff = min(backoff * 2, MAX_BACKOFF_S)


if __name__ == "__main__":
    main()
