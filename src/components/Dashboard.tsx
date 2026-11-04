import type { Profile } from "../types";
import type { PlatformIndicator } from "../lib/platform";
import { Avatar, Button, Card, StatusPill } from "./ui";
import { PlayIcon, PlatformGlyph, RestartIcon, StopIcon, TerminalIcon } from "./icons";
import { cx } from "../lib/utils";

export function Dashboard({
  profile,
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
          The bot does not start on its own. Press Start to connect, and closing the window
          keeps it running.
        </p>
      </Card>
    </div>
  );
}
