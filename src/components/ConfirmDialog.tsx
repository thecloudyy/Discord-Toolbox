import { Button } from "./ui";
import { AlertIcon, LoaderIcon } from "./icons";

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel,
  busy,
  onConfirm,
  onCancel,
}: {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  busy: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6 backdrop-blur-sm">
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-title"
        className="w-full max-w-md rounded-xl border bg-card p-5 shadow-2xl"
      >
        <div className="flex items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-destructive/10 text-destructive">
            <AlertIcon className="h-4 w-4" />
          </span>
          <div className="min-w-0 space-y-1.5">
            <h2 id="confirm-title" className="text-base font-semibold">
              {title}
            </h2>
            <p className="text-sm leading-relaxed text-muted-foreground">{description}</p>
          </div>
        </div>
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="outline" onClick={onCancel} disabled={busy}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={onConfirm} disabled={busy}>
            {busy && <LoaderIcon />}
            {confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
