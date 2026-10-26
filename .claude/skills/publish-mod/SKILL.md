---
name: publish-mod
description: Get a game mod into Mod Sync and ship it to friends via Cloudflare. Use when the user wants to add a new game or mod to Mod Sync, ship/push/release a mod update, change what a package installs, or says things like "put this mod in mod sync", "send the new VEIN build to everyone", "add <game> to the installer".
---

# Publish a mod to Mod Sync

Mod Sync installs whatever `mods/<game>/build.mjs` stages into the game folder, tracks every file, and can remove it all again. Friends' apps check the catalog on launch and every 30 minutes, and update mods automatically when the game isn't running. Whatever you deploy reaches everyone, so test before you deploy.

Read the repo's `CLAUDE.md` first; it lists the paths and commands.

## A. Ship an update to a game that's already in Mod Sync

1. **Version.** Bump the mod's own version in its repo (VEIN: `mod = "x.y.z"` in `..\vein-mods\src\lua\vein_core\version.lua`). If only the packaging changed (loader settings, an extra file), bump `"revision"` in `mods/<game>/mod.json` instead. Never republish a version with different contents; `publish-mod.mjs` refuses unless you pass `--force`.
2. **Mod repo checks.** Run that repo's own fast checks before packaging. For VEIN that's `tools/dev/Invoke-VeinFastChecks.ps1`; also read `docs/current-status.md` for anything flagged as not ready.
3. **Notes.** Rewrite `"notes"` in `mods/<game>/mod.json`: 1–4 short lines on what players will notice. No internal jargon, no ticket numbers, no trailing periods. Update each `mods[].summary` if what a mod does changed.
4. **Build.** `npm run publish:mod -- <game>`. Read the printed file count and size; a big jump means something unintended got staged. List the zip if unsure: `unzip -l worker/public/packages/<game>/<game>-<version>.zip`.
5. **Test before deploy.** Build the app if `src-tauri/target/debug/mod-sync.exe` is older than the source (`cd src-tauri && cargo build`), then run `node tools/e2e/sandbox.mjs <game>`. It serves `worker/public` locally and installs, repairs, updates and uninstalls into a fake Steam library, checking that the folder comes back byte-identical. Everything must PASS.
6. **Deploy.** `npm run deploy`. Then confirm the live catalog has the new version: `curl -s https://mod-sync.example.invalid/catalog.json`. Optionally run `node tools/e2e/sandbox.mjs <game> --live`.
7. **Tell the user** the version, the notes friends will see, and that apps update within 30 minutes.

## B. Add a new game

1. **Facts first.** You need:
   - the Steam app id (`appmanifest_<id>.acf` in a `steamapps` folder; `libraryfolders.vdf` lists the libraries);
   - the install dir;
   - the folder the mod's files go into, relative to the game root (UE games: usually `<Project>/Binaries/Win64`, or `<Project>/Content/Paks/~mods` for pak mods);
   - the exe names when running (check Task Manager or the folder: launcher plus the `-Win64-Shipping`/`-Test` exe);
   - where the game and loader write logs.
2. **What gets installed.** Work out the exact files from the mod repo's release output, not from a dev install. Dev installs carry PDBs, dumps, object dumps, dev-only mods and hot-reload settings. If a loader (UE4SS etc.) is needed, ship the minimal runtime: the proxy DLL, the loader DLL, shared libs and game-specific settings or signatures copied from a known-working install. No PDBs and no docs.
3. **Create `mods/<game>/`** by copying `mods/vein/`:
   - `mod.json`: `id`, `name`, `steamAppId`, `target`, `processNames`, `logs` (`{game}`, `{target}`, `{localappdata}` tokens), `crashGlobs`, `mods` (name / version / one-line summary each, `{modVersion}` substitutes), `notes`.
   - Also `preserve`: files the user may edit or that other mods share, e.g. a loader's `mods.txt` and `**/config_local.lua`. These are never overwritten when present.
   - `ownedDirs`: dirs the package creates, such as `ue4ss`. They're swept whole on uninstall, but only if Mod Sync created them.
   - `edits`: line edits to shared files, like enabling the mod in `mods.txt`.
   - Set `"hideIfMissing": true` for things only some people own (dedicated servers).
   - `build.mjs`: export `async function build({ mod, stage, work })` that writes files into `stage` relative to `target` and returns `{ modVersion }`. Build into `work`, never into the mod repo's own `dist/`.
4. Then do A.4 to A.7. Game art (hero and logo) is pulled automatically from the local Steam cache, or Steam's CDN, at publish time.

## Rules

- Never install onto the real game folders to test. Use the sandbox. Ask before touching a real install.
- If the game is running on this machine, the app refuses to install (expected); the sandbox skips that check on purpose.
- Don't bump `minApp` in the catalog unless old apps truly can't handle the new catalog.
- `.keys/` holds the app signing key and the reports token. Never commit, print or upload them.
- Bug reports from friends: `npm run reports`, `npm run reports -- get latest` (unzips into `reports/`).
