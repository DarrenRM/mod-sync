// Stages the VEIN client package: UE4SS (VEIN-tuned) + SharedCraftingClient.
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

  // 1. SharedCraftingClient, built by the mod repo's own release script (into our work dir, not its dist/).
  const versionLua = readFileSync(join(repo, "src/lua/vein_core/version.lua"), "utf8");
  const modVersion = versionLua.match(/^\s*mod\s*=\s*"([^"]+)"/m)[1];
  const out = join(work, "vein-release");
  mkdirSync(out, { recursive: true });
  execFileSync(
    "powershell.exe",
    ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", join(repo, "tools/release/Build-SharedCraftingPackage.ps1"), "-OutDir", out],
    { stdio: ["ignore", "inherit", "inherit"] },
  );
  const client = unzipSync(readFileSync(join(out, `SharedCrafting-client-${modVersion}.zip`)));
  let modFiles = 0;
  for (const [name, bytes] of Object.entries(client)) {
    const m = name.match(/^[^/]+\/(SharedCraftingClient\/.+[^/])$/);
    if (m) {
      put(stage, `ue4ss/Mods/${m[1]}`, bytes);
      modFiles++;
    }
  }
  if (!modFiles) throw new Error("No SharedCraftingClient files in the client zip");

  // 2. UE4SS runtime: the loader, its shared Lua libs and the built-in Keybinds mod. No PDBs or docs.
  const ue4ss = unzipSync(readFileSync(mod.source.ue4ssZip));
  const keep = [/^dwmapi\.dll$/, /^ue4ss\/UE4SS\.dll$/, /^ue4ss\/LICENSE$/, /^ue4ss\/Mods\/shared\/.+[^/]$/, /^ue4ss\/Mods\/Keybinds\/.+[^/]$/];
  for (const [name, bytes] of Object.entries(ue4ss)) {
    if (keep.some((re) => re.test(name))) put(stage, name, bytes);
  }

  // 3. VEIN-specific settings, signatures, layouts and a starting mods.txt (from a working install).
  cpSync(join(here, "layer"), stage, { recursive: true });

  return { modVersion };
}
