import { api, type Api } from "./api";
import type { AppState, Art, GameView, Progress } from "./types";

type Toast = { id: number; text: string; tone: "ok" | "error" };

class Store {
  state = $state<AppState | null>(null);
  view = $state<{ name: "library" } | { name: "game"; id: string }>({ name: "library" });
  progress = $state<Record<string, Progress | undefined>>({});
  errors = $state<Record<string, string | undefined>>({});
  toasts = $state<Toast[]>([]);
  sheet = $state<null | "settings" | { report: string | null }>(null);
  appUpdate = $state<null | { version: string; ready: boolean }>(null);
  art = $state<Record<string, Art>>({});
  api!: Api;
  #toastId = 0;

  async init() {
    this.api = await api;
    this.api.onProgress((p) => (this.progress[p.id] = p));
    this.api.onState((s) => (this.state = s));
    this.api.onToast((t) => this.toast(t));
    await this.refresh(true);
  }

  async refresh(remote = false) {
    try {
      this.state = await this.api.state(remote);
      for (const g of this.state.games) if (!(g.id in this.art)) this.loadArt(g.id);
    } catch (e) {
      this.toast(String(e), "error");
    }
  }

  async loadArt(id: string) {
    this.art[id] = {};
    try {
      this.art[id] = await this.api.art(id);
    } catch {}
  }

  game(id: string): GameView | undefined {
    return this.state?.games.find((g) => g.id === id);
  }

  get visibleGames() {
    return this.state?.games ?? [];
  }

  busy(id: string) {
    return !!this.progress[id];
  }

  async act(id: string, fn: (api: Api) => Promise<AppState | void>) {
    if (this.busy(id)) return;
    this.errors[id] = undefined;
    this.progress[id] = { id, phase: "download", done: 0, total: 0 };
    try {
      const next = await fn(this.api);
      if (next) this.state = next;
    } catch (e) {
      this.errors[id] = String(e).replace(/^Error:\s*/, "");
    } finally {
      this.progress[id] = undefined;
    }
  }

  toast(text: string, tone: Toast["tone"] = "ok") {
    const id = ++this.#toastId;
    this.toasts.push({ id, text, tone });
    setTimeout(() => (this.toasts = this.toasts.filter((t) => t.id !== id)), 3800);
  }
}

export const store = new Store();

export type Action =
  | { kind: "install" | "update" | "repair" | "play" | "steam" | "app"; label: string; disabled?: false }
  | { kind: "none"; label: string; disabled: true };

/** The one obvious next step for a game. */
export function primaryAction(g: GameView, needsAppUpdate: boolean): Action {
  if (needsAppUpdate) return { kind: "app", label: "Update app" };
  if (g.game === "missing") return { kind: "steam", label: g.owned ? "Install on Steam" : "Get on Steam" };
  if (g.game === "updating") return { kind: "none", label: "Steam is updating", disabled: true };
  if (g.running && g.mods !== "current") return { kind: "none", label: `Close ${g.name} first`, disabled: true };
  switch (g.mods) {
    case "none":
      return { kind: "install", label: "Sync" };
    case "outdated":
      return { kind: "update", label: "Sync" };
    case "broken":
      return { kind: "repair", label: "Sync" };
    default:
      return g.running ? { kind: "none", label: "Running", disabled: true } : { kind: "play", label: "Play" };
  }
}

export function statusLine(g: GameView): { text: string; tone: "ok" | "new" | "warn" | "dim" } {
  if (g.game === "missing") return { text: "Not on this PC", tone: "dim" };
  if (g.game === "updating") return { text: "Steam is updating", tone: "dim" };
  switch (g.mods) {
    case "none":
      return { text: "Not synced", tone: "dim" };
    case "outdated":
      return { text: "Out of sync", tone: "new" };
    case "broken":
      return { text: "Files missing", tone: "warn" };
    default:
      return { text: g.running ? "Running" : "In sync", tone: "ok" };
  }
}

/** Fuller wording for the big header on a game page. */
export function heroStatus(g: GameView): { text: string; tone: "ok" | "new" | "warn" | "dim" } {
  const st = statusLine(g);
  if (g.game === "installed" && g.mods === "outdated")
    return { text: `${st.text}  ·  ${g.installedVersion} → ${g.latest.version}`, tone: "new" };
  if (g.game === "installed" && g.mods === "current") return { text: `${st.text}  ·  ${g.installedVersion}`, tone: "ok" };
  return st;
}

export function progressLabel(p: Progress): string {
  const pct = p.total ? Math.round((p.done / p.total) * 100) : null;
  const verb = p.phase === "download" ? "Downloading" : p.phase === "install" ? "Syncing" : "Removing";
  return pct === null || p.phase !== "download" ? verb : `${verb} ${pct}%`;
}

export function formatSize(bytes: number) {
  return bytes > 1e6 ? `${(bytes / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}
