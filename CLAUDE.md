# Mod Sync

Windows app (Tauri v2 + Svelte 5) that installs, updates and fully uninstalls game mods for a friend group. Backend is one Cloudflare Worker. Design: [docs/design.md](docs/design.md).

- Worker: https://mod-sync.example.invalid (KV `REPORTS`)
- Landing page with the download button: same URL

## Layout

| Path | What |
| --- | --- |
| `src/` | Svelte UI. `lib/mock.ts` stands in for the backend in a plain browser (`npm run dev`). |
| `src-tauri/` | Tauri shell: commands (`lib.rs`), app logic (`core.rs`), headless CLI (`cli.rs`). |
| `crates/engine/` | Install engine: apply/update/uninstall with undo log, `mods.txt` line edits, Steam detection. `cargo test` there. |
| `worker/` | Worker (`src/index.ts`) + static assets in `public/` (catalog, packages, app updates). |
| `mods/<game>/` | `mod.json` (catalog entry source) + `build.mjs` (stages the package) + layer files. |
| `tools/` | `publish-mod.mjs`, `release-app.mjs`, `reports.mjs`, `make-icon.mjs`. |
| `.keys/` | Gitignored. `updater.key` (app update signing, no password) and `admin-token` (reports API). Back these up. |

## Ship a mod update (e.g. VEIN)

1. Bump the mod version in the mod repo (VEIN: `..\vein-mods\src\lua\vein_core\version.lua`). For a packaging-only change, bump `revision` in `mods/vein/mod.json` instead.
2. Update `notes` in `mods/vein/mod.json` (short lines, what players will notice).
3. `npm run publish:mod -- vein` then `npm run deploy`.

Friends' apps pick it up within 30 minutes (or on launch) and update automatically when the game isn't running.

## Ship an app update

`npm run release:app -- 0.2.0 "notes"` then `npm run deploy`. Installed apps download it and show "Restart to update". Bump `minApp` in `worker/public/catalog.json` only when old apps must not run against the new catalog.

## Bug reports

`npm run reports` lists, `npm run reports -- get latest` downloads and unzips into `reports/` (gitignored). Each holds `report.json`, `app.log`, the install record and the game logs.

## Testing

- Engine: `npm run test:engine`.
- Headless app (same code as the UI): `mod-sync.exe --cli status|install <id>|uninstall <id>|report <id> <note>|check-update|self-update [--out file]`.
- Sandbox: `MODSYNC_STEAM_PATH=<fake steam root>` and `MODSYNC_DATA_DIR=<dir>` keep tests off the real Steam library and app data. `MODSYNC_BASE_URL` points at another Worker (e.g. `wrangler dev`).
- Never test installs on the real VEIN install without asking; `..\vein-mods\docs\current-status.md` describes its safety state.

## Adding a game

Copy `mods/vein/`, set `steamAppId`, `target` (package root inside the game folder), `processNames`, `logs`, `preserve`, `ownedDirs`, `edits`, write `build.mjs` to stage files relative to `target`, then publish. Use `hideIfMissing: true` for things only some people have (a dedicated server).
