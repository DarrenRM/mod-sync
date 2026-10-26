// Drives the real app window (WebView2 over CDP) through install -> play state -> uninstall on a
// fake Steam library, taking screenshots into build/screens/.
//
//   node tools/e2e/ui.mjs [path-to-exe] [--send-report]
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import puppeteer from "puppeteer-core";

const root = resolve(import.meta.dirname, "../..");
const args = process.argv.slice(2);
const exe = args.find((a) => !a.startsWith("--")) ?? join(root, "src-tauri/target/debug/mod-sync.exe");
const sendReport = args.includes("--send-report");
const shots = join(root, "build/screens");
mkdirSync(shots, { recursive: true });

const box = join(tmpdir(), `modsync-ui-${Date.now()}`);
const steam = join(box, "steam");
const game = join(steam, "steamapps/common/Vein");
const win64 = join(game, "Vein/Binaries/Win64");
mkdirSync(join(steam, "steamapps"), { recursive: true });
writeFileSync(join(steam, "steamapps/libraryfolders.vdf"), `"libraryfolders"\n{\n\t"0"\n\t{\n\t\t"path"\t\t"${steam.replaceAll("\\", "\\\\")}"\n\t}\n}\n`);
writeFileSync(join(steam, "steamapps/appmanifest_1857950.acf"), `"AppState"\n{\n\t"appid"\t\t"1857950"\n\t"name"\t\t"VEIN"\n\t"StateFlags"\t\t"4"\n\t"installdir"\t\t"Vein"\n}\n`);
mkdirSync(win64, { recursive: true });
writeFileSync(join(win64, "Vein-Win64-Test.exe"), "stub");

const snapshot = (dir) => {
  const out = {};
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      if (e.isDirectory()) walk(p);
      else out[relative(dir, p)] = createHash("sha256").update(readFileSync(p)).digest("hex");
    }
  };
  walk(dir);
  return JSON.stringify(out);
};
const before = snapshot(game);

let failures = 0;
const check = (ok, msg) => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${msg}`);
  if (!ok) failures++;
};

const port = 9333;
const app = spawn(exe, [], {
  env: {
    ...process.env,
    MODSYNC_STEAM_PATH: steam,
    MODSYNC_DATA_DIR: join(box, "data"),
    MODSYNC_SKIP_RUNNING_CHECK: "1",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  },
  stdio: "ignore",
});

let browser;
for (let i = 0; i < 60 && !browser; i++) {
  try {
    browser = await puppeteer.connect({ browserURL: `http://127.0.0.1:${port}`, defaultViewport: null });
  } catch {
    await new Promise((r) => setTimeout(r, 500));
  }
}
if (!browser) throw new Error("Couldn't attach to the app webview");

let page;
for (let i = 0; i < 40 && !page; i++) {
  page = (await browser.pages()).find((p) => !p.url().startsWith("devtools"));
  if (!page) await new Promise((r) => setTimeout(r, 250));
}
const shot = (name) => page.screenshot({ path: join(shots, `${name}.png`) });
const text = () => page.evaluate(() => document.body.innerText.toLowerCase());
const clickText = async (selector, label) => {
  const handle = await page.waitForFunction(
    (sel, label) => [...document.querySelectorAll(sel)].find((el) => el.textContent.trim().startsWith(label) && !el.disabled),
    { timeout: 120_000 },
    selector,
    label,
  );
  await handle.asElement().click();
};
const waitText = (t, timeout = 120_000) => page.waitForFunction((t) => document.body.innerText.toLowerCase().includes(t.toLowerCase()), { timeout }, t);

try {
  // First launch: Settings opens once, Start with Windows off, closed with Done.
  await page.waitForFunction(() => [...document.querySelectorAll("button")].some((b) => b.textContent.trim() === "Done"), { timeout: 30_000 });
  await new Promise((r) => setTimeout(r, 900));
  await shot("0-first-launch");
  const toggles = await page.evaluate(() =>
    [...document.querySelectorAll(".sheet [role=switch], .sheet input[type=checkbox]")].map((t) => t.getAttribute("aria-checked") ?? String(t.checked)),
  );
  check(toggles.join(",") === "true,false,true", `first launch opens Settings: on, off, on (${toggles.join(",")})`);
  await clickText("button", "Done");
  await new Promise((r) => setTimeout(r, 700));
  const saved = JSON.parse(readFileSync(join(box, "data/settings.json"), "utf8"));
  check(saved.welcomed === true && saved.background === false, "Done remembers it, Start with Windows stays off");

  await waitText("VEIN", 30_000);
  await new Promise((r) => setTimeout(r, 1200));
  await shot("1-library");
  check(
    await page.evaluate(() => [...document.querySelectorAll(".row button.btn")].some((b) => b.textContent.trim() === "Sync")),
    "library offers Sync",
  );
  check((await text()).includes("not synced"), "library says Not synced");

  await page.click(".row");
  await waitText("Adds");
  await new Promise((r) => setTimeout(r, 900));
  await shot("2-detail-before");
  const t = await text();
  check(t.includes("shared crafting") && t.includes("ue4ss"), "detail lists the mods that will be added");

  await clickText("button.btn", "Sync");
  await new Promise((r) => setTimeout(r, 350));
  await shot("3-syncing");
  await clickText("button.btn", "Play");
  await new Promise((r) => setTimeout(r, 600));
  await shot("4-installed");
  check(existsSync(join(win64, "ue4ss/UE4SS.dll")), "files on disk after Install");
  check((await text()).includes("in sync"), "shows In sync");

  await clickText("button.link", "Uninstall mods");
  await new Promise((r) => setTimeout(r, 400));
  await shot("5-confirm-uninstall");
  await clickText("button.link", "Remove");
  await page.waitForFunction(
    () => [...document.querySelectorAll("button.btn")].some((b) => b.textContent.trim() === "Sync" && !b.disabled),
    { timeout: 60_000 },
  );
  await new Promise((r) => setTimeout(r, 600));
  await shot("6-uninstalled");
  check(snapshot(game) === before, "game folder byte-identical after Uninstall");

  await page.click('button[aria-label="Settings"]');
  await new Promise((r) => setTimeout(r, 700));
  await shot("7-settings");
  await page.keyboard.press("Escape");
  await new Promise((r) => setTimeout(r, 500));

  await clickText("button.link", "Report a problem");
  await new Promise((r) => setTimeout(r, 700));
  await page.type("textarea", "UI test report, safe to delete");
  await shot("8-report");
  if (sendReport) {
    await clickText("button.send", "Send report");
    await waitText("Sent", 30_000);
    check(true, "report sent");
  }
} catch (e) {
  check(false, `UI flow: ${e.message}`);
  await shot("error").catch(() => {});
} finally {
  await browser.disconnect().catch(() => {});
  app.kill();
}

console.log(failures ? `\n${failures} failed (sandbox ${box})` : `\nAll passed. Screens in ${shots}`);
if (!failures) {
  await new Promise((r) => setTimeout(r, 1000)); // let the app release its files
  rmSync(box, { recursive: true, force: true, maxRetries: 5 });
}
process.exit(failures ? 1 : 0);
