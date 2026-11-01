import type { Config, Profile } from "../types";
import type { PlatformIndicator } from "../lib/platform";
import { Avatar, Button, Card, StatusPill } from "./ui";
import { HashIcon, PlayIcon, PlatformGlyph, RestartIcon, StopIcon, TerminalIcon } from "./icons";
import { cx } from "../lib/utils";

export function Dashboard({
  profile,
  indicators,
  onlineSummary,
  running,
  live,
  cfg,
  msg,
  onStart,
  onStop,
  onRestart,
  onOpenLogs,
  onOpenVoice,
}: {
  profile: Profile;
  indicators: PlatformIndicator[];
  onlineSummary: string;
  running: boolean;
  live: boolean;
  cfg: Config;
  msg: string;
  onStart: () => void;
  onStop: () => void;
  onRestart: () => void;
  onOpenLogs: () => void;
  onOpenVoice: () => void;
}) {
  const name = profile.global_name || profile.username || "Discord account";
  const channel = (cfg.DISCORD_CHANNEL_ID || "").trim();
  const mode = channel
    ? `Staying in voice channel ${channel}.`
    : "Keeping the account online without joining voice.";

  return (
    <div className="space-y-5">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">Dashboard</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Account status and bot controls.
        </p>
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
          Closing the window keeps the bot running. Use Stop to end it.
        </p>
      </Card>

      <Card className="flex flex-wrap items-center gap-4 p-5">
        <span className="ui-card flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
          <HashIcon />
        </span>
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-semibold">Running mode</h3>
          <p className="mt-0.5 text-xs text-muted-foreground">{mode}</p>
        </div>
        <Button variant="outline" onClick={onOpenVoice}>
          VC keeper
        </Button>
      </Card>
    </div>
  );
}
