export type Config = Record<string, string>;

export type Profile = {
  id?: string;
  username?: string;
  global_name?: string | null;
  avatar_url?: string;
};

export type DetectedAccount = Profile & { client: string; token: string };

export type ScanReport = { accounts: DetectedAccount[]; notes: string[] };

export type Session = { client: string; status: string; os?: string };

export type BotStatus = {
  running: boolean;
  pid?: number | null;
  user?: Profile | null;
  sessions?: Session[];
  spoofed_platform?: string | null;
  updated_at?: number | null;
};

export type Tab = "dashboard" | "settings" | "badges" | "hypesquad" | "logs";

export type ProfileBadge = {
  id: string;
  description: string;
  icon_url?: string | null;
};

export type HypeSquadHouse = "bravery" | "brilliance" | "balance";

export type HypeSquadStatus = {
  user_id: string;
  username: string;
  display_name: string;
  avatar_url?: string | null;
  house?: HypeSquadHouse | null;
  flags: number;
  badges: ProfileBadge[];
};

export type BadgeRunState = {
  running: boolean;
  games?: number;
  hours?: number;
  username?: string | null;
  error?: string | null;
};

export type UpdateInfo = {
  available: boolean;
  current: string;
  version?: string | null;
  url?: string | null;
  asset_name?: string | null;
  asset_size?: number | null;
};

export type UpdateProgress = {
  phase: "downloading" | "installing";
  received: number;
  total?: number | null;
  percent: number;
};
