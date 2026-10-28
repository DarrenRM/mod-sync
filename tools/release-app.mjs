// Build a signed app release and put it where the Worker serves it. Deploy afterwards.
//
//   node tools/release-app.mjs 0.2.0 "What changed"     set version, build, sign, copy
//   node tools/release-app.mjs                          rebuild the current version
//
// Needs .keys/updater.key (back it up: without it, installed apps can't be updated).
import { execSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const [version, notes = ""] = process.argv.slice(2);
const pkgPath = join(root, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));

if (version) {
  pkg.version = version;
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
  const cargoPath = join(root, "src-tauri", "Cargo.toml");
  writeFileSync(cargoPath, readFileSync(cargoPath, "utf8").replace(/^version = ".*"$/m, `version = "${version}"`));
}
const v = pkg.version;

// The Worker URL is not stored in the repo (see .env.example). tauri.conf.json carries a
// placeholder updater endpoint; the real one is merged in for this build only.
const base = (process.env.MODSYNC_BASE_URL ?? "").replace(/\/+$/, "");
if (!base) throw new Error("Set MODSYNC_BASE_URL to the Worker URL first (see .env.example)");
const configOverride = join(root, "build", "tauri.release.conf.json");
mkdirSync(join(root, "build"), { recursive: true });
writeFileSync(configOverride, JSON.stringify({ plugins: { updater: { endpoints: [`${base}/app/latest.json`] } } }));

const keyPath = join(root, ".keys", "updater.key");
if (!existsSync(keyPath)) throw new Error("Missing .keys/updater.key");

execSync(`npx tauri build --config "${configOverride}"`, {
  cwd: root,
  stdio: "inherit",
  env: {
    ...process.env,
    MODSYNC_BASE_URL: base,
    TAURI_SIGNING_PRIVATE_KEY: readFileSync(keyPath, "utf8"),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "",
  },
});

const nsis = join(root, "src-tauri", "target", "release", "bundle", "nsis");
const exe = readdirSync(nsis).find((f) => f.endsWith(`_${v}_x64-setup.exe`));
if (!exe) throw new Error(`No installer for ${v} in ${nsis}`);
const sig = readFileSync(join(nsis, `${exe}.sig`), "utf8").trim();

const out = join(root, "worker", "public", "app");
mkdirSync(out, { recursive: true });
const versioned = `ModSync_${v}_x64-setup.exe`;
copyFileSync(join(nsis, exe), join(out, versioned));
copyFileSync(join(nsis, exe), join(out, "ModSync-setup.exe")); // stable link for the landing page

const latest = {
  version: v,
  notes,
  pub_date: new Date().toISOString(),
  platforms: { "windows-x86_64": { signature: sig, url: `${base}/app/${versioned}` } },
};
writeFileSync(join(out, "latest.json"), JSON.stringify(latest, null, 2) + "\n");

// Keep only the two newest versioned installers to stay well under asset limits.
const olds = readdirSync(out)
  .filter((f) => /^ModSync_.+_x64-setup\.exe$/.test(f) && f !== versioned)
  .sort()
  .slice(0, -1);
for (const f of olds) execSync(`node -e "require('fs').unlinkSync(process.argv[1])" "${join(out, f)}"`);

console.log(`\nApp ${v} ready in worker/public/app. Next: npm run deploy`);
