import { useEffect, useRef, useState } from "react";
import type { Profile } from "../types";
import { Avatar } from "./ui";
import { LogOutIcon } from "./icons";
import { cx } from "../lib/utils";

export function AccountMenu({
  profile,
  running,
  onSwitchAccount,
}: {
  profile: Profile;
  running: boolean;
  onSwitchAccount: () => void;
}) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);
  const name = profile.global_name || profile.username || "Discord account";

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onPointerDown);
    return () => document.removeEventListener("mousedown", onPointerDown);
  }, [open]);

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        className={cx("rail-button", open && "rail-button-active")}
        title={name}
        aria-label={`Account: ${name}`}
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <Avatar url={profile.avatar_url} name={name} className="h-7 w-7 text-xs" />
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{name}</span>
        <span
          className={cx(
            "h-2 w-2 shrink-0 rounded-full",
            running ? "bg-emerald-400" : "bg-muted-foreground/70",
          )}
        />
      </button>

      {open && (
        <div className="fade-in absolute bottom-full left-0 z-50 mb-2 w-64 rounded-xl border bg-popover p-2 text-popover-foreground shadow-[0_16px_40px_-12px_rgba(0,0,0,0.7)]">
          <div className="flex items-center gap-3 px-2 py-2">
            <Avatar url={profile.avatar_url} name={name} className="h-10 w-10 text-sm" />
            <div className="min-w-0">
              <p className="truncate text-sm font-semibold">{name}</p>
              <p className="truncate text-xs text-muted-foreground">
                @{profile.username ?? "unknown"}
              </p>
            </div>
          </div>
          <div className="my-1 h-px bg-border" />
          <button
            type="button"
            className="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
            onClick={() => {
              setOpen(false);
              onSwitchAccount();
            }}
          >
            <LogOutIcon />
            Switch account
          </button>
        </div>
      )}
    </div>
  );
}
