// App self-update: check quietly, download in the background, restart when the user says so.
import { isTauri } from "./api";
import { store } from "./store.svelte";

type Update = { version: string; download(): Promise<void>; install(): Promise<void> };
let pending: Update | null = null;

export async function checkAppUpdate(manual = false): Promise<"none" | "ready" | "error"> {
  if (!isTauri) return "none";
  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) return "none";
    store.appUpdate = { version: update.version, ready: false };
    await update.download();
    pending = update;
    store.appUpdate = { version: update.version, ready: true };
    return "ready";
  } catch (e) {
    if (manual) store.toast(`Update check failed: ${e}`, "error");
    return "error";
  }
}

export async function installAppUpdate() {
  if (!pending) {
    if ((await checkAppUpdate(true)) !== "ready") return;
  }
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await pending!.install();
  await relaunch();
}
