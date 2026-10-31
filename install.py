#!/usr/bin/env python3
"""One-command setup for Discord Toolbox (no shell scripts required)."""

import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
VENV = ROOT / ".venv"
ENV_FILE = ROOT / ".env"

# pip/httpx choke on IPv6 CIDR entries in NO_PROXY, so sanitize it for the
# install subprocesses.
INSTALL_ENV = os.environ.copy()
INSTALL_ENV["NO_PROXY"] = "localhost,127.0.0.1,.local"
INSTALL_ENV["no_proxy"] = INSTALL_ENV["NO_PROXY"]


def run(*args: str) -> None:
    print("+", " ".join(args))
    subprocess.run(list(args), check=True, env=INSTALL_ENV, cwd=ROOT)


def venv_python() -> Path:
    if os.name == "nt":
        return VENV / "Scripts" / "python.exe"
    return VENV / "bin" / "python"


def main() -> int:
    if not VENV.exists():
        print("[discord-toolbox] creating virtual environment...")
        run(sys.executable, "-m", "venv", str(VENV))

    py = venv_python()
    print("[discord-toolbox] upgrading pip...")
    run(str(py), "-m", "pip", "install", "--upgrade", "pip")

    print("[discord-toolbox] installing dependencies (this may take a few minutes)...")
    run(str(py), "-m", "pip", "install", "-e", ".")

    needs_setup = not ENV_FILE.exists()
    if ENV_FILE.exists():
        try:
            content = ENV_FILE.read_text(encoding="utf-8")
            if "DISCORD_TOKEN=" not in content:
                needs_setup = True
            elif "DISCORD_TOKEN=\n" in content or "DISCORD_TOKEN=\r\n" in content:
                # empty token -> treat as unconfigured
                needs_setup = True
        except OSError:
            needs_setup = True

    if needs_setup:
        print("[discord-toolbox] first run setup")
        try:
            token = input("discord token: ").strip()
            guild = input("server (guild) id (optional): ").strip()
            channel = input("voice channel id (optional, leave empty to stay online only): ").strip()
        except EOFError:
            token = guild = channel = ""
        lines = [
            f"DISCORD_TOKEN={token}",
            f"DISCORD_GUILD_ID={guild}",
            f"DISCORD_CHANNEL_ID={channel}",
            "LOG_LEVEL=INFO",
            "SELF_DEAF=false",
            "REJOIN_COOLDOWN_MIN=10",
            "REJOIN_COOLDOWN_MAX=120",
        ]
        ENV_FILE.write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"[discord-toolbox] wrote {ENV_FILE}")
    else:
        print(f"[discord-toolbox] keeping existing {ENV_FILE}")

    print()
    print("[discord-toolbox] install complete.")
    print("desktop app: npm install, then npm run tauri:dev")
    if os.name == "nt":
        print("headless bot: .\\.venv\\Scripts\\discord-toolbox.exe")
    else:
        print("headless bot: ./.venv/bin/discord-toolbox")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as exc:
        print(f"[discord-toolbox] command failed with exit code {exc.returncode}", file=sys.stderr)
        raise SystemExit(exc.returncode)
    except KeyboardInterrupt:
        print("\n[discord-toolbox] cancelled", file=sys.stderr)
        raise SystemExit(130)
