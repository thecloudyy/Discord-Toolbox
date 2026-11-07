import { useState } from "react";
import type { Profile } from "../types";
import type { PlatformIndicator } from "../lib/platform";
import { Avatar, Button, Card, StatusPill } from "./ui";
import {
  AlertIcon,
  CheckIcon,
  CopyIcon,
  PlayIcon,
  PlatformGlyph,
  RestartIcon,
  StopIcon,
  TerminalIcon,
} from "./icons";
import { cx } from "../lib/utils";

export function Dashboard({
  profile,
  token,
  indicators,
  onlineSummary,
  running,
  live,
  msg,
  onStart,
  onStop,
  onRestart,
  onOpenLogs,
}: {
  profile: Profile;
  token: string;
  indicators: PlatformIndicator[];
  onlineSummary: string;
  running: boolean;
  live: boolean;
  msg: string;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onOpenLogs: () => void;
}) {
  const name = profile.global_name || profile.username || "Discord account";
  const [copied, setCopied] = useState(false);

  async function copyToken() {
    if (!token) return;
    try {
      await navigator.clipboard.writeText(token);
    } catch {
      // Webview clipboard API can be unavailable; fall back to a hidden textarea.
      const area = document.createElement("textarea");
      area.value = token;
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      document.execCommand("copy");
      area.remove();
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 2000);
  }

  return (
    <div className="space-y-5">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Account status and bot controls.
        </p>
      </header>

      <div className="rounded-xl border border-amber-500/30 bg-amber-500/5 p-4">
        <div className="flex flex-wrap items-start gap-3">
          <AlertIcon className="mt-0.5 h-5 w-5 text-amber-300" />
          <div className="min-w-0 flex-1">
            <p className="text-sm font-semibold text-amber-200">
              Warning: your token is the keys to your account
            </p>
            <p className="mt-1 text-xs leading-relaxed text-amber-200/80">
              Anyone with this token can sign in as you, read your messages and change your
              account. Copy it only for your own use — never share it or paste it into another
              site or app.
            </p>
          </div>
          <Button
            variant="warning"
            className="shrink-0"
            onClick={() => void copyToken()}
            disabled={!token}
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
            {copied ? "Copied" : "Copy my Discord token"}
          </Button>
        </div>
      </div>

      <Card className="p-5">
        <div className="flex flex-wrap items-start gap-4">
          <Avatar url={profile.avatar_url} name={name} className="h-14 w-14 text-lg" />
          <div className="min-w-0 flex-1 space-y-2">
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="truncate text-xl font-semibold">{name}</h2>
              {profile.username && (
                <span className="text-sm text-muted-foreground">@{profile.username}</span>
              )}
            </div>
            <div className="flex flex-wrap items-center gap-1.5">
              {indicators.map((indicator) => (
                <span
                  key={indicator.key}
                  title={indicator.title}
                  className={cx(
                    "inline-flex items-center gap-1.5 rounded-full border px-2 py-1 text-[11px] font-medium",
                    indicator.online
                      ? "border-border/80 bg-muted/40 text-foreground/90"
                      : "border-border/50 bg-muted/20 text-muted-foreground",
                  )}
                >
                  <PlatformGlyph
                    paths={indicator.paths}
                    color={indicator.online ? indicator.color : "#80848e"}
                    className="h-3.5 w-3.5"
                  />
                  {indicator.label}
                </span>
              ))}
            </div>
            <p className="text-xs text-muted-foreground">
              {onlineSummary
                ? `Online on ${onlineSummary}`
                : "Not connected yet. Start the bot to go online."}
            </p>
          </div>
          <StatusPill running={running} live={live} />
        </div>

        <div className="mt-4 flex flex-wrap items-center gap-2 border-t border-border/60 pt-4">
          {running ? (
            <Button variant="outline" onClick={onRestart}>
              <RestartIcon /> Restart
            </Button>
          ) : (
            <Button onClick={onStart}>
              <PlayIcon /> Start
            </Button>
          )}
          <Button variant="destructive" onClick={onStop} disabled={!running}>
            <StopIcon /> Stop
          </Button>
          <Button variant="ghost" className="ml-auto" onClick={onOpenLogs}>
            <TerminalIcon /> View logs
          </Button>
        </div>

        {msg && <p className="mt-3 text-xs text-muted-foreground">{msg}</p>}
        <p className="mt-3 text-xs text-muted-foreground">
          The bot does not start on its own. Press Start to connect, and closing the window
          keeps it running.
        </p>
      </Card>
    </div>
  );
}
