// End-to-end install / repair / update / uninstall test with a fake Steam library.
//
//   node tools/e2e/sandbox.mjs [game-id] [--live] [--exe <path>]
//
// Default serves worker/public locally, so a freshly published package is tested BEFORE deploy.
// --live tests what's deployed on the Worker instead.
//
// It builds a fake Steam root + game folder (with a pre-existing file at every top-level package
// path, so backups get exercised), runs the real app headless (--cli), checks every package file
// landed, simulates the game writing logs and a crash dump, repairs a deleted file, updates to a
// synthetic next version, uninstalls, and checks the folder is byte-identical to before.
import { execFile, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { tmpdir } from "node:os";
import { unzipSync, zipSync } from "fflate";

const root = resolve(import.meta.dirname, "../..");
const argv = process.argv.slice(2);
const live = argv.includes("--live");
const exeIdx = argv.indexOf("--exe");
const exe = exeIdx >= 0 ? argv[exeIdx + 1] : join(root, "src-tauri/target/debug/mod-sync.exe");
const id = argv.find((a, i) => !a.startsWith("--") && argv[i - 1] !== "--exe") ?? "vein";
const LIVE_BASE = "https://mod-sync.example.invalid";

let failures = 0;
const check = (ok, msg) => {
  console.log(`${ok ? "PASS" : "FAIL"}  ${msg}`);
  if (!ok) failures++;
};

// --- catalog + package under test ---
const publicDir = join(root, "worker/public");
const catalog = live
  ? await (await fetch(`${LIVE_BASE}/catalog.json?t=${Date.now()}`)).json()
  : JSON.parse(readFileSync(join(publicDir, "catalog.json"), "utf8"));
const entry = catalog.games.find((g) => g.id === id);
if (!entry) throw new Error(`No ${id} in catalog`);
const pkgBytes = live
  ? new Uint8Array(await (await fetch(`${LIVE_BASE}/${entry.package.url}`)).arrayBuffer())
  : readFileSync(join(publicDir, entry.package.url));
const files = Object.fromEntries(Object.entries(unzipSync(pkgBytes)).filter(([n]) => !n.endsWith("/")));
console.log(`${entry.name} ${entry.package.version} (${Object.keys(files).length} files, ${live ? "live" : "local"})\n`);

// Synthetic next version: one file dropped, one added, one changed.
const names = Object.keys(files).sort();
const dropped = names.find((n) => n.includes("/") && !entry.package.preserve?.some((p) => n.endsWith(p.replace("**/", ""))));
const changed = names.find((n) => n !== dropped && n.endsWith(".lua")) ?? names.find((n) => n !== dropped);
const added = `${dirname(changed)}/added_by_sandbox.txt`.replace(/^\.\//, "");
const v2files = { ...files, [added]: new TextEncoder().encode("v2\n"), [changed]: new TextEncoder().encode("-- v2\n") };
delete v2files[dropped];
const v2zip = zipSync(v2files);

// --- local server: the catalog under test, and a "v2" mode for the update step ---
let mode = "v1";
const server = createServer((req, res) => {
  const url = decodeURIComponent(req.url.split("?")[0]);
  if (url === "/catalog.json") {
    const cat = structuredClone(catalog);
    if (mode === "v2") {
      const g = cat.games.find((g) => g.id === id);
      g.package = { ...g.package, version: "99.0.0", url: "sandbox-v2.zip", size: v2zip.length, sha256: createHash("sha256").update(v2zip).digest("hex") };
    }
    return res.end(JSON.stringify(cat));
  }
  if (url === "/sandbox-v2.zip") return res.end(Buffer.from(v2zip));
  if (url === `/${entry.package.url}`) return res.end(Buffer.from(pkgBytes));
  const p = join(publicDir, url);
  if (!live && p.startsWith(publicDir) && existsSync(p)) return res.end(readFileSync(p));
  res.writeHead(404).end();
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;

// --- fake Steam library ---
const box = join(tmpdir(), `modsync-sandbox-${Date.now()}`);
const steam = join(box, "steam");
const data = join(box, "data");
const installdir = entry.name.replace(/[^\w -]/g, "");
const game = join(steam, "steamapps/common", installdir);
const target = join(game, entry.target);
mkdirSync(join(steam, "steamapps"), { recursive: true });
writeFileSync(join(steam, "steamapps/libraryfolders.vdf"), `"libraryfolders"\n{\n\t"0"\n\t{\n\t\t"path"\t\t"${steam.replaceAll("\\", "\\\\")}"\n\t}\n}\n`);
writeFileSync(
  join(steam, `steamapps/appmanifest_${entry.steamAppId}.acf`),
  `"AppState"\n{\n\t"appid"\t\t"${entry.steamAppId}"\n\t"name"\t\t"${entry.name}"\n\t"StateFlags"\t\t"4"\n\t"installdir"\t\t"${installdir}"\n}\n`,
);
mkdirSync(target, { recursive: true });
writeFileSync(join(target, "game-file.bin"), "a game file");
// A pre-existing file where the package puts a top-level file: must be backed up and restored.
const topFile = names.find((n) => !n.includes("/"));
if (topFile) writeFileSync(join(target, topFile), "someone else's file");

function snapshot(dir) {
  const out = {};
  const walk = (d) => {
    for (const e of readdirSync(d, { withFileTypes: true })) {
      const p = join(d, e.name);
      const k = relative(dir, p).replaceAll("\\", "/");
      if (e.isDirectory()) {
        out[`${k}/`] = "dir";
        walk(p);
      } else out[k] = createHash("sha256").update(readFileSync(p)).digest("hex");
    }
  };
  walk(dir);
  return out;
}
const before = snapshot(game);

const env = (extra = {}) => ({ ...process.env, MODSYNC_STEAM_PATH: steam, MODSYNC_DATA_DIR: data, MODSYNC_BASE_URL: base, ...extra });
const cli = (...args) => cliWith({ MODSYNC_SKIP_RUNNING_CHECK: "1" }, ...args);
const cliWith = (extra, ...args) =>
  new Promise((done) => {
    const out = join(box, "cli-out.txt");
    rmSync(out, { force: true });
    execFile(exe, ["--cli", ...args, "--out", out], { env: env(extra), timeout: 180_000 }, () =>
      done(existsSync(out) ? readFileSync(out, "utf8").trim() : "(no output)"),
    );
  });
const status = async () => JSON.parse(await cli("status")).games.find((g) => g.id === id);

try {
  let st = await status();
  check(st?.game === "installed", `game detected (${st?.game})`);
  check(st?.mods === "none", `no mods yet (${st?.mods})`);

  const procs = (entry.processNames ?? []).map((p) => p.toLowerCase());
  const tasks = execFileSync("tasklist", { encoding: "utf8" }).toLowerCase();
  if (procs.some((p) => tasks.includes(p))) {
    check((await cliWith({}, "install", id)).startsWith(`error: Close ${entry.name}`), `refuses while ${entry.name} is running`);
  }

  check((await cli("install", id)) === `installed ${id} ${entry.package.version}`, "install");
  const missing = names.filter((n) => !existsSync(join(target, n)) || Buffer.compare(readFileSync(join(target, n)), Buffer.from(files[n])) !== 0);
  const preservedSkips = missing.filter((n) => entry.package.preserve?.some((p) => n.endsWith(p.replace("**/", ""))));
  check(missing.length === preservedSkips.length, missing.length ? `not in place: ${missing.join(", ")}` : `all ${names.length} files in place`);
  for (const e of entry.package.edits ?? []) {
    check(readFileSync(join(target, e.file), "utf8").includes(e.line), `${e.file} has "${e.line}"`);
  }
  st = await status();
  check(st.mods === "current" && st.installedVersion === entry.package.version, `status current ${st.installedVersion}`);

  // The game runs: loader logs, crash dump, user config.
  for (const d of entry.package.ownedDirs ?? []) {
    writeFileSync(join(target, d, "runtime.log"), "log");
    writeFileSync(join(target, d, "crash_1.dmp"), "dump");
  }
  const cfg = names.map((n) => dirname(n)).find((d) => d.endsWith("scripts"));
  if (cfg) writeFileSync(join(target, cfg, "config_local.lua"), "return { debug = true }\n");

  const victim = names.find((n) => n !== topFile) ?? names[0];
  rmSync(join(target, victim));
  check((await status()).mods === "broken", `deleted ${victim} shows as broken`);
  check((await cli("install", id)).startsWith("installed"), "repair");
  check(existsSync(join(target, victim)), "deleted file restored");

  mode = "v2";
  st = await status();
  check(st.mods === "outdated", `next version shows as outdated (${st.installedVersion} -> ${st.latest.version})`);
  check((await cli("install", id)) === `installed ${id} 99.0.0`, "update");
  check(existsSync(join(target, added)), `added file present`);
  check(!existsSync(join(target, dropped)), `dropped file removed (${dropped})`);
  check(readFileSync(join(target, changed), "utf8") === "-- v2\n", `changed file updated`);
  if (cfg) check(readFileSync(join(target, cfg, "config_local.lua"), "utf8").includes("debug"), "config_local.lua kept");

  check((await cli("uninstall", id)) === `uninstalled ${id}`, "uninstall");
  check((await cli("uninstall", id)) === `uninstalled ${id}`, "second uninstall is a no-op");
  const after = snapshot(game);
  const diff = [...new Set([...Object.keys(before), ...Object.keys(after)])].filter((k) => before[k] !== after[k]);
  check(diff.length === 0, diff.length ? `folder differs: ${diff.join(", ")}` : "game folder byte-identical to before");
  check((await status()).mods === "none", "status back to none");
  check(!existsSync(join(data, `installs/${id}.json`)), "install record removed");
} finally {
  server.close();
}

console.log(failures ? `\n${failures} failed (sandbox kept at ${box})` : "\nAll passed");
if (!failures) rmSync(box, { recursive: true, force: true });
process.exit(failures ? 1 : 0);
