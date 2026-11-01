import type { Config } from "../types";
import { Button, Card, Field, Input, Switch } from "./ui";
import { HashIcon, ServerIcon } from "./icons";

export function VcKeeperPage({
  cfg,
  selfDeaf,
  msg,
  onField,
  onSelfDeaf,
  onSave,
}: {
  cfg: Config;
  selfDeaf: boolean;
  msg: string;
  onField: (key: string, value: string) => void;
  onSelfDeaf: (value: boolean) => void;
  onSave: () => void;
}) {
  return (
    <div className="space-y-5">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">VC keeper</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Stay online, optionally parked in a voice channel.
        </p>
      </header>

      <Card className="space-y-4 p-5">
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

        <p className="text-xs text-muted-foreground">
          {cfg.DISCORD_CHANNEL_ID
            ? "Server ID is required when a voice channel is set."
            : "Leave both fields empty to stay online only."}
        </p>

        <div className="border-t border-border/60 pt-4">
          <Switch
            checked={selfDeaf}
            onChange={onSelfDeaf}
            label="Self-deafen"
            description="Join voice with your microphone muted and headphones off. Applies after a restart."
          />
        </div>

        <div className="flex flex-wrap items-center gap-3 border-t border-border/60 pt-4">
          <Button onClick={onSave}>Save configuration</Button>
          <p className="text-xs text-muted-foreground">
            Voice changes apply while the bot runs. Self-deafen applies after a restart.
          </p>
        </div>

        {msg && <p className="text-xs text-muted-foreground">{msg}</p>}
      </Card>
    </div>
  );
}
