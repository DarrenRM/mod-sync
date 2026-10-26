// Stages the VEIN client package: UE4SS (VEIN-tuned) + every client mod in the mod repo's
// VeinMods client bundle (SharedCraftingClient, QuickMoveClient, ...).
// Files land relative to Vein/Binaries/Win64.
import { execFileSync } from "node:child_process";
import { readFileSync, mkdirSync, writeFileSync, cpSync, readdirSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { unzipSync } from "fflate";

const here = dirname(fileURLToPath(import.meta.url));

function put(stage, rel, bytes) {
  const dest = join(stage, rel);
  mkdirSync(dirname(dest), { recursive: true });
  writeFileSync(dest, bytes);
}

export async function build({ mod, stage, work }) {
  const repo = mod.source.repo;

  // 1. The client mods, built by the mod repo's own release script (into our work dir, not its
  //    dist/). The VeinMods client bundle holds every product's client mod folder.
  const versionLua = readFileSync(join(repo, "src/lua/vein_core/version.lua"), "utf8");
  const modVersion = versionLua.match(/^\s*mod\s*=\s*"([^"]+)"/m)[1];
  const out = join(work, "vein-release");
  mkdirSync(out, { recursive: true });
  execFileSync(
    "powershell.exe",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", join(repo, "tools/release/Build-VeinModPackages.ps1"), "-OutDir", out],
    { stdio: ["ignore", "inherit", "inherit"] },
  );
  const bundles = readdirSync(out).filter((f) => /^VeinMods-client-.+\.zip$/.test(f));
  if (bundles.length !== 1) throw new Error(`Expected one VeinMods-client zip, found: ${bundles.join(", ") || "none"}`);
  const client = unzipSync(readFileSync(join(out, bundles[0])));

  // Product versions from the bundle's package.psd1, for {version:<Product>} in mod.json.
  const psd1 = Object.entries(client).find(([name]) => /^[^/]+\/package\.psd1$/.test(name));
  if (!psd1) throw new Error("No package.psd1 in the client bundle");
  const versions = {};
  for (const m of new TextDecoder().decode(psd1[1]).matchAll(/Name = '([^']+)'; Version = '([^']+)'/g)) versions[m[1]] = m[2];

  // Mod folders only (<bundle>/<Mod>/scripts/...); the bundle's own install scripts and README stay out.
  const modDirs = new Set();
  for (const [name, bytes] of Object.entries(client)) {
    const m = name.match(/^[^/]+\/(([^/]+)\/scripts\/.+[^/])$/);
    if (m) {
      put(stage, `ue4ss/Mods/${m[1]}`, bytes);
      modDirs.add(m[2]);
    }
  }
  // Every shipped mod needs its mods.txt edit in mod.json, and every edit needs its mod.
  const enabled = mod.edits.filter((e) => e.file === "ue4ss/Mods/mods.txt").map((e) => e.key);
  for (const key of enabled) if (!modDirs.has(key)) throw new Error(`mod.json enables ${key}, but the client bundle has no ${key}`);
  for (const dir of modDirs) if (!enabled.includes(dir)) throw new Error(`The client bundle has ${dir}; add its mods.txt edit to mod.json`);

  // 2. UE4SS runtime: the loader, its shared Lua libs and the built-in Keybinds mod. No PDBs or docs.
  const ue4ss = unzipSync(readFileSync(mod.source.ue4ssZip));
  const keep = [/^dwmapi\.dll$/, /^ue4ss\/UE4SS\.dll$/, /^ue4ss\/LICENSE$/, /^ue4ss\/Mods\/shared\/.+[^/]$/, /^ue4ss\/Mods\/Keybinds\/.+[^/]$/];
  for (const [name, bytes] of Object.entries(ue4ss)) {
    if (keep.some((re) => re.test(name))) put(stage, name, bytes);
  }

  // 3. VEIN-specific settings, signatures, layouts and a starting mods.txt (from a working install).
  cpSync(join(here, "layer"), stage, { recursive: true });

  return { modVersion, versions };
}
