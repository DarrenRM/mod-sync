// A sync the app starts by itself (Keep mods in sync) must end with the button back to normal.
// Installs VEIN into a fake Steam library, deletes one file so it needs a repair, opens the real
// app and checks that it repairs on its own, shows progress, and returns to "Play" / "In sync".
//
//   node tools/e2e/auto-sync.mjs [path-to-exe]
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import puppeteer from "puppeteer-core";

const root = resolve(import.meta.dirname, "../..");
const exe = process.argv[2] ?? join(root, "src-tauri/target/debug/mod-sync.exe");
const box = join(tmpdir(), `modsync-autosync-${Date.now()}`);
const steam = join(box, "steam");
const game = join(steam, "steamapps/common/Vein");
const win64 = join(game, "Vein/Binaries/Win64");
mkdirSync(join(steam, "steamapps"), { recursive: true });
writeFileSync(join(steam, "steamapps/libraryfolders.vdf"), `"libraryfolders"\n{\n\t"0"\n\t{\n\t\t"path"\t\t"${steam.replaceAll("\\", "\\\\")}"\n\t}\n}\n`);
writeFileSync(join(steam, "steamapps/appmanifest_1857950.acf"), `"AppState"\n{\n\t"appid"\t\t"1857950"\n\t"name"\t\t"VEIN"\n\t"StateFlags"\t\t"4"\n\t"installdir"\t\t"Vein"\n}\n`);
mkdirSync(win64, { recursive: true });
writeFileSync(join(win64, "Vein-Win64-Test.exe"), "stub");

const data = join(box, "data");
mkdirSync(data, { recursive: true });
// Past the first-launch sheet, with the defaults (Keep mods in sync on).
writeFileSync(join(data, "settings.json"), JSON.stringify({ autoUpdate: true, background: false, shareLogs: false, welcomed: true }));
const env = { ...process.env, MODSYNC_STEAM_PATH: steam, MODSYNC_DATA_DIR: data, MODSYNC_SKIP_RUNNING_CHECK: "1" };

let failures = 0;
const check = (ok, msg) => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${msg}`);
  if (!ok) failures++;
};

execFileSync(exe, ["--cli", "install", "vein", "--out", join(box, "install.txt")], { env, stdio: "ignore" });
const dropped = join(win64, "ue4ss/LICENSE");
rmSync(dropped);

const port = 9336;
const app = spawn(exe, [], { env: { ...env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` }, stdio: "ignore" });
let browser;
try {
  for (let i = 0; i < 60 && !browser; i++) {
    try {
      browser = await puppeteer.connect({ browserURL: `http://127.0.0.1:${port}`, defaultViewport: null });
    } catch {
      await new Promise((r) => setTimeout(r, 250));
    }
  }
  if (!browser) throw new Error("couldn't attach to the app webview");
  let page;
  for (let i = 0; i < 40 && !page; i++) {
    page = (await browser.pages()).find((p) => !p.url().startsWith("devtools"));
    if (!page) await new Promise((r) => setTimeout(r, 100));
  }
  // Record every label the row's button shows, polling from here so a page reload can't lose it.
  const labels = [];
  let busy = true;
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    const b = await page
      .evaluate(() => {
        const el = document.querySelector(".row button.btn");
        return el && { text: el.querySelector(".text")?.textContent.trim() ?? el.textContent.trim(), busy: el.classList.contains("busy") };
      })
      .catch(() => null);
    if (b?.text && labels.at(-1) !== b.text) labels.push(b.text);
    if (b) busy = b.busy;
    if (b?.text === "Play" && !b.busy && labels.some((l) => l.startsWith("Syncing"))) break;
    await new Promise((r) => setTimeout(r, 50));
  }
  const status = await page.evaluate(() => document.querySelector(".row .status")?.textContent.trim());
  mkdirSync(join(root, "build/screens"), { recursive: true });
  await page.screenshot({ path: join(root, "build/screens/auto-sync.png") });
  check(existsSync(dropped), "the app repaired the missing file by itself");
  check(labels.some((l) => /^Syncing \d+%$/.test(l)), `button showed progress while syncing (${labels.join(" → ")})`);
  check(labels.at(-1) === "Play" && !busy, "button back to Play afterwards");
  check(status === "In sync", `status says In sync (${status})`);
} catch (e) {
  check(false, e.message);
} finally {
  await browser?.disconnect().catch(() => {});
  app.kill();
}

console.log(failures ? `\n${failures} failed (sandbox ${box})` : "\nAll passed. Screen in build/screens/auto-sync.png");
if (!failures) {
  await new Promise((r) => setTimeout(r, 1000));
  rmSync(box, { recursive: true, force: true, maxRetries: 5 });
}
process.exit(failures ? 1 : 0);
