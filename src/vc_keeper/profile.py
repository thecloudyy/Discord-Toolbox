"""Fetch a Discord account's profile for the UI (token stays local)."""

from __future__ import annotations

import json
import urllib.error
import urllib.request

API_URL = "https://discord.com/api/v9/users/@me"


def fetch_profile(token: str, size: int = 128) -> dict[str, object]:
    """Return id, username, global name and avatar URL for a token."""
    token = token.strip()
    if not token:
        raise ValueError("token is empty")

    request = urllib.request.Request(
        API_URL,
        headers={"Authorization": token, "User-Agent": "discord-toolbox"},
    )
    try:
        with urllib.request.urlopen(request, timeout=10) as response:
            user = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        if exc.code in (401, 403):
            raise ValueError("discord rejected the token") from exc
        raise RuntimeError(f"discord returned HTTP {exc.code}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"could not reach discord: {exc.reason}") from exc

    avatar = user.get("avatar")
    if avatar:
        avatar_url = f"https://cdn.discordapp.com/avatars/{user['id']}/{avatar}.png?size={size}"
    else:
        avatar_url = f"https://cdn.discordapp.com/embed/avatars/{(int(user['id']) >> 22) % 6}.png"

    return {
        "id": str(user.get("id", "")),
        "username": user.get("username"),
        "global_name": user.get("global_name"),
        "avatar_url": avatar_url,
    }
