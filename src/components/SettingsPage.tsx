import type { Config } from "../types";
import { platformMeta, spoofGroups } from "../lib/platform";
import { Button, Callout, Card, Field, Input, Select } from "./ui";
import { AlertIcon, PlatformGlyph, SlidersIcon } from "./icons";
import { cx } from "../lib/utils";

export function SettingsPage({
  cfg,
  selectedPlatform,
  msg,
  onField,
  onSetPlatform,
  onSave,
}: {
  cfg: Config;
  selectedPlatform: string;
  msg: string;
  onField: (key: string, value: string) => void;
  onSetPlatform: (value: string) => void;
  onSave: () => void;
}) {
  return (
    <div className="space-y-5">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight">Presence and platform</h1>
        <p className="mt-1 text-sm text-muted-foreground">
          Presence, activity and the client platform Discord sees. Everything here is off
          until you turn it on.
        </p>
      </header>

      <Card className="space-y-4 p-5">
        <div className="flex items-center gap-2.5">
          <span className="ui-card flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <SlidersIcon />
          </span>
          <div>
            <h3 className="text-sm font-semibold">Presence</h3>
            <p className="text-xs text-muted-foreground">
              What your account shows while it stays online.
            </p>
          </div>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            label="Status"
            hint="Keep current leaves whatever status you already have alone."
          >
            <Select
              value={cfg.STATUS || ""}
              onChange={(event) => onField("STATUS", event.target.value)}
            >
              <option value="">Keep current</option>
              <option value="online">Online</option>
              <option value="idle">Idle</option>
              <option value="dnd">Do not disturb</option>
              <option value="invisible">Invisible</option>
            </Select>
          </Field>

          <Field label="Activity type">
            <Select
              value={cfg.ACTIVITY_TYPE || ""}
              onChange={(event) => onField("ACTIVITY_TYPE", event.target.value)}
            >
              <option value="">Keep current</option>
              <option value="playing">Playing</option>
              <option value="listening">Listening</option>
              <option value="watching">Watching</option>
              <option value="competing">Competing</option>
              <option value="custom">Custom</option>
            </Select>
          </Field>

          <Field label="Activity text" hint="Shown next to the activity type. Leave it empty to keep your current activity.">
            <Input
              value={cfg.ACTIVITY_TEXT || ""}
              onChange={(event) => onField("ACTIVITY_TEXT", event.target.value)}
              placeholder="e.g. always around"
            />
          </Field>

          <Field label="Custom status" hint="Appears under your name. Leave it empty to keep your current status.">
            <Input
              value={cfg.CUSTOM_STATUS || ""}
              onChange={(event) => onField("CUSTOM_STATUS", event.target.value)}
              placeholder="e.g. always online"
            />
          </Field>
        </div>
      </Card>

      <Card className="space-y-4 p-5">
        <div className="flex items-center gap-2.5">
          <span className="ui-card flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <PlatformGlyph paths={platformMeta.desktop.paths} />
          </span>
          <div>
            <h3 className="text-sm font-semibold">Platform spoofer</h3>
            <p className="text-xs text-muted-foreground">
              The client identity Discord builds your session from. Off by default; applies on
              the next login.
            </p>
          </div>
        </div>

        <div className="space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <span className="w-16 shrink-0 text-[11px] font-semibold uppercase tracking-[0.06em] text-muted-foreground">
              Spoofing
            </span>
            <button
              type="button"
              aria-pressed={selectedPlatform === "off"}
              onClick={() => onSetPlatform("off")}
              className={cx(
                "inline-flex h-9 items-center gap-2 rounded-lg border px-3 text-sm transition-colors",
                selectedPlatform === "off"
                  ? "border-primary/60 bg-primary/15 text-primary shadow-[inset_0_1px_0_hsl(var(--surface-highlight))]"
                  : "border-border bg-background/40 text-muted-foreground hover:bg-accent hover:text-foreground",
              )}
            >
              Off
            </button>
            <span className="text-xs text-muted-foreground">
              Send the identity the bot normally uses.
            </span>
          </div>

          {spoofGroups.map((group) => {
            const meta = platformMeta[group.key];
            return (
              <div key={group.key} className="flex flex-wrap items-center gap-2">
                <span className="w-16 shrink-0 text-[11px] font-semibold uppercase tracking-[0.06em] text-muted-foreground">
                  {group.label}
                </span>
                {group.options.map((option) => {
                  const active = selectedPlatform === option.value;
                  return (
                    <button
                      key={option.value}
                      type="button"
                      aria-pressed={active}
                      onClick={() => onSetPlatform(option.value)}
                      className={cx(
                        "inline-flex h-9 items-center gap-2 rounded-lg border px-3 text-sm transition-colors",
                        active
                          ? "border-primary/60 bg-primary/15 text-primary shadow-[inset_0_1px_0_hsl(var(--surface-highlight))]"
                          : "border-border bg-background/40 text-muted-foreground hover:bg-accent hover:text-foreground",
                      )}
                    >
                      <PlatformGlyph paths={meta.paths} className="h-3.5 w-3.5" />
                      {option.label}
                    </button>
                  );
                })}
              </div>
            );
          })}
        </div>

        <Callout tone="warning">
          <span className="flex gap-2">
            <AlertIcon className="mt-0.5 h-3.5 w-3.5" />
            Console and VR identities can make the client look out of date, and forcing a platform
            you are not on may draw attention to your account.
          </span>
        </Callout>

        <p className="text-xs text-muted-foreground">
          The indicators on the dashboard come from Discord's own session list, not from this
          setting.
        </p>
      </Card>

      <Card className="space-y-4 p-5">
        <div className="flex flex-wrap items-center gap-3">
          <Button onClick={onSave}>Save configuration</Button>
          <p className="text-xs text-muted-foreground">
            Presence changes apply while the bot runs. Platform changes apply after a restart.
          </p>
        </div>

        {msg && <p className="text-xs text-muted-foreground">{msg}</p>}
      </Card>
    </div>
  );
}
