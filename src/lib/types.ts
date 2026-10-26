export type ModInfo = { name: string; version: string; summary: string };

export type PackageInfo = {
  version: string;
  released: string;
  size: number;
  mods: ModInfo[];
  notes: string[];
};

export type GameView = {
  id: string;
  name: string;
  steamAppId: number;
  /** missing: not found on this PC · updating: Steam is mid-update · installed */
  game: "installed" | "missing" | "updating";
  /** Owns it on Steam (best guess). Picks "Install on Steam" vs "Get on Steam" when missing. */
  owned: boolean;
  path: string | null;
  running: boolean;
  /** none · current · outdated · broken (files missing) */
  mods: "none" | "current" | "outdated" | "broken";
  installedVersion: string | null;
  latest: PackageInfo;
};

export type Settings = {
  autoUpdate: boolean;
  background: boolean;
  shareLogs: boolean;
};

export type AppState = {
  appVersion: string;
  offline: boolean;
  needsAppUpdate: boolean;
  settings: Settings;
  games: GameView[];
};

export type Art = { hero?: string | null; logo?: string | null };

export type Progress = {
  id: string;
  phase: "download" | "install" | "remove";
  done: number;
  total: number;
};
