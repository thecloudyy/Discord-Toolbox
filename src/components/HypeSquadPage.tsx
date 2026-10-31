import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HypeSquadHouse, HypeSquadStatus, ProfileBadge } from "../types";
import { Avatar, Button, Callout, Card } from "./ui";
import {
  AlertIcon,
  CheckIcon,
  LoaderIcon,
  RefreshIcon,
  SparklesIcon,
  TrashIcon,
} from "./icons";
import { cx } from "../lib/utils";

type Busy = "" | "refresh" | "apply" | "remove";
type Notice = { tone: "info" | "error"; text: string };

const HOUSES: Array<{
  key: HypeSquadHouse;
  name: string;
  color: string;
  blurb: string;
}> = [
  {
    key: "bravery",
    name: "Bravery",
    color: "#9c84ef",
    blurb: "First in, no hesitation.",
  },
  {
    key: "brilliance",
    name: "Brilliance",
    color: "#ed4245",
    blurb: "Patience meets discipline.",
  },
  {
    key: "balance",
    name: "Balance",
    color: "#45ddc0",
    blurb: "Calm in the chaos.",
  },
];

function houseName(house: HypeSquadHouse) {
  return HOUSES.find((entry) => entry.key === house)?.name ?? house;
}

function houseColor(house: HypeSquadHouse) {
  return HOUSES.find((entry) => entry.key === house)?.color ?? "#8b93a7";
}

function HouseEmblem({ house, className }: { house: HypeSquadHouse; className?: string }) {
  const common = {
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 1.7,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    className: cx("h-5 w-5", className),
  };

  if (house === "bravery") {
    return (
      <svg {...common}>
        <path d="M12 3 5.5 20l6.5-3 6.5 3L12 3Z" />
        <path d="M12 9.5 9.6 15.6l2.4-1.1 2.4 1.1L12 9.5Z" />
      </svg>
    );
  }
  if (house === "brilliance") {
    return (
      <svg {...common}>
        <path d="M12 3.2 14 9.9l6.8 2.1-6.8 2.1-2 6.7-2-6.7L3.2 12 10 9.9l2-6.7Z" />
      </svg>
    );
  }
  return (
    <svg {...common}>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2.6v3M12 18.4v3M2.6 12h3M18.4 12h3M5.4 5.4l2.1 2.1M16.5 16.5l2.1 2.1M18.6 5.4l-2.1 2.1M7.5 16.5l-2.1 2.1" />
    </svg>
  );
}

function BadgeIcon({ badge }: { badge: ProfileBadge }) {
  const [failed, setFailed] = useState(false);
  if (badge.icon_url && !failed) {
    return (
      <img
        src={badge.icon_url}
        alt=""
        draggable={false}
        onError={() => setFailed(true)}
        className="h-6 w-6 object-contain"
      />
    );
  }
  return (
    <span className="flex h-6 w-6 items-center justify-center rounded-md bg-muted text-[10px] font-semibold uppercase text-muted-foreground">
      {badge.description.slice(0, 1)}
    </span>
  );
}

