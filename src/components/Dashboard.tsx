import type { Config, Profile } from "../types";
import type { PlatformIndicator } from "../lib/platform";
import { Avatar, Button, Card, Field, Input, StatusPill } from "./ui";
import {
  HashIcon,
  PlayIcon,
  PlatformGlyph,
  PowerIcon,
  RestartIcon,
  ServerIcon,
  StopIcon,
  TerminalIcon,
} from "./icons";
import { cx } from "../lib/utils";

export function Dashboard({
  profile,
  indicators,
  onlineSummary,
  running,
  live,
  cfg,
  log,
  msg,
  onField,
  onSave,
  onStart,
  onStop,
  onRestart,
  onTerminate,
  onOpenLogs,
}: {
  profile: Profile;
  indicators: PlatformIndicator[];
  onlineSummary: string;
  running: boolean;
  live: boolean;
  cfg: Config;
  log: string;
  msg: string;
  onField: (key: string, value: string) => void;
  onSave: () => void;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onTerminate: () => void;
  onOpenLogs: () => void;
}) {
  const name = profile.global_name || profile.username || "Discord account";
  const tail =
    log.split("\n").filter(Boolean).slice(-12).join("\n");

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Keep alive</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Keep your account online, with or without a voice channel.
          </p>
        </div>
        <StatusPill running={running} live={live} />
      </header>

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
          <Button variant="ghost" onClick={onTerminate} disabled={!running}>
            <PowerIcon /> Terminate
          </Button>
          <Button variant="ghost" className="ml-auto" onClick={onOpenLogs}>
            <TerminalIcon /> View log
          </Button>
        </div>

        {msg && <p className="mt-3 text-xs text-muted-foreground">{msg}</p>}
      </Card>

      <Card className="p-5">
        <div className="flex items-center gap-2.5">
          <span className="ui-card flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <HashIcon />
          </span>
          <div>
            <h3 className="text-sm font-semibold">Voice channel (optional)</h3>
            <p className="text-xs text-muted-foreground">
              Join this channel and stay in it. Leave both fields empty to stay online only.
            </p>
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="Server ID">
            <div className="relative">
              <ServerIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={cfg.DISCORD_GUILD_ID || ""}
                onChange={(event) => onField("DISCORD_GUILD_ID", event.target.value)}
                placeholder="123456789012345678"
                inputMode="numeric"
                spellCheck={false}
                className="pl-9"
              />
            </div>
          </Field>
          <Field label="Voice channel ID">
            <div className="relative">
              <HashIcon className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={cfg.DISCORD_CHANNEL_ID || ""}
                onChange={(event) => onField("DISCORD_CHANNEL_ID", event.target.value)}
                placeholder="123456789012345678"
                inputMode="numeric"
                spellCheck={false}
                className="pl-9"
              />
            </div>
          </Field>
        </div>

        <div className="flex flex-wrap items-center gap-3">
          <Button variant="secondary" onClick={onSave}>
            Save configuration
          </Button>
          <p className="text-xs text-muted-foreground">
            {cfg.DISCORD_CHANNEL_ID
              ? "Server ID is required when a voice channel is set."
              : "Leave both fields empty to stay online only."}
          </p>
        </div>
      </Card>

      <Card>
        <div className="flex items-center justify-between gap-3 border-b border-border/60 px-5 py-3.5">
          <div className="flex items-center gap-2">
            <TerminalIcon className="text-muted-foreground" />
            <h3 className="text-sm font-semibold">Recent activity</h3>
          </div>
          <Button variant="ghost" size="sm" onClick={onOpenLogs}>
            Open log
          </Button>
        </div>
        <pre className="max-h-56 overflow-auto whitespace-pre-wrap px-5 py-4 font-mono text-[11px] leading-5 text-muted-foreground">
          {tail || "No output yet. Start the bot to see it connect."}
        </pre>
      </Card>
    </div>
  );
}