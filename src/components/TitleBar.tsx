import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import logo from "../assets/logo.png";
import {
  CloseIcon,
  MaximizeIcon,
  MinusIcon,
  RestoreIcon,
} from "./icons";

const appWindow = (() => {
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
})();

export function TitleBar() {
  const [version, setVersion] = useState("1.0.3");
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let disposed = false;
    void getVersion()
      .then((value) => {
        if (!disposed) setVersion(value);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    if (!appWindow) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const sync = () => {
      void appWindow
        ?.isMaximized()
        .then((value) => {
          if (!disposed) setMaximized(value);
        })
        .catch(() => undefined);
    };

    sync();
    void appWindow
      .onResized(sync)
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => undefined);

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  function handleDrag(event: React.MouseEvent<HTMLElement>) {
    if (!appWindow || event.button !== 0) return;
    if (event.detail === 2) {
      void appWindow.toggleMaximize();
      return;
    }
    void appWindow.startDragging();
  }

  return (
    <header className="flex h-12 w-full shrink-0 select-none border-b border-border/40 bg-background">
      <div
        className="flex h-full w-14 shrink-0 items-center justify-center"
        onMouseDown={handleDrag}
      >
        <img
          src={logo}
          alt=""
          draggable={false}
          className="h-8 w-8 rounded-[10px] shadow-[0_4px_14px_-6px_hsl(var(--primary)/0.7)]"
        />
      </div>
      <div
        className="flex h-full min-w-0 flex-1 items-center gap-2"
        onMouseDown={handleDrag}
      >
        <span className="truncate text-xs font-medium text-foreground/90">
          Discord Toolbox
        </span>
        <span className="rounded-full border border-border/70 bg-muted/40 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
          v{version}
        </span>
      </div>
      <div className="flex h-full shrink-0">
        <button
          type="button"
          className="titlebar-button"
          title="Minimize"
          aria-label="Minimize"
          tabIndex={-1}
          onClick={() => void appWindow?.minimize()}
        >
          <MinusIcon />
        </button>
        <button
          type="button"
          className="titlebar-button"
          title={maximized ? "Restore" : "Maximize"}
          aria-label={maximized ? "Restore" : "Maximize"}
          tabIndex={-1}
          onClick={() => void appWindow?.toggleMaximize()}
        >
          {maximized ? <RestoreIcon /> : <MaximizeIcon />}
        </button>
        <button
          type="button"
          className="titlebar-button titlebar-button-close"
          title="Close"
          aria-label="Close"
          tabIndex={-1}
          onClick={() => void appWindow?.close()}
        >
          <CloseIcon />
        </button>
      </div>
    </header>
  );
}