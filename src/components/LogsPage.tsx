import type { RefObject } from "react";
import { Button, Card, StatusPill } from "./ui";
import { TerminalIcon, TrashIcon } from "./icons";

export function LogsPage({
  log,
  running,
  live,
  onClear,
  logBoxRef,
}: {
  log: string;
  running: boolean;
  live: boolean;
  onClear: () => void;
  logBoxRef: RefObject<HTMLPreElement | null>;
}) {
  const lines = log ? log.split("\n").length : 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4">
      <header className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">Logs</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Bot output, updated as it runs.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <StatusPill running={running} live={live} />
          <Button variant="outline" size="sm" onClick={onClear} disabled={!log}>
            <TrashIcon />
            Clear
          </Button>
        </div>
      </header>

      <Card className="flex min-h-[420px] flex-1 flex-col overflow-hidden">
        <div className="flex items-center justify-between border-b border-border/60 px-4 py-2.5 text-xs text-muted-foreground">
          <span className="flex items-center gap-2">
            <TerminalIcon />
            engine output
          </span>
          <span>{lines ? `${lines} line${lines === 1 ? "" : "s"}` : "empty"}</span>
        </div>
        <pre
          ref={logBoxRef}
          className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap px-4 py-3 font-mono text-[11px] leading-5 text-foreground/80"
        >
          {log || "No output yet. Start the bot from the dashboard."}
        </pre>
      </Card>
    </div>
  );
}
