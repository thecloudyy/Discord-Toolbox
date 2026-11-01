from __future__ import annotations

import asyncio
import json
import logging
import os
import random
import re
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

HEALTH_CHECK_EVERY_S = 60.0
LIBRARY_RECONNECT_GRACE_S = 30.0
MAX_BACKOFF_S = 60.0
STATE_PUBLISH_S = 5.0
CONFIG_POLL_S = 1.0
SNOWFLAKE = re.compile(r"^\d{16,22}$")

STATE_FILE = app_dir() / ".vc-keeper.state.json"

# Which platform to pretend to be. Desktop variants are spoofed with a full
# desktop client identity; the rest are spoofed the same way Equicord's
# PlatformSpoofer does it: by overriding the `browser` field of the gateway
# IDENTIFY properties, which is what Discord reads to fill in a session's
# client/os.
PLATFORM_CHOICES = ("windows", "mac", "linux", "web", "android", "ios", "xbox", "playstation", "vr")
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


def _read_id(env: Mapping[str, str], name: str, problems: list[str]) -> int:
    raw = env.get(name, "").strip()
    if not raw:
        return 0
    if not SNOWFLAKE.fullmatch(raw):
        problems.append(f"{name} must be a Discord ID (16-22 digits)")
        return 0
    return int(raw)


def _read_seconds(env: Mapping[str, str], name: str, default: float, problems: list[str]) -> float:
    raw = env.get(name, "").strip()
    if not raw:
        return default
    try:
        value = float(raw)
    except ValueError:
        problems.append(f"{name} must be a number of seconds")
        return default
    if value < 0:
        problems.append(f"{name} can't be negative")
    return value


@dataclass(frozen=True, slots=True)
class Config:
    token: str
    guild_id: int
    channel_id: int
    self_deaf: bool
    cooldown_min_s: float
    cooldown_max_s: float
    log_level: str
    status: str = "online"
    activity_type: str = ""
    activity_text: str = ""
    custom_status: str = ""
    platform: str = "windows"

    @classmethod
    def from_env(cls) -> Config:
        return cls.from_mapping(os.environ)

    @classmethod
    def from_mapping(cls, env: Mapping[str, str]) -> Config:
        problems: list[str] = []

        token = env.get("DISCORD_TOKEN", "").strip()
        if not token:
            problems.append("DISCORD_TOKEN is missing")

        guild_id = _read_id(env, "DISCORD_GUILD_ID", problems)
        channel_id = _read_id(env, "DISCORD_CHANNEL_ID", problems)
        if channel_id and not guild_id:
            problems.append("DISCORD_GUILD_ID is required when a voice channel is set")
        cooldown_min = _read_seconds(env, "REJOIN_COOLDOWN_MIN", 10, problems)
        cooldown_max = _read_seconds(env, "REJOIN_COOLDOWN_MAX", 120, problems)
        if cooldown_max < cooldown_min:
            problems.append("REJOIN_COOLDOWN_MAX must be >= REJOIN_COOLDOWN_MIN")

        self_deaf = env.get("SELF_DEAF", "false").strip().lower() in {"1", "true", "yes", "on"}
        log_level = env.get("LOG_LEVEL", "INFO").strip().upper()
        if log_level not in {"DEBUG", "INFO", "WARNING", "ERROR"}:
            problems.append("LOG_LEVEL must be DEBUG, INFO, WARNING or ERROR")

        status = env.get("STATUS", "online").strip().lower() or "online"
        if status not in {"online", "idle", "dnd", "invisible"}:
            problems.append("STATUS must be online, idle, dnd or invisible")

        activity_type = env.get("ACTIVITY_TYPE", "").strip().lower()
        if activity_type and activity_type not in {"playing", "listening", "watching", "competing", "custom"}:
            problems.append("ACTIVITY_TYPE must be playing, listening, watching, competing or custom")

        activity_text = env.get("ACTIVITY_TEXT", "").strip()
        custom_status = env.get("CUSTOM_STATUS", "").strip()

        platform = env.get("PLATFORM", "windows").strip().lower() or "windows"
        platform = PLATFORM_ALIASES.get(platform, platform)
        if platform not in PLATFORM_CHOICES:
            problems.append("PLATFORM must be one of " + ", ".join(PLATFORM_CHOICES))

        if problems:
            raise ConfigError("; ".join(problems))

        return cls(
            token, guild_id, channel_id, self_deaf, cooldown_min, cooldown_max, log_level,
            status, activity_type, activity_text, custom_status, platform,
        )

    @property
    def voice_enabled(self) -> bool:
        return bool(self.guild_id and self.channel_id)

    def presence(self) -> tuple[str, str, str, str]:
        """The parts of the config that only affect presence, for change detection."""
        return (self.status, self.activity_type, self.activity_text, self.custom_status)


