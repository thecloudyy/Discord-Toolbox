import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AppRail } from "./components/AppRail";
import { BadgesPage } from "./components/BadgesPage";
import { ConfirmDialog } from "./components/ConfirmDialog";
import { Dashboard } from "./components/Dashboard";
import { HypeSquadPage } from "./components/HypeSquadPage";
import { LoginPanel } from "./components/LoginPanel";
import { LogsPage } from "./components/LogsPage";
import { SettingsPage } from "./components/SettingsPage";
import { TitleBar } from "./components/TitleBar";
import { UpdateBanner } from "./components/UpdateBanner";
import {
  buildPlatformIndicators,
  onlineSummaryOf,
  platformAlias,
  spoofClientMap,
} from "./lib/platform";
import type {
  BadgeRunState,
  BotStatus,
  Config,
  DetectedAccount,
  Profile,
  ScanReport,
  Session,
  Tab,
  UpdateInfo,
  UpdateProgress,
} from "./types";

export default function App() {
  const [tab, setTab] = useState<Tab>("home");
  const [tokens, setTokens] = useState<DetectedAccount[]>([]);
  const [scanNotes, setScanNotes] = useState<string[]>([]);
  const [detecting, setDetecting] = useState(false);
  const [checking, setChecking] = useState(false);
  const [accountSelected, setAccountSelected] = useState(false);
  const [manualToken, setManualToken] = useState("");
  const [cfg, setCfgState] = useState<Config>({});
  const cfgRef = useRef<Config>({});
  const [profile, setProfile] = useState<Profile>({});
  const [selfDeaf, setSelfDeaf] = useState(false);
  const [running, setRunning] = useState(false);
  const [sessions, setSessions] = useState<Session[]>([]);
  const [log, setLog] = useState("");
  const [live, setLive] = useState(false);
  const [msg, setMsg] = useState("");
  const [badgeState, setBadgeState] = useState<BadgeRunState>({ running: false });
  const [badgeLog, setBadgeLog] = useState("");
  const [badgeMsg, setBadgeMsg] = useState("");
  const [quitPrompt, setQuitPrompt] = useState<{ hours: number; games: number } | null>(null);
  const [quitting, setQuitting] = useState(false);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null);
  const [updateError, setUpdateError] = useState("");
  const logBoxRef = useRef<HTMLPreElement | null>(null);

  const setCfg = useCallback((next: Config) => {
    cfgRef.current = next;
    setCfgState(next);
  }, []);

  const selectedPlatform =
    platformAlias[(cfg.PLATFORM || "windows").toLowerCase()] || cfg.PLATFORM || "windows";
  const spoofClient = spoofClientMap[selectedPlatform] || "desktop";
  const indicators = buildPlatformIndicators(sessions, spoofClient);
  const onlineSummary = onlineSummaryOf(indicators);

  const applyStatus = useCallback((status: BotStatus) => {
    setRunning(Boolean(status.running));
    setSessions(status.running ? status.sessions ?? [] : []);
    if (status.user) setProfile((current) => ({ ...current, ...status.user }));
    setLive(true);
  }, []);

  const patchConfig = useCallback(
    async (entries: Config) => {
      setCfg({ ...cfgRef.current, ...entries });
      await invoke("write_env_multi", { entries }).catch(() => undefined);
    },
    [setCfg],
  );

  async function scanTokens(autoSelect = false) {
    setDetecting(true);
    try {
      const report = await invoke<ScanReport>("scan_tokens");
      const accounts = report.accounts ?? [];
      setTokens(accounts);
      setScanNotes(report.notes ?? []);
      if (autoSelect && accounts.length === 1) {
        await selectAccount(accounts[0]);
      }
    } catch (error) {
      setTokens([]);
      setScanNotes([String(error)]);
    } finally {
      setDetecting(false);
    }
  }

  async function selectAccount(account: DetectedAccount) {
    setProfile(account);
    setAccountSelected(true);
    setManualToken("");
    setMsg("");
    setTab("home");
    await patchConfig({ DISCORD_TOKEN: account.token });
  }

  async function useManualToken() {
    const token = manualToken.trim();
    if (!token) return;
    setChecking(true);
    try {
      const me = await invoke<Profile>("account_profile", { token });
      setProfile(me);
      setAccountSelected(true);
      setManualToken("");
      setMsg("");
      setTab("home");
      await patchConfig({ DISCORD_TOKEN: token });
    } catch (error) {
      setMsg(String(error) || "Discord rejected this token");
    } finally {
      setChecking(false);
    }
  }

  async function setPlatform(value: string) {
    await patchConfig({ PLATFORM: value });
  }

  async function save() {
    const next = { ...cfgRef.current, SELF_DEAF: selfDeaf ? "true" : "false" };
    setCfg(next);
    await invoke("write_env_multi", { entries: next }).catch(() => undefined);
    setMsg("Configuration saved");
  }

  async function persistAll() {
    const next = { ...cfgRef.current, SELF_DEAF: selfDeaf ? "true" : "false" };
    setCfg(next);
    await invoke("write_env_multi", { entries: next }).catch(() => undefined);
  }

  async function start() {
    setLog("");
    await persistAll();
    try {
      await invoke("start_bot");
      setMsg("Bot started");
    } catch (error) {
      setMsg(String(error));
    }
  }

  async function restart() {
    setLog("");
    await persistAll();
    try {
      await invoke("restart_bot");
      setMsg("Bot restarted");
    } catch (error) {
      setMsg(String(error));
    }
  }

  async function stop() {
    await invoke("stop_bot").catch((error) => setMsg(String(error)));
    setRunning(false);
    setSessions([]);
    setMsg("Bot stopped");
  }

  async function terminate() {
    await invoke("terminate_bot").catch((error) => setMsg(String(error)));
    setRunning(false);
    setSessions([]);
    setMsg("Bot terminated");
  }

  async function forgeBadges(hours: number, games: number, allowPrompt = true) {
    setBadgeLog("");
    setBadgeMsg("");
    try {
      const token = (cfgRef.current.DISCORD_TOKEN || "").trim();
      await invoke("forge_badges", { token, hours, games });
    } catch (error) {
      const text = String(error);
      if (allowPrompt && text.startsWith("COOKIE_LOCKED:")) {
        setQuitPrompt({ hours, games });
        return;
      }
      setBadgeMsg(text.replace(/^COOKIE_LOCKED:?/, ""));
    }
  }

  async function confirmQuitDiscord() {
    if (!quitPrompt) return;
    const { hours, games } = quitPrompt;
    setQuitting(true);
    try {
      await invoke("quit_discord");
      await new Promise((resolve) => setTimeout(resolve, 1500));
      setQuitPrompt(null);
      await forgeBadges(hours, games, false);
    } catch (error) {
      setQuitPrompt(null);
      setBadgeMsg(String(error));
    } finally {
      setQuitting(false);
    }
  }

  async function downloadUpdate() {
    setUpdateError("");
    setUpdateProgress({ phase: "downloading", received: 0, total: null, percent: 0 });
    try {
      await invoke("download_update");
    } catch (error) {
      setUpdateProgress(null);
      setUpdateError(String(error));
    }
  }

  async function stopBadges() {
    await invoke("stop_badges").catch((error) => setBadgeMsg(String(error)));
  }

  async function switchAccount() {
    setAccountSelected(false);
    setProfile({});
    setManualToken("");
    setMsg("");
    setTokens([]);
    setScanNotes([]);
    setTab("home");
    await scanTokens(false);
  }

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void listen<string>("bot-log", (event) => {
      setLog((previous) => [
        ...previous.split("\n").filter(Boolean),
        event.payload,
      ].slice(-500).join("\n"));
    }).then((unlisten) => (cancelled ? unlisten() : unlisteners.push(unlisten)));

    void listen<BotStatus>("bot-state", (event) => applyStatus(event.payload)).then(
      (unlisten) => {
        cancelled ? unlisten() : unlisteners.push(unlisten);
      },
    );

    void listen<string>("badge-log", (event) => {
      setBadgeLog((previous) => [
        ...previous.split("\n").filter(Boolean),
        event.payload,
      ].slice(-500).join("\n"));
    }).then((unlisten) => (cancelled ? unlisten() : unlisteners.push(unlisten)));

    void listen<BadgeRunState>("badge-state", (event) =>
      setBadgeState(event.payload),
    ).then((unlisten) => (cancelled ? unlisten() : unlisteners.push(unlisten)));

    void listen<UpdateProgress>("update-progress", (event) =>
      setUpdateProgress(event.payload),
    ).then((unlisten) => (cancelled ? unlisten() : unlisteners.push(unlisten)));

    const checkForUpdate = () => {
      void invoke<UpdateInfo>("check_update")
        .then((info) => {
          if (!cancelled && info.available) setUpdate(info);
        })
        .catch(() => undefined);
    };
    const updateKickoff = window.setTimeout(checkForUpdate, 5000);
    const updateTimer = window.setInterval(checkForUpdate, 30 * 60 * 1000);

    void (async () => {
      const config = await invoke<Config>("app_config").catch(() => ({}) as Config);
      if (cancelled) return;
      setCfg(config);
      setSelfDeaf((config.SELF_DEAF || "").toLowerCase() === "true");

      const status = await invoke<BotStatus>("bot_status").catch(() => null);
      if (status && !cancelled) applyStatus(status);

      const badge = await invoke<{ running: boolean }>("badge_status").catch(() => null);
      if (badge && !cancelled) {
        setBadgeState((current) => ({ ...current, running: badge.running }));
      }

      const savedToken = (config.DISCORD_TOKEN || "").trim();
      if (savedToken) {
        setAccountSelected(true);
        try {
          const me = await invoke<Profile>("account_profile", { token: savedToken });
          if (!cancelled) {
            setProfile(me);
            setMsg("");
          }
        } catch (error) {
          if (!cancelled) setMsg(`Saved token could not be verified: ${String(error)}`);
        }
      } else {
        void scanTokens(true);
      }
    })();

    return () => {
      cancelled = true;
      window.clearTimeout(updateKickoff);
      window.clearInterval(updateTimer);
      for (const unlisten of unlisteners) unlisten();
    };
    // Boot once; scanTokens/selectAccount read live state through refs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [applyStatus, setCfg]);

  useEffect(() => {
    const box = logBoxRef.current;
    if (box) box.scrollTop = box.scrollHeight;
  }, [log]);

  function changeTab(next: Tab) {
    setTab(next);
    setMsg("");
  }

  function fieldChanged(key: string, value: string) {
    const next = { ...cfgRef.current, [key]: value };
    cfgRef.current = next;
    setCfgState(next);
  }

  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-background font-sans text-foreground">
      <TitleBar />

      <div className="flex min-h-0 flex-1 gap-3 pr-3 pb-3">
        <AppRail
          tab={tab}
          profile={accountSelected ? profile : null}
          running={running}
          onTab={changeTab}
          onSwitchAccount={switchAccount}
        />

        <section className="app-workspace flex min-h-0 flex-1 flex-col overflow-hidden rounded-[20px] border bg-card">
          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto flex min-h-full w-full max-w-5xl flex-col p-6">
              {update?.available && (
                <UpdateBanner
                  info={update}
                  progress={updateProgress}
                  error={updateError}
                  onDownload={() => void downloadUpdate()}
                />
              )}
              {!accountSelected ? (
                <LoginPanel
                  tokens={tokens}
                  detecting={detecting}
                  notes={scanNotes}
                  manualToken={manualToken}
                  checking={checking}
                  message={msg}
                  onManualToken={setManualToken}
                  onUseManual={useManualToken}
                  onDetect={() => void scanTokens(false)}
                  onSelect={selectAccount}
                />
              ) : tab === "settings" ? (
                <SettingsPage
                  cfg={cfg}
                  selectedPlatform={selectedPlatform}
                  selfDeaf={selfDeaf}
                  msg={msg}
                  onField={fieldChanged}
                  onSelfDeaf={setSelfDeaf}
                  onSetPlatform={setPlatform}
                  onSave={save}
                />
              ) : tab === "badges" ? (
                <BadgesPage
                  cfg={cfg}
                  state={badgeState}
                  log={badgeLog}
                  message={badgeMsg}
                  onForge={forgeBadges}
                  onStop={stopBadges}
                />
              ) : tab === "hypesquad" ? (
                <HypeSquadPage token={(cfg.DISCORD_TOKEN || "").trim()} />
              ) : tab === "logs" ? (                <LogsPage
                  log={log}
                  running={running}
                  live={live}
                  onClear={() => setLog("")}
                  logBoxRef={logBoxRef}
                />
              ) : (
                <Dashboard
                  profile={profile}
                  indicators={indicators}
                  onlineSummary={onlineSummary}
                  running={running}
                  live={live}
                  cfg={cfg}
                  log={log}
                  msg={msg}
                  onField={fieldChanged}
                  onSave={save}
                  onStart={start}
                  onStop={stop}
                  onRestart={restart}
                  onTerminate={terminate}
                  onOpenLogs={() => changeTab("logs")}
                />
              )}
            </div>
          </div>
        </section>
      </div>

      <ConfirmDialog
        open={Boolean(quitPrompt)}
        title="Quit Discord?"
        description="Discord is open and keeps its cookie store locked, so the forger cannot read your session cookie. Discord will be closed and the forge will continue automatically."
        confirmLabel="Quit Discord & forge"
        busy={quitting}
        onConfirm={() => void confirmQuitDiscord()}
        onCancel={() => {
          setQuitPrompt(null);
          setBadgeMsg("Forge cancelled — quit Discord to continue.");
        }}
      />
    </div>
  );
}