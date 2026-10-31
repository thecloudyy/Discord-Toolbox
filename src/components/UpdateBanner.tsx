import type { UpdateInfo, UpdateProgress } from "../types";
import { Button } from "./ui";
import { ArrowUpCircleIcon, DownloadIcon, LoaderIcon } from "./icons";

export function UpdateBanner({
  info,
  progress,
  error,
  onDownload,
}: {
  info: UpdateInfo;
  progress: UpdateProgress | null;
  error: string;
  onDownload: () => void;
}) {
  const busy = Boolean(progress) && !error;
  const label =
    progress?.phase === "installing"
      ? "Starting installer…"
      : progress?.phase === "downloading"
        ? `Downloading… ${progress.percent ?? 0}%`
        : "Download Update";

  return (
    <div
      role="status"
      className="mb-6 flex shrink-0 flex-col items-start justify-between gap-3 rounded-xl border border-primary/30 bg-primary/10 p-4 sm:flex-row sm:items-center"
    >
      <div className="flex items-center gap-3">
        <ArrowUpCircleIcon className="h-5 w-5 shrink-0 text-primary" />
        <div>
          <p className="font-semibold text-primary">A new version is available!</p>
          <p className="text-sm text-muted-foreground">
            Version {info.version} is now available. You are currently on {info.current}.
          </p>
        </div>
      </div>
      <div className="flex shrink-0 flex-col items-end gap-1">
        <Button onClick={onDownload} disabled={busy} className="shrink-0">
          {busy ? <LoaderIcon /> : <DownloadIcon />}
          {label}
        </Button>
        {error && (
          <p className="max-w-[300px] text-right text-xs text-destructive">{error}</p>
        )}
      </div>
    </div>
  );
}