# --------------------------------------------------------------------------- client


class VcKeeper(discord.Client):
    def __init__(self, config: Config) -> None:
        super().__init__(
            afk=True,
            sync_presence=False,
            max_messages=None,
            guild_subscriptions=False,
            member_cache_flags=discord.MemberCacheFlags(joined=False),
        )
        self.config = config
        self.started_at = time.time()
        self._wake = asyncio.Event()
        self._kicked = False
        self._supervisor: asyncio.Task[None] | None = None
        self._state: asyncio.Task[None] | None = None
        self._config_task: asyncio.Task[None] | None = None
        self._shutdown: asyncio.Task[None] | None = None

    # ---- lifecycle --------------------------------------------------------

    async def setup_hook(self) -> None:
        self._supervisor = asyncio.create_task(self._supervise(), name="vc-keeper-supervisor")
        self._supervisor.add_done_callback(self._on_supervisor_done)
        self._state = asyncio.create_task(self._publish_loop(), name="vc-keeper-state")
        self._config_task = asyncio.create_task(self._watch_config(), name="vc-keeper-config")

    def _on_supervisor_done(self, task: asyncio.Task[None]) -> None:
        if task.cancelled() or self.is_closed():
            return
        log.critical(
            "supervisor died unexpectedly, shutting down so systemd restarts us", exc_info=task.exception()
        )
        self.request_shutdown()

    def request_shutdown(self) -> None:
        if self._shutdown is None:
            self._shutdown = asyncio.get_running_loop().create_task(self.close())

    async def on_ready(self) -> None:
        assert self.user is not None
        log.info("logged in as %s (%s)", self.user, self.user.id)
        if not self.config.voice_enabled:
            log.info("no voice channel set, staying online only")
        await self._update_presence()
        self._wake.set()
        self.publish_state()

    async def on_resumed(self) -> None:
        log.info("gateway session resumed")
        await self._update_presence()
        self._wake.set()
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
        configured = (self.config.platform or "windows").lower()
        platform = PLATFORM_ALIASES.get(configured, configured)

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
        status = status_map.get(self.config.status, discord.Status.online)

        config = self.config
        activity = None
        if config.activity_type and config.activity_text:
            factory = ACTIVITY_FACTORIES.get(config.activity_type)
            if factory is not None:
                activity = factory(config.activity_text)
        if activity is None and config.custom_status:
            activity = discord.CustomActivity(name=config.custom_status)

        try:
            await self.change_presence(status=status, activity=activity)
        except Exception as exc:
            log.warning("could not update presence: %s", exc)

    async def on_voice_state_update(
        self, member: discord.Member | discord.User, before: discord.VoiceState, after: discord.VoiceState
    ) -> None:
        if not self.config.voice_enabled or self.user is None or member.id != self.user.id:
            return
        guild = getattr(after.channel or before.channel, "guild", None)
        if guild is not None and guild.id != self.config.guild_id:
            return

        now = after.channel.id if after.channel else None
        if now == self.config.channel_id:
            log.debug("voice state: in target channel")
            return

        where = f"moved to {now}" if now else "disconnected"
        log.warning("voice state changed: %s", where)
        self._kicked = True
        self._wake.set()
        self.publish_state()

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
        old_vc = self._voice_client()
        self.config = config

        if config.presence() != old.presence():
            detail = config.status
            if config.activity_type and config.activity_text:
                detail += f" ({config.activity_type} {config.activity_text})"
            elif config.custom_status:
                detail += f" ({config.custom_status})"
            if self.is_ready():
                log.info("presence updated: %s", detail)
                await self._update_presence()
            else:
                log.info("presence %s saved, applies on login", detail)

        if (config.guild_id, config.channel_id) != (old.guild_id, old.channel_id):
            if config.voice_enabled:
                log.info("target is now channel %s in guild %s", config.channel_id, config.guild_id)
            else:
                log.info("voice channel cleared, staying online only")
            if old_vc is not None:
                try:
                    await old_vc.disconnect(force=True)
                except (discord.ClientException, OSError):
                    log.debug("could not leave the old channel", exc_info=True)
            self._wake.set()

        needs_restart = [
            name
            for name, changed in (
                ("token", config.token != old.token),
                ("platform", config.platform != old.platform),
                ("self-deafen", config.self_deaf != old.self_deaf),
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

    # ---- the one loop that decides everything -------------------------------

    async def _supervise(self) -> None:
        await self.wait_until_ready()
        failures = 0

        while not self.is_closed():
            try:
                await asyncio.wait_for(self._wake.wait(), timeout=HEALTH_CHECK_EVERY_S)
            except TimeoutError:
                pass
            self._wake.clear()

            if not self.config.voice_enabled:
                self._kicked = False
                continue

            if self._in_target():
                failures = 0
                self._kicked = False
                continue

            if self._kicked:
                self._kicked = False
                delay = random.uniform(self.config.cooldown_min_s, self.config.cooldown_max_s)
                log.info("rejoining in %.0fs (cooldown)", delay)
                await asyncio.sleep(delay)
                if self._in_target():
                    continue

            try:
                await self._ensure_joined()
                failures = 0
                log.info("in voice channel %s", self.config.channel_id)
            except (discord.ClientException, discord.HTTPException, TimeoutError, OSError) as exc:
                failures += 1
                delay = min(2.0**failures, MAX_BACKOFF_S) * random.uniform(0.8, 1.2)
                log.error(
                    "join attempt %d failed (%s: %s), retrying in %.0fs",
                    failures,
                    type(exc).__name__,
                    exc,
                    delay,
                )
                await asyncio.sleep(delay)
                self._wake.set()

    # ---- helpers --------------------------------------------------------------

    def _voice_client(self) -> discord.VoiceClient | None:
        guild = self.get_guild(self.config.guild_id)
        vc = guild.voice_client if guild else None
        return vc if isinstance(vc, discord.VoiceClient) else None

    def _in_target(self) -> bool:
        vc = self._voice_client()
        return (
            vc is not None and vc.is_connected() and getattr(vc.channel, "id", None) == self.config.channel_id
        )

    def _target_channel(self) -> discord.VoiceChannel | discord.StageChannel:
        guild = self.get_guild(self.config.guild_id)
        if guild is None:
            raise discord.ClientException(f"not in guild {self.config.guild_id} (check DISCORD_GUILD_ID)")
        channel = guild.get_channel(self.config.channel_id)
        if not isinstance(channel, discord.VoiceChannel | discord.StageChannel):
            raise discord.ClientException(f"{self.config.channel_id} isn't a voice/stage channel you can see")
        return channel

    async def _ensure_joined(self) -> None:
        channel = self._target_channel()
        vc = self._voice_client()

        if vc is not None:
            if getattr(vc.channel, "id", None) == channel.id:
                log.debug("voice client reconnecting on its own, giving it %.0fs", LIBRARY_RECONNECT_GRACE_S)
                loop = asyncio.get_running_loop()
                deadline = loop.time() + LIBRARY_RECONNECT_GRACE_S
                while loop.time() < deadline:
                    await asyncio.sleep(1)
                    if self._in_target():
                        return
                log.warning("library reconnect didn't finish, starting fresh")
                await vc.disconnect(force=True)
            elif vc.is_connected():
                log.info("moving back to target channel")
                await vc.move_to(channel)
                return
            else:
                await vc.disconnect(force=True)

        await channel.connect(self_mute=True, self_deaf=self.config.self_deaf, reconnect=True, timeout=30.0)


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
    if config.voice_enabled:
        log.info("starting discord-toolbox (voice %s in guild %s)", config.channel_id, config.guild_id)
    else:
        log.info("starting discord-toolbox (no voice channel, online only)")

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
