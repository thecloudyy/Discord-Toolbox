import { useEffect, useMemo, useRef, useState } from "react";
import type { BadgeRunState, Config } from "../types";
import { Button, Callout, Card, Field, Input, StatusPill } from "./ui";
import { AlertIcon, MedalIcon, PlayIcon, StopIcon } from "./icons";

const MAX_SAFE_HOURS = 10_000;

export function BadgesPage({
  cfg,
  state,
  log,
  message,
  onForge,
  onStop,
}: {
  cfg: Config;
  state: BadgeRunState;
  log: string;
  message: string;
  onForge: (hours: number, games: number) => void;
  onStop: () => void;
}) {
  const [hours, setHours] = useState("6000");
  const [games, setGames] = useState("250");
  const logRef = useRef<HTMLPreElement | null>(null);

  const running = Boolean(state.running);

  const parsedHours = useMemo(() => {
    const value = Number.parseFloat(hours);
    return Number.isFinite(value) && value > 0
      ? Math.min(value, MAX_SAFE_HOURS)
      : MAX_SAFE_HOURS;
  }, [hours]);

  const parsedGames = useMemo(() => {
    const raw = games.trim();
    if (!raw) return 0;
    const value = Number.parseInt(raw, 10);
    return Number.isFinite(value) && value > 0 ? value : 0;
  }, [games]);

  const perGame = parsedGames > 0 ? parsedHours / parsedGames : null;

  useEffect(() => {
    const box = logRef.current;
    if (box) box.scrollTop = box.scrollHeight;
  }, [log]);

  const tail = log.split("\n").filter(Boolean).slice(-200).join("\n");
  const summary = !running && state.username
    ? `Finished ${state.games ?? 0} game(s) for @${state.username}, ${state.hours ?? 0} hours in total.`
    : "";

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Badges</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Credit game time and game variety through Discord's own telemetry.
          </p>
        </div>
        <StatusPill running={running} live />
      </header>

      <Card className="space-y-4 p-5">
        <div className="flex items-center gap-2.5">
          <span className="ui-card flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <MedalIcon />
          </span>
          <div>
            <h3 className="text-sm font-semibold">Game badge forger</h3>
            <p className="text-xs text-muted-foreground">
              Replays the client's game telemetry once, so playtime and game variety are credited
              together. No game needs to be installed.
            </p>
          </div>
        </div>

        <Callout tone="warning">
          <span className="flex gap-2">
            <AlertIcon className="mt-0.5 h-3.5 w-3.5" />
            This posts forged telemetry to Discord and breaks the Terms of Service. Your account
            can be suspended. Use it at your own risk.
          </span>
        </Callout>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            label="Total hours"
            hint={`Split across every selected game. 5,000 total unlocks the last Game Time tier. Maximum ${MAX_SAFE_HOURS.toLocaleString()}.`}
          >
            <Input
              value={hours}
              onChange={(event) => setHours(event.target.value)}
              inputMode="decimal"
              max={MAX_SAFE_HOURS}
              spellCheck={false}
            />
          </Field>
          <Field
            label="Games"
            hint="Each title counts toward game variety. Leave empty to use every detectable game."
          >
            <Input
              value={games}
              onChange={(event) => setGames(event.target.value)}
              inputMode="numeric"
              placeholder="All"
              spellCheck={false}
            />
          </Field>
        </div>

        <p className="text-xs text-muted-foreground">
          {perGame !== null
            ? `${parsedHours.toLocaleString()} hours covers ${parsedGames.toLocaleString()} game(s), about ${perGame.toFixed(1)} hours each.`
            : `${parsedHours.toLocaleString()} hours is spread over every detectable game.`}
        </p>

        <div className="flex flex-wrap items-center gap-2 border-t border-border/60 pt-4">
          <Button onClick={() => onForge(parsedHours, parsedGames)} disabled={running}>
            <PlayIcon /> Forge badges
          </Button>
          <Button variant="destructive" onClick={onStop} disabled={!running}>
            <StopIcon /> Stop
          </Button>
          {state.username && (
            <span className="text-xs text-muted-foreground">@{state.username}</span>
          )}
        </div>

        {message && <p className="text-xs text-muted-foreground">{message}</p>}
        {state.error && <Callout tone="danger">{state.error}</Callout>}
        {summary && !state.error && (
          <p className="text-xs text-muted-foreground">{summary}</p>
        )}
      </Card>

      <Card className="flex min-h-[300px] flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-border/60 px-5 py-3">
          <div className="flex items-center gap-2">
            <MedalIcon className="text-muted-foreground" />
            <h3 className="text-sm font-semibold">Progress</h3>
          </div>
          <span className="text-xs text-muted-foreground">{running ? "Forging…" : "Idle"}</span>
        </div>
        <pre
          ref={logRef}
          className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap px-5 py-4 font-mono text-[11px] leading-5 text-muted-foreground"
        >
          {tail || "No run yet. Forge badges to see progress here."}
        </pre>
      </Card>
    </div>
  );
}