export function HypeSquadPage({ token }: { token: string }) {
  const [status, setStatus] = useState<HypeSquadStatus | null>(null);
  const [selected, setSelected] = useState<HypeSquadHouse | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState<Busy>("");
  const [notice, setNotice] = useState<Notice | null>(null);

  const load = useCallback(
    async (mode: "" | "refresh" = "") => {
      if (!token) {
        setNotice({ tone: "error", text: "Sign in to a Discord account first." });
        return;
      }
      if (mode === "refresh") {
        setBusy("refresh");
      } else {
        setLoading(true);
      }
      try {
        const next = await invoke<HypeSquadStatus>("hypesquad_status", { token });
        setStatus(next);
        if (mode === "refresh") {
          setNotice({ tone: "info", text: "Refreshed." });
        }
      } catch (error) {
        setNotice({ tone: "error", text: String(error) });
      } finally {
        setBusy("");
        setLoading(false);
      }
    },
    [token],
  );

  useEffect(() => {
    setSelected(null);
    setNotice(null);
    void load();
  }, [load]);

  async function apply() {
    if (!selected || busy || !token) return;
    const target = selected;
    setBusy("apply");
    setNotice(null);
    try {
      const next = await invoke<HypeSquadStatus>("set_hypesquad", { token, target });
      setStatus(next);
      setSelected(null);
      setNotice({ tone: "info", text: `Done. You're in ${houseName(target)} now.` });
    } catch (error) {
      setNotice({ tone: "error", text: String(error) });
    } finally {
      setBusy("");
    }
  }

  async function remove() {
    if (busy || !token) return;
    if (!status?.house) {
      setNotice({ tone: "info", text: "You don't have a HypeSquad badge to remove." });
      return;
    }

    setBusy("remove");
    setNotice(null);
    try {
      const next = await invoke<HypeSquadStatus>("remove_hypesquad", { token });
      setStatus(next);
      setSelected(null);
      setNotice({ tone: "info", text: "HypeSquad badge removed." });
    } catch (error) {
      setNotice({ tone: "error", text: String(error) });
    } finally {
      setBusy("");
    }
  }

  const currentHouse = status?.house ?? null;
  const canApply = Boolean(selected) && !busy && !loading;
  const canRemove = !busy && !loading && Boolean(currentHouse);
  const badgeCount = status?.badges.length ?? 0;

  return (
    <div className="space-y-5">
      <header className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight">HypeSquad</h1>
          <p className="mt-1 text-sm text-muted-foreground">
            Switch your house or drop the badge. Your profile badges are shown below.
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void load("refresh")}
          disabled={Boolean(busy) || loading}
        >
          {busy === "refresh" || loading ? <LoaderIcon /> : <RefreshIcon />}
          Refresh
        </Button>
      </header>

      <Callout tone="warning">
        <span className="flex gap-2">
          <AlertIcon className="mt-0.5 h-3.5 w-3.5" />
          These requests go to Discord with your account token, which is against their Terms of
          Service. Your account could get suspended.
        </span>
      </Callout>

      {loading && !status ? (
        <Card className="flex items-center gap-3 p-5 text-sm text-muted-foreground">
          <LoaderIcon /> Loading your profile…
        </Card>
      ) : status ? (
        <Card className="overflow-hidden">
          <div className="flex flex-wrap items-center gap-4 p-5">
            <Avatar
              url={status.avatar_url}
              name={status.display_name}
              className="h-14 w-14 text-lg"
            />
            <div className="min-w-0 flex-1">
              <p className="truncate text-lg font-semibold">{status.display_name}</p>
              <p className="truncate text-xs text-muted-foreground">@{status.username}</p>
              {currentHouse && (
                <span
                  className="mt-2 inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-[11px] font-medium"
                  style={{
                    borderColor: `${houseColor(currentHouse)}66`,
                    color: houseColor(currentHouse),
                    backgroundColor: `${houseColor(currentHouse)}14`,
                  }}
                >
                  <HouseEmblem house={currentHouse} className="h-3.5 w-3.5" />
                  {houseName(currentHouse)}
                </span>
              )}
            </div>
          </div>

          <div className="border-t border-border/60 px-5 py-4">
            <div className="flex items-center justify-between gap-3">
              <h3 className="text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
                Badges
              </h3>
              <span className="text-[11px] text-muted-foreground">
                {badgeCount} {badgeCount === 1 ? "badge" : "badges"}
              </span>
            </div>
            {badgeCount > 0 ? (
              <ul className="mt-3 grid grid-cols-2 gap-2 sm:grid-cols-3">
                {status.badges.map((badge) => (
                  <li
                    key={badge.id}
                    className="flex items-center gap-2.5 rounded-lg border border-border/70 bg-muted/30 px-2.5 py-2"
                    title={badge.id}
                  >
                    <BadgeIcon badge={badge} />
                    <span className="min-w-0 truncate text-xs text-muted-foreground">
                      {badge.description}
                    </span>
                  </li>
                ))}
              </ul>
            ) : (
              <p className="mt-2 text-xs text-muted-foreground">
                Discord didn't return any badges for this account.
              </p>
            )}
          </div>
        </Card>
      ) : null}

      <Card className="space-y-4 p-5">
        <div className="flex items-center gap-2.5">
          <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-primary/10 text-primary">
            <SparklesIcon />
          </span>
          <div>
            <h3 className="text-sm font-semibold">Pick a house</h3>
            <p className="text-xs text-muted-foreground">
              You can change this whenever you want.
            </p>
          </div>
        </div>

        <div className="grid gap-2.5 sm:grid-cols-3">
          {HOUSES.map((house) => {
            const isSelected = selected === house.key;
            const isCurrent = currentHouse === house.key;
            return (
              <button
                key={house.key}
                type="button"
                onClick={() => setSelected(isSelected ? null : house.key)}
                disabled={Boolean(busy) || loading}
                aria-pressed={isSelected}
                className={cx(
                  "ui-card relative rounded-xl border p-3.5 text-left transition-colors disabled:opacity-60",
                  isSelected ? "bg-muted/40" : "hover:bg-muted/30",
                )}
                style={
                  isSelected
                    ? {
                        borderColor: `${house.color}99`,
                        boxShadow: `0 0 0 1px ${house.color}66, 0 12px 28px -20px ${house.color}`,
                      }
                    : undefined
                }
              >
                <span className="flex items-center gap-2.5">
                  <span
                    className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg"
                    style={{ backgroundColor: `${house.color}1f`, color: house.color }}
                  >
                    <HouseEmblem house={house.key} />
                  </span>
                  <span className="min-w-0 text-sm font-semibold">{house.name}</span>
                </span>
                <span className="mt-2 block text-xs leading-relaxed text-muted-foreground">
                  {house.blurb}
                </span>
                <span className="mt-2 flex flex-wrap gap-1.5">
                  {isCurrent && (
                    <span
                      className="inline-flex items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium"
                      style={{ backgroundColor: `${house.color}1f`, color: house.color }}
                    >
                      <CheckIcon className="h-3 w-3" /> Current
                    </span>
                  )}
                  {isSelected && (
                    <span className="inline-flex items-center gap-1 rounded-full bg-primary/15 px-1.5 py-0.5 text-[10px] font-medium text-primary">
                      Selected
                    </span>
                  )}
                </span>
              </button>
            );
          })}
        </div>

        <div className="flex flex-wrap items-center gap-2 border-t border-border/60 pt-4">
          <Button onClick={() => void apply()} disabled={!canApply}>
            {busy === "apply" ? <LoaderIcon /> : <SparklesIcon />}
            {selected ? `Apply ${houseName(selected)}` : "Apply house"}
          </Button>
          <Button variant="destructive" onClick={() => void remove()} disabled={!canRemove}>
            {busy === "remove" ? <LoaderIcon /> : <TrashIcon />}
            Remove badge
          </Button>
          {!selected && (
            <span className="text-xs text-muted-foreground">Pick a house to switch.</span>
          )}
        </div>

        {notice && (
          <Callout tone={notice.tone === "error" ? "danger" : "muted"}>
            {notice.text}
          </Callout>
        )}

        <p className="text-xs text-muted-foreground">
          Removing doesn't always work. Discord rejects it for some accounts, so switching houses
          is the safer bet.
        </p>
      </Card>
    </div>
  );
}
