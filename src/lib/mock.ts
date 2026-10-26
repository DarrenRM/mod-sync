// Browser-only stand-in for the Rust backend, so the UI can be designed and tested in a plain browser.
import type { Api } from "./api";
import type { AppState, GameView, Progress } from "./types";

const veinMods = [
  { name: "Shared Crafting", version: "0.5.0", summary: "Craft from your base's storage" },
  { name: "UE4SS", version: "3.0.1", summary: "Script loader" },
];

const s: AppState = {
  appVersion: "0.1.0",
  offline: false,
  needsAppUpdate: false,
  settings: { autoUpdate: true, background: false, shareLogs: true },
  games: [
    {
      id: "vein",
      name: "VEIN",
      steamAppId: 1857950,
      game: "installed",
      owned: true,
      path: "F:\\SteamLibrary\\steamapps\\common\\Vein",
      running: false,
      mods: "outdated",
      installedVersion: "0.3.0",
      latest: {
        version: "0.5.0",
        released: "2026-10-26",
        size: 6_412_000,
        mods: veinMods,
        notes: [
          "Water from bottles, tanks, taps and the ground",
          "Recipes you can't make are dimmed",
          "Tooltips name where each ingredient comes from",
        ],
      },
    },
    {
      id: "abiotic",
      name: "Abiotic Factor",
      steamAppId: 427410,
      game: "missing",
      owned: false,
      path: null,
      running: false,
      mods: "none",
      installedVersion: null,
      latest: {
        version: "1.2.0",
        released: "2026-10-12",
        size: 2_100_000,
        mods: [{ name: "Stack Pack", version: "1.2.0", summary: "Bigger stacks, faster benches" }],
        notes: [],
      },
    },
  ],
};

const progressCbs = new Set<(p: Progress) => void>();
const toastCbs = new Set<(t: string) => void>();
const clone = () => structuredClone(s);
const game = (id: string) => s.games.find((g) => g.id === id) as GameView;
const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function run(id: string, phase: Progress["phase"], steps: number, ms: number) {
  for (let i = 0; i <= steps; i++) {
    progressCbs.forEach((cb) => cb({ id, phase, done: i, total: steps }));
    await wait(ms);
  }
}

export const mock: Api = {
  async state() {
    await wait(250);
    return clone();
  },
  async install(id) {
    await run(id, "download", 40, 30);
    await run(id, "install", 20, 25);
    const g = game(id);
    g.mods = "current";
    g.installedVersion = g.latest.version;
    toastCbs.forEach((cb) => cb(`${g.name} synced  ·  ${g.latest.version}`));
    return clone();
  },
  async uninstall(id) {
    await run(id, "remove", 15, 30);
    const g = game(id);
    g.mods = "none";
    g.installedVersion = null;
    toastCbs.forEach((cb) => cb(`${g.name} is back to vanilla`));
    return clone();
  },
  async play(id) {
    game(id).running = true;
    toastCbs.forEach((cb) => cb(`Launching ${game(id).name}`));
  },
  async getOnSteam() {},
  async locate() {
    return clone();
  },
  async openFolder() {},
  async art(id) {
    const g = game(id);
    const cdn = `https://cdn.cloudflare.steamstatic.com/steam/apps/${g.steamAppId}`;
    return id === "vein" ? { hero: `${cdn}/library_hero.jpg`, logo: `${cdn}/logo.png` } : {};
  },
  async report() {
    await wait(900);
  },
  async setSettings(settings) {
    s.settings = settings;
    return clone();
  },
  async onProgress(cb) {
    progressCbs.add(cb);
    return () => progressCbs.delete(cb);
  },
  async onState() {
    return () => {};
  },
  async onToast(cb) {
    toastCbs.add(cb);
    return () => toastCbs.delete(cb);
  },
  window: { minimize() {}, close() {}, drag() {} },
};
