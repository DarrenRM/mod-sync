// When a game isn't installed, the app should offer "Install on Steam" to owners and
// "Get on Steam" to everyone else. Runs the real app twice against a fake Steam folder
// (without and with the game in Steam's library art cache) and reads the button label.
// Never clicks it, so nothing opens in the real Steam.
//
//   node tools/e2e/steam-button.mjs [path-to-exe]
import { spawn, execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import puppeteer from "puppeteer-core";

const root = resolve(import.meta.dirname, "../..");
const exe = process.argv[2] ?? join(root, "src-tauri/target/debug/mod-sync.exe");
const appId = 1857950;
const box = join(tmpdir(), `modsync-steambtn-${Date.now()}`);
const steam = join(box, "steam");
mkdirSync(join(steam, "steamapps"), { recursive: true });
writeFileSync(join(steam, "steamapps/libraryfolders.vdf"), `"libraryfolders"\n{\n\t"0"\n\t{\n\t\t"path"\t\t"${steam.replaceAll("\\", "\\\\")}"\n\t}\n}\n`);

let failures = 0;
const check = (ok, msg) => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${msg}`);
  if (!ok) failures++;
};
const env = (data) => ({ ...process.env, MODSYNC_STEAM_PATH: steam, MODSYNC_DATA_DIR: join(box, data) });

function status(data) {
  const out = join(box, `${data}-status.json`);
  execFileSync(exe, ["--cli", "status", "--out", out], { env: env(data), stdio: "ignore" });
  return JSON.parse(readFileSync(out, "utf8")).games.find((g) => g.id === "vein");
}

async function buttonLabel(data, port) {
  const app = spawn(exe, [], {
    env: { ...env(data), WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}` },
    stdio: "ignore",
  });
  let browser;
  try {
    for (let i = 0; i < 60 && !browser; i++) {
      try {
        browser = await puppeteer.connect({ browserURL: `http://127.0.0.1:${port}`, defaultViewport: null });
      } catch {
        await new Promise((r) => setTimeout(r, 500));
      }
    }
    if (!browser) throw new Error("couldn't attach to the app webview");
    let page;
    for (let i = 0; i < 40 && !page; i++) {
      page = (await browser.pages()).find((p) => !p.url().startsWith("devtools"));
      if (!page) await new Promise((r) => setTimeout(r, 250));
    }
    await page.waitForFunction(() => document.querySelector(".row button.btn"), { timeout: 30_000 });
    await page.click(".row");
    await page.waitForFunction(() => document.body.innerText.toLowerCase().includes("adds"), { timeout: 30_000 });
    await new Promise((r) => setTimeout(r, 900));
    mkdirSync(join(root, "build/screens"), { recursive: true });
    await page.screenshot({ path: join(root, `build/screens/steam-${data}.png`) });
    return await page.evaluate(() => [...document.querySelectorAll("button.btn")].map((b) => b.textContent.trim()).find(Boolean));
  } finally {
    await browser?.disconnect().catch(() => {});
    app.kill();
    await new Promise((r) => setTimeout(r, 800));
  }
}

try {
  let g = status("not-owned");
  check(g.game === "missing" && g.owned === false, `not in art cache: missing, owned=${g.owned}`);
  check((await buttonLabel("not-owned", 9334)) === "Get on Steam", "button says Get on Steam");

  mkdirSync(join(steam, `appcache/librarycache/${appId}/0a1b2c`), { recursive: true });
  writeFileSync(join(steam, `appcache/librarycache/${appId}/0a1b2c/library_600x900.jpg`), "x");
  g = status("owned");
  check(g.game === "missing" && g.owned === true, `in art cache: missing, owned=${g.owned}`);
  check((await buttonLabel("owned", 9335)) === "Install on Steam", "button says Install on Steam");
} catch (e) {
  check(false, e.message);
}

console.log(failures ? `\n${failures} failed (sandbox ${box})` : "\nAll passed. Screens in build/screens/steam-*.png");
if (!failures) rmSync(box, { recursive: true, force: true, maxRetries: 5 });
process.exit(failures ? 1 : 0);
