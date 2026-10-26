import type { AppState, Art, Progress, Settings } from "./types";
import { mock } from "./mock";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Unlisten = () => void;

export interface Api {
  state(refresh?: boolean): Promise<AppState>;
  install(id: string): Promise<AppState>;
  uninstall(id: string): Promise<AppState>;
  play(id: string): Promise<void>;
  getOnSteam(id: string): Promise<void>;
  locate(id: string): Promise<AppState>;
  openFolder(id: string): Promise<void>;
  report(id: string | null, note: string): Promise<void>;
  art(id: string): Promise<Art>;
  setSettings(s: Settings): Promise<AppState>;
  welcomeDone(): Promise<AppState>;
  onProgress(cb: (p: Progress) => void): Promise<Unlisten>;
  onState(cb: (s: AppState) => void): Promise<Unlisten>;
  onToast(cb: (text: string) => void): Promise<Unlisten>;
  window: { minimize(): void; close(): void; drag(): void };
}

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const { open } = await import("@tauri-apps/plugin-dialog");
  const win = getCurrentWindow();
  return {
    state: (refresh = false) => invoke("get_state", { refresh }),
    install: (id) => invoke("install", { id }),
    uninstall: (id) => invoke("uninstall", { id }),
    play: (id) => invoke("play", { id }),
    getOnSteam: (id) => invoke("get_on_steam", { id }),
    locate: async (id) => {
      const dir = await open({ directory: true, title: "Game folder" });
      return invoke("locate", { id, path: typeof dir === "string" ? dir : null });
    },
    openFolder: (id) => invoke("open_folder", { id }),
    report: (id, note) => invoke("send_report", { id, note }),
    art: (id) => invoke("get_art", { id }),
    setSettings: (settings) => invoke("set_settings", { settings }),
    welcomeDone: () => invoke("welcome_done"),
    onProgress: (cb) => listen<Progress>("progress", (e) => cb(e.payload)),
    onState: (cb) => listen<AppState>("state", (e) => cb(e.payload)),
    onToast: (cb) => listen<string>("toast", (e) => cb(e.payload)),
    window: {
      minimize: () => void win.minimize(),
      close: () => void win.close(),
      drag: () => void win.startDragging(),
    },
  };
}

export const api: Promise<Api> = isTauri ? tauriApi() : Promise.resolve(mock);
