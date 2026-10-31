import { useState } from "react";
import type {
  ButtonHTMLAttributes,
  InputHTMLAttributes,
  ReactNode,
  SelectHTMLAttributes,
} from "react";
import { ChevronDownIcon } from "./icons";
import { cx } from "../lib/utils";

type ButtonVariant =
  | "default"
  | "secondary"
  | "outline"
  | "ghost"
  | "destructive"
  | "success"
  | "warning";
type ButtonSize = "sm" | "md" | "icon";

const buttonVariants: Record<ButtonVariant, string> = {
  default:
    "bg-primary text-primary-foreground shadow-[inset_0_1px_0_rgba(255,255,255,0.25),0_3px_8px_-3px_hsl(var(--primary)/0.35)] hover:brightness-110",
  secondary:
    "bg-secondary text-secondary-foreground hover:bg-secondary/80",
  outline:
    "border border-border bg-transparent text-foreground hover:bg-accent hover:text-accent-foreground",
  ghost: "text-muted-foreground hover:bg-accent hover:text-accent-foreground",
  destructive: "bg-destructive text-destructive-foreground hover:brightness-110",
  success: "bg-emerald-600 text-white hover:bg-emerald-500",
  warning: "bg-amber-600 text-white hover:bg-amber-500",
};

const buttonSizes: Record<ButtonSize, string> = {
  sm: "h-9 px-3 text-sm",
  md: "h-10 px-4 text-sm",
  icon: "h-9 w-9",
};

export function Button({
  variant = "default",
  size = "md",
  className,
  type = "button",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
}) {
  return (
    <button
      type={type}
      className={cx(
        "inline-flex select-none items-center justify-center gap-2 rounded-lg font-medium transition-[color,background-color,box-shadow,transform,filter] duration-150",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:ring-offset-background",
        "active:translate-y-px disabled:pointer-events-none disabled:opacity-50",
        buttonSizes[size],
        buttonVariants[variant],
        className,
      )}
      {...props}
    />
  );
}

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return <input className={cx("ui-input", className)} {...props} />;
}

export function Select({
  className,
  children,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <div className="relative">
      <select
        className={cx("ui-input appearance-none pr-9", className)}
        {...props}
      >
        {children}
      </select>
      <ChevronDownIcon
        className="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground"
      />
    </div>
  );
}

export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <section
      className={cx("ui-card rounded-xl border bg-card text-card-foreground", className)}
    >
      {children}
    </section>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <label className="block space-y-1.5">
      <span className="block text-[11px] font-semibold uppercase tracking-[0.08em] text-muted-foreground">
        {label}
      </span>
      {children}
      {hint && <span className="block text-xs text-muted-foreground/80">{hint}</span>}
    </label>
  );
}

export function Avatar({
  url,
  name,
  className,
}: {
  url?: string | null;
  name?: string | null;
  className?: string;
}) {
  const [failedUrl, setFailedUrl] = useState<string | null>(null);
  const showImage = Boolean(url && failedUrl !== url);

  if (showImage) {
    return (
      <img
        src={url ?? undefined}
        alt=""
        draggable={false}
        onError={() => setFailedUrl(url ?? null)}
        className={cx("shrink-0 rounded-full bg-muted object-cover", className)}
      />
    );
  }

  const initial = (name ?? "?").trim().charAt(0).toUpperCase() || "?";
  return (
    <div
      className={cx(
        "flex shrink-0 items-center justify-rounded-full bg-primary/15 font-semibold text-primary",
        className,
      )}
    >
      {initial}
    </div>
  );
}

export function Switch({
  checked,
  onChange,
  label,
  description,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label: string;
  description?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="ui-card flex w-full items-center justify-between gap-4 rounded-xl border bg-muted/30 p-4 text-left transition-colors hover:bg-muted/50"
    >
      <span className="min-w-0">
        <span className="block text-sm font-medium">{label}</span>
        {description && (
          <span className="mt-0.5 block text-xs text-muted-foreground">
            {description}
          </span>
        )}
      </span>
      <span
        className={cx(
          "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full transition-colors",
          checked ? "bg-primary" : "bg-input",
        )}
      >
        <span
          className={cx(
            "inline-block h-5 w-5 transform rounded-full bg-white shadow transition-transform",
            checked ? "translate-x-[22px]" : "translate-x-0.5",
          )}
        />
      </span>
    </button>
  );
}

export function StatusPill({ running, live }: { running: boolean; live: boolean }) {
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium",
        running
          ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-300"
          : "border-border bg-muted/40 text-muted-foreground",
      )}
    >
      <span
        className={cx(
          "h-1.5 w-1.5 rounded-full",
          running ? "bg-emerald-400" : "bg-muted-foreground/60",
          running && !live && "animate-pulse",
        )}
      />
      {running ? (live ? "Running" : "Reconnecting…") : "Stopped"}
    </span>
  );
}

export function Callout({
  tone = "muted",
  children,
}: {
  tone?: "muted" | "warning" | "danger";
  children: ReactNode;
}) {
  const tones = {
    muted: "border-border/70 bg-muted/30 text-muted-foreground",
    warning: "border-amber-500/25 bg-amber-500/5 text-amber-200/90",
    danger: "border-destructive/30 bg-destructive/5 text-destructive",
  };
  return (
    <div className={cx("rounded-lg border px-3 py-2.5 text-xs leading-relaxed", tones[tone])}>
      {children}
    </div>
  );
}