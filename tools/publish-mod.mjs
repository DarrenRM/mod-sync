// Build a game's mod package and put it in the catalog. Deploy afterwards with `npm run deploy`.
//
//   node tools/publish-mod.mjs vein            build + add to worker/public
//   node tools/publish-mod.mjs vein --force    replace a published version whose contents changed
//
// The package version is the mod's version, plus "-r<revision>" when mod.json has revision > 0
// (bump revision to ship a packaging-only change).
import { createHash } from "node:crypto";
import { execSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { zipSync } from "fflate";
import sharp from "sharp";

const root = resolve(import.meta.dirname, "..");
const [id, ...flags] = process.argv.slice(2);
if (!id) {
  console.error("usage: node tools/publish-mod.mjs <game-id> [--force]");
  process.exit(1);
}
const force = flags.includes("--force");
const modDir = join(root, "mods", id);
const mod = JSON.parse(readFileSync(join(modDir, "mod.json"), "utf8"));

const work = join(root, "build", id);
const stage = join(work, "stage");
rmSync(work, { recursive: true, force: true });
mkdirSync(stage, { recursive: true });

const { build } = await import(pathToFileURL(join(modDir, "build.mjs")).href);
const { modVersion } = await build({ mod, stage, work });
const version = mod.revision > 0 ? `${modVersion}-r${mod.revision}` : modVersion;

// Deterministic zip: sorted entries, fixed timestamps, so the same files give the same hash.
function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}
const files = walk(stage).sort();
const entries = {};
for (const f of files) {
  entries[relative(stage, f).replaceAll("\\", "/")] = [readFileSync(f), { level: 9, mtime: new Date(1980, 0, 1) }];
}
const zip = zipSync(entries);
const sha256 = createHash("sha256").update(zip).digest("hex");

const pkgRel = `packages/${id}/${id}-${version}.zip`;
const pkgPath = join(root, "worker", "public", pkgRel);
const catalogPath = join(root, "worker", "public", "catalog.json");
const catalog = existsSync(catalogPath) ? JSON.parse(readFileSync(catalogPath, "utf8")) : { schema: 1, minApp: "0.1.0", games: [] };
const prev = catalog.games.find((g) => g.id === id);

if (prev?.package.version === version && prev.package.sha256 !== sha256 && !force) {
  console.error(`\n${id} ${version} is already published with different contents.`);
  console.error(`Bump the mod version (or "revision" in mods/${id}/mod.json), or pass --force.`);
  process.exit(1);
}

mkdirSync(join(pkgPath, ".."), { recursive: true });
writeFileSync(pkgPath, zip);

// Game art from the local Steam library cache, else Steam's CDN. Shown on the game's page.
async function steamArt(appId, name) {
  try {
    const out = execSync('reg query "HKCU\\Software\\Valve\\Steam" /v SteamPath', { encoding: "utf8" });
    const steamRoot = out.match(/SteamPath\s+REG_SZ\s+(.+)/)?.[1].trim();
    const dir = join(steamRoot, "appcache", "librarycache", String(appId));
    const candidates = [join(dir, name), ...readdirSync(dir, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => join(dir, e.name, name))];
    const hit = candidates.find((p) => existsSync(p));
    if (hit) return readFileSync(hit);
  } catch {}
  const res = await fetch(`https://cdn.cloudflare.steamstatic.com/steam/apps/${appId}/${name}`);
  return res.ok ? Buffer.from(await res.arrayBuffer()) : null;
}
const art = {};
for (const [kind, name, file] of [["hero", "library_hero.jpg", "hero.jpg"], ["logo", "logo.png", "logo.png"]]) {
  let bytes = await steamArt(mod.steamAppId, name);
  if (!bytes) continue;
  // Trim transparent padding so logos line up with the page's left edge.
  if (kind === "logo") bytes = await sharp(bytes).trim().png().toBuffer();
  const rel = `games/${id}/${file}`;
  mkdirSync(join(root, "worker", "public", "games", id), { recursive: true });
  writeFileSync(join(root, "worker", "public", rel), bytes);
  art[kind] = `${rel}?v=${createHash("sha256").update(bytes).digest("hex").slice(0, 8)}`;
}

const subst = (s) => s.replaceAll("{modVersion}", modVersion);
const entry = {
  id: mod.id,
  name: mod.name,
  steamAppId: mod.steamAppId,
  hideIfMissing: !!mod.hideIfMissing,
  target: mod.target,
  processNames: mod.processNames,
  logs: mod.logs,
  crashGlobs: mod.crashGlobs,
  art,
  package: {
    version,
    url: pkgRel,
    sha256,
    size: zip.length,
    released: prev?.package.version === version ? prev.package.released : new Date().toISOString().slice(0, 10),
    mods: mod.mods.map((m) => ({ ...m, version: subst(m.version) })),
    notes: mod.notes,
    preserve: mod.preserve,
    ownedDirs: mod.ownedDirs,
    edits: mod.edits,
  },
};
catalog.games = [...catalog.games.filter((g) => g.id !== id), entry];
writeFileSync(catalogPath, JSON.stringify(catalog, null, 2) + "\n");

console.log(`\n${mod.name} ${version}`);
console.log(`  ${files.length} files, ${(zip.length / 1e6).toFixed(2)} MB`);
console.log(`  sha256 ${sha256}`);
console.log(`  ${relative(root, pkgPath)}`);
console.log(`\nNext: npm run deploy`);
