import { useState } from "react";
import type { DetectedAccount } from "../types";
import logo from "../assets/logo.png";
import { Avatar, Button, Callout, Input } from "./ui";
import {
  AlertIcon,
  CloseIcon,
  KeyIcon,
  LoaderIcon,
  RefreshIcon,
  SparklesIcon,
  UserIcon,
} from "./icons";

export function LoginPanel({
  tokens,
  detecting,
  notes,
  manualToken,
  checking,
  message,
  onManualToken,
  onUseManual,
  onDetect,
  onSelect,
}: {
  tokens: DetectedAccount[];
  detecting: boolean;
  notes: string[];
  manualToken: string;
  checking: boolean;
  message: string;
  onManualToken: (value: string) => void;
  onUseManual: () => void;
  onDetect: () => void;
  onSelect: (account: DetectedAccount) => void;
}) {
  const [manualOpen, setManualOpen] = useState(false);
  const [showMethods, setShowMethods] = useState(false);
  const accountsView = tokens.length > 0 && !showMethods;

  return (
    <section className="flex flex-1 flex-col items-center justify-center gap-6 py-6">
      <div className="flex items-center gap-4">
        <img
          src={logo}
          alt=""
          className="h-14 w-14 rounded-2xl shadow-[0_10px_30px_-10px_hsl(var(--primary)/0.6)]"
        />
        <div>
          <h2 className="text-2xl font-semibold tracking-tight">Welcome</h2>
          <p className="mt-1 text-sm text-muted-foreground">
            Sign in with the Discord account you want to keep in voice.
          </p>
        </div>
      </div>

      <div className="w-full max-w-2xl overflow-hidden rounded-xl border bg-card/60 shadow-[0_16px_50px_-32px_hsl(var(--primary)/0.45)]">
        {accountsView ? (
          <>
            <div className="flex items-start justify-between gap-4 border-b border-border/70 px-5 py-4">
              <div>
                <h3 className="font-semibold">Select an account</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                  {tokens.length} account{tokens.length === 1 ? "" : "s"} found on this PC
                </p>
              </div>
              <Button variant="ghost" size="sm" onClick={() => setShowMethods(true)}>
                Use a token instead
              </Button>
            </div>

            <div className="max-h-80 space-y-2 overflow-y-auto p-4">
              {tokens.map((account) => {
                const name = account.global_name || account.username || "Discord account";
                return (
                  <button
                    key={account.token}
                    type="button"
                    onClick={() => onSelect(account)}
                    className="flex w-full items-center gap-3 rounded-lg border border-border/70 bg-background/40 px-4 py-3 text-left transition-colors hover:bg-accent/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  >
                    <Avatar
                      url={account.avatar_url}
                      name={name}
                      className="h-9 w-9 rounded-lg text-sm"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-semibold">{name}</span>
                      <span className="block truncate text-xs text-muted-foreground">
                        @{account.username ?? "unknown"}
                      </span>
                    </span>
                    <span className="shrink-0 rounded-full border border-border/70 bg-muted/40 px-2 py-0.5 text-[10px] text-muted-foreground">
                      {account.client}
                    </span>
                  </button>
                );
              })}
            </div>

            <div className="flex flex-wrap items-center gap-2 border-t border-border/70 px-5 py-3">
              <Button variant="ghost" size="sm" onClick={onDetect} disabled={detecting}>
                {detecting ? <LoaderIcon /> : <RefreshIcon />}
                {detecting ? "Scanning…" : "Detect again"}
              </Button>
              {message && <span className="text-xs text-destructive">{message}</span>}
            </div>
          </>)
          : (
          <div className="space-y-5 p-6">
            <div>
              <h3 className="font-semibold">Discord account</h3>
              <p className="mt-1 text-sm text-muted-foreground">
                Detect the account signed in to the Discord desktop app, or paste a token.
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <Button onClick={onDetect} disabled={detecting}>
                {detecting ? <LoaderIcon /> : <SparklesIcon />}
                {detecting ? "Scanning…" : "Auto Detect Token"}
              </Button>
              <Button variant="outline" onClick={() => setManualOpen((value) => !value)}>
                <KeyIcon /> Manual Input
              </Button>
              {tokens.length > 0 && (
                <Button variant="ghost" onClick={() => setShowMethods(false)}>
                  <UserIcon /> {tokens.length} detected
                </Button>
              )}
            </div>

            {manualOpen && (
              <div className="fade-in flex flex-wrap items-center gap-2">
                <Input
                  type="password"
                  value={manualToken}
                  onChange={(event) => onManualToken(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") onUseManual();
                  }}
                  placeholder="Paste your Discord token"
                  autoComplete="off"
                  spellCheck={false}
                  className="min-w-[220px] flex-1"
                />
                <Button onClick={onUseManual} disabled={checking || !manualToken.trim()}>
                  {checking && <LoaderIcon />}
                  Sign in
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label="Close manual input"
                  onClick={() => setManualOpen(false)}
                >
                  <CloseIcon />
                </Button>
              </div>
            )}

            {detecting && (
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <LoaderIcon /> Reading local Discord profiles…
              </p>
            )}

            {!detecting && notes.length > 0 && (
              <div className="space-y-1.5">
                {notes.map((note) => (
                  <Callout key={note} tone="warning">
                    <span className="flex gap-2">
                      <AlertIcon className="mt-0.5 h-3.5 w-3.5" />
                      {note}
                    </span>
                  </Callout>
                ))}
              </div>
            )}

            {message && <Callout tone="danger">{message}</Callout>}
          </div>
        )}
      </div>

      <p className="max-w-md text-center text-xs leading-relaxed text-muted-foreground">
        Auto-detect reads the token your Discord desktop client already stores on this PC. The
        token is only used to sign this account in — never share it with anyone.
      </p>
    </section>
  );
}