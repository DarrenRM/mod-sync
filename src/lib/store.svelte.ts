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
    this.api.onProgress((p) => {
      if (p.phase === "done") this.finish(p.id, true);
      else this.progress[p.id] = p;
    });
    this.api.onState((s) => (this.state = s));
    this.api.onToast((t) => this.toast(t));
    await this.refresh(true);
    if (this.state?.firstRun) {
      this.welcome = true;
      this.sheet = "settings";
    }
  }

  /** First launch: Settings is open with a Done button until the sheet is closed once. */
  welcome = $state(false);

  closeSheet() {
    this.sheet = null;
    if (this.welcome) {
      this.welcome = false;
      this.api.welcomeDone().then((s) => (this.state = s), (e) => this.toast(String(e), "error"));
    }
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

  async act(id: string, fn: (api: Api) => Promise<AppState | void>, phase: Progress["phase"] = "download") {
    if (this.busy(id)) return;
    this.errors[id] = undefined;
    this.progress[id] = { id, phase, done: 0, total: 0 };
    let ok = false;
    try {
      const next = await fn(this.api);
      if (next) this.state = next;
      ok = true;
    } catch (e) {
      this.errors[id] = String(e).replace(/^Error:\s*/, "");
    } finally {
      this.finish(id, ok);
    }
  }

  #finishing: Record<string, ReturnType<typeof setTimeout>> = {};

  /** End a game's progress. A finished sync rests on a full bar for a moment, then the button
   *  goes back to normal. Called when an action returns and when the backend says it's done
   *  (syncs the app starts by itself only get the latter). */
  finish(id: string, ok: boolean) {
    const p = this.progress[id];
    if (!p || this.#finishing[id]) return;
    if (!ok || p.phase === "remove") {
      this.progress[id] = undefined;
      this.refresh();
      return;
    }
    this.progress[id] = { id, phase: "install", done: 1, total: 1 };
    this.#finishing[id] = setTimeout(() => {
      delete this.#finishing[id];
      this.progress[id] = undefined;
      this.refresh();
    }, 500);
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

/** The one obvious next step for a game. The status line says what state the mods are in; the
 *  button says what you can do about it, so the two never repeat each other. */
export function primaryAction(g: GameView, needsAppUpdate: boolean): Action {
  if (g.game === "missing") return { kind: "steam", label: g.owned ? "Install on Steam" : "Get on Steam" };
  if (g.game === "updating") return { kind: "none", label: "Wait for Steam", disabled: true };
  if (g.mods === "current") return g.running ? { kind: "none", label: "Running", disabled: true } : { kind: "play", label: "Play" };
  // Only syncing needs the newer app; playing what's already in sync is fine.
  if (needsAppUpdate) return { kind: "app", label: "Update app" };
  if (g.running) return { kind: "none", label: `Close ${g.name} first`, disabled: true };
  return { kind: g.mods === "none" ? "install" : g.mods === "outdated" ? "update" : "repair", label: "Sync" };
}

type Status = { text: string; tone: "ok" | "new" | "warn" | "dim" };

/** While a game is busy, its status says so instead of the state it's leaving. */
function busyStatus(p: Progress | undefined): Status | undefined {
  return p && { text: p.phase === "remove" ? "Removing…" : "Syncing…", tone: "new" };
}

/** Mods state in a couple of words, for the library row. */
export function statusLine(g: GameView, p?: Progress): Status {
  const busy = busyStatus(p);
  if (busy) return busy;
  if (g.game === "missing") return { text: "Not on this PC", tone: "dim" };
  if (g.game === "updating") return { text: "Steam is updating", tone: "dim" };
  switch (g.mods) {
    case "none":
      return { text: "Not synced", tone: "dim" };
    case "outdated":
    case "broken":
      return { text: "Out of sync", tone: "new" };
    default:
      return { text: "In sync", tone: "ok" };
  }
}

/** The same words as the row, plus the detail that fits in the big header on a game page. */
export function heroStatus(g: GameView, p?: Progress): Status {
  const st = statusLine(g, p);
  if (p) return p.phase === "remove" ? st : { ...st, text: `${st.text}  ·  ${g.latest.version}` };
  if (g.game !== "installed") return st;
  switch (g.mods) {
    case "outdated":
      return { ...st, text: `${st.text}  ·  ${g.installedVersion} → ${g.latest.version}` };
    case "broken":
      return { ...st, text: `${st.text}  ·  files missing` };
    case "current":
      return { ...st, text: `${st.text}  ·  ${g.installedVersion}` };
    default:
      return st;
  }
}

const countGames = (n: number) => `${n} ${n === 1 ? "game" : "games"}`;

/** One line for the whole library, in the rows' words. Busy beats everything, then what needs doing. */
export function librarySummary(state: AppState | null, progress: Record<string, Progress | undefined>): Status {
  if (!state) return { text: "", tone: "dim" };
  const all = state.games;
  const busy = all.filter((g) => progress[g.id]);
  const syncing = busy.filter((g) => progress[g.id]?.phase !== "remove");
  if (syncing.length)
    return { text: syncing.length === 1 ? `Syncing ${syncing[0].name}…` : `Syncing ${countGames(syncing.length)}…`, tone: "new" };
  if (busy.length) return { text: `Removing ${busy[0].name} mods…`, tone: "new" };
  const installed = all.filter((g) => g.game === "installed");
  const outOfSync = installed.filter((g) => g.mods === "outdated" || g.mods === "broken").length;
  const notSynced = installed.filter((g) => g.mods === "none").length;
  if (state.needsAppUpdate && outOfSync + notSynced) return { text: "Update the app to sync", tone: "new" };
  if (outOfSync) return { text: `${countGames(outOfSync)} out of sync`, tone: "new" };
  if (notSynced) return { text: `${countGames(notSynced)} not synced`, tone: "dim" };
  if (installed.length) return { text: "All in sync", tone: "ok" };
  if (all.some((g) => g.game === "updating")) return { text: "Steam is updating", tone: "dim" };
  if (!all.length) return { text: state.offline ? "Offline" : "No games yet", tone: "dim" };
  return { text: "No games on this PC", tone: "dim" };
}

/** One 0..1 bar for a whole sync: downloading fills the first 85%, installing the rest.
 *  null when there's no measurable progress (removing mods). */
export function syncPercent(p: Progress): number | null {
  if (p.phase === "remove" || p.phase === "done") return null;
  const f = p.total ? Math.min(1, p.done / p.total) : 0;
  return p.phase === "download" ? f * 0.85 : 0.85 + f * 0.15;
}

export function progressLabel(p: Progress): string {
  const pct = syncPercent(p);
  return pct === null ? "Removing" : `Syncing ${Math.round(pct * 100)}%`;
}

export function formatSize(bytes: number) {
  return bytes > 1e6 ? `${(bytes / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1e3))} KB`;
}
