import type { Session } from "../types";

export type PlatformKey = "desktop" | "web" | "mobile" | "embedded" | "vr";

export type PlatformMeta = { label: string; tooltip: string; paths: string[] };

export const platformMeta: Record<PlatformKey, PlatformMeta> = {
  desktop: {
    label: "Desktop",
    tooltip: "Desktop",
    paths: ["M4 4.8h16v10.4H4z", "M12 15.2v4.2M9.5 19.4h5"],
  },
  web: {
    label: "Web",
    tooltip: "Web",
    paths: [
      "M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17Z",
      "M3.5 12h17",
      "M12 3.5c2.4 2.3 3.6 5.1 3.6 8.5s-1.2 6.2-3.6 8.5c-2.4-2.3-3.6-5.1-3.6-8.5S9.6 5.8 12 3.5Z",
    ],
  },
  mobile: {
    label: "Mobile",
    tooltip: "Mobile",
    paths: [
      "M9.6 2.6h4.8a2.6 2.6 0 0 1 2.6 2.6v13.6a2.6 2.6 0 0 1-2.6 2.6H9.6A2.6 2.6 0 0 1 7 18.8V5.2a2.6 2.6 0 0 1 2.6-2.6Z",
      "M10.8 18.2h2.4",
    ],
  },
  embedded: {
    label: "Embedded",
    tooltip: "Embedded (console or game)",
    paths: [
      "M7.4 7h9.2a4.6 4.6 0 0 1 4.6 4.6v1.4a3.9 3.9 0 0 1-6.6 2.8l-1.1-1.2h-3l-1.1 1.2A3.9 3.9 0 0 1 2.8 13v-1.4A4.6 4.6 0 0 1 7.4 7Z",
      "M8.2 10.2v3.2M6.6 11.8h3.2",
      "M15.6 11.2h.1",
      "M17.9 13.3h.1",
    ],
  },
  vr: {
    label: "VR",
    tooltip: "VR",
    paths: [
      "M4.8 8.6h14.4a2.3 2.3 0 0 1 2.3 2.3v2.2a2.3 2.3 0 0 1-2.3 2.3H11.1l-1.5-1.7H4.8a2.3 2.3 0 0 1-2.3-2.3v-2.2a2.3 2.3 0 0 1 2.3-2.3Z",
    ],
  },
};

export type SpoofGroup = {
  key: PlatformKey;
  label: string;
  options: Array<{ value: string; label: string }>;
};

export const spoofGroups: SpoofGroup[] = [
  {
    key: "desktop",
    label: "Desktop",
    options: [
      { value: "windows", label: "Windows" },
      { value: "mac", label: "macOS" },
      { value: "linux", label: "Linux" },
    ],
  },
  { key: "web", label: "Web", options: [{ value: "web", label: "Web" }] },
  {
    key: "mobile",
    label: "Mobile",
    options: [
      { value: "android", label: "Android" },
      { value: "ios", label: "iOS" },
    ],
  },
  {
    key: "embedded",
    label: "Console",
    options: [
      { value: "xbox", label: "Xbox" },
      { value: "playstation", label: "PlayStation" },
    ],
  },
  { key: "vr", label: "VR", options: [{ value: "vr", label: "VR" }] },
];

const statusColours: Record<string, string> = {
  online: "#23a55a",
  idle: "#f0b232",
  dnd: "#f23f43",
  offline: "#80848e",
  invisible: "#80848e",
};

export const platformAlias: Record<string, string> = {
  desktop: "windows",
  macos: "mac",
  mobile: "ios",
};

export const spoofClientMap: Record<string, string> = {
  windows: "desktop",
  mac: "desktop",
  linux: "desktop",
  web: "web",
  android: "mobile",
  ios: "mobile",
  xbox: "embedded",
  playstation: "embedded",
  vr: "vr",
};

const osOrder: PlatformKey[] = ["desktop", "web", "mobile", "embedded", "vr"];
const osRank: Record<string, number> = { online: 0, idle: 1, dnd: 2, offline: 3, invisible: 3 };

export type PlatformIndicator = {
  key: string;
  label: string;
  paths: string[];
  online: boolean;
  color: string;
  title: string;
};

export function buildPlatformIndicators(
  sessions: Session[],
  spoofClient: string,
): PlatformIndicator[] {
  const merged: Record<string, { status: string; os: string[] }> = {};
  for (const session of sessions) {
    const client = session.client;
    if (!osOrder.includes(client as PlatformKey)) continue;
    const status = session.status in osRank ? session.status : "offline";
    const entry = merged[client] || (merged[client] = { status, os: [] });
    if (osRank[status] < osRank[entry.status]) entry.status = status;
    if (session.os && session.os !== "unknown" && !entry.os.includes(session.os)) {
      entry.os.push(session.os);
    }
  }

  const listFor = (client: string, list: string[]) => {
    if (client === "embedded" || client === "vr") return [];
    if (client === "mobile") return list.filter((os) => os === "ios" || os === "android");
    return list;
  };

  const indicators: PlatformIndicator[] = [];
  for (const key of osOrder) {
    const entry = merged[key];
    if (!entry) continue;
    const meta = platformMeta[key];
    const badge = entry.status === "invisible" ? "offline" : entry.status;
    const os = listFor(key, entry.os);
    indicators.push({
      key,
      label: meta.label,
      paths: meta.paths,
      online: true,
      color: statusColours[entry.status] || "#80848e",
      title: `${meta.tooltip}${os.length ? " · " + os.join(", ") : ""} · ${badge}`,
    });
  }

  if (indicators.length === 0 && spoofClient) {
    const key = (spoofClient as PlatformKey) in platformMeta ? (spoofClient as PlatformKey) : "desktop";
    const meta = platformMeta[key];
    indicators.push({
      key: "spoof",
      label: meta.label,
      paths: meta.paths,
      online: false,
      color: "#80848e",
      title: `Spoofed as ${meta.tooltip} · not online`,
    });
  }

  return indicators;
}

export function onlineSummaryOf(indicators: PlatformIndicator[]): string {
  return indicators
    .filter((indicator) => indicator.key !== "spoof")
    .map((indicator) => indicator.label)
    .join(", ");
}
