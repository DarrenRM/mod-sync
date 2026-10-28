# Mod Sync

A Windows app that keeps a friend group's game mods in sync with every version published, and removes them without a trace.

## What it is

Mod Sync is personal software for Darren's friend group. Friends open the app, see which games have mods, install them with one click and stay current as new versions ship. One more click returns the game folder to exactly how it was. The first game is VEIN: a UE4SS mod loader plus a set of Lua mods.

## What's interesting

- **Uninstall replays a record.** Every install writes down each file it wrote (with its SHA-256 and a backup of anything it replaced), each directory it created, and each line it edited in a shared file along with the line it replaced. Uninstall plays that record backwards, and the end-to-end runs require the game folder to come back byte-identical.
- **A write-ahead undo log.** The engine logs each step before it touches the disk, so a failed install rolls back and an interrupted one is recovered from its log on the next run.
- **Shared files are edited, not owned.** Enabling a mod is a line edit to the loader's `mods.txt`, reversed exactly, trailing newline included. `preserve` globs keep a player's own config files through updates and uninstall. `ownedDirs` are swept whole only when Mod Sync created them.
- **Packages are reproducible and verified.** Zips are built with sorted entries and fixed timestamps, so the same files give the same SHA-256. The app checks the hash before installing, and the publish tool refuses to reuse a version number for different contents.
- **Updates respect the game.** The app checks the catalog on launch and every 30 minutes, and updates a game's mods only while that game is not running. The app updates itself through Tauri's signed updater.

## How it works

```
mods/<game>/            npm run         worker/public/         npm run        Cloudflare Worker
mod.json, build.mjs --> publish:mod --> catalog, package zips, --> deploy --> static assets, plus
                                        updater manifest                      bug reports in KV
                                                                                     |
                                        Tauri app  <---- catalog, zip, installer ----+
                                        Svelte UI + Rust shell
                                              |
                                        engine crate --> game folder + install record
```

| Path | What |
| --- | --- |
| `crates/engine/` | The install engine: apply, update and uninstall under an undo log, `mods.txt` line edits, Steam detection. |
| `src-tauri/` | The Tauri v2 shell: commands, catalog handling, settings, the auto-update loop, and a headless `--cli` mode that runs the same code paths as the UI. |
| `src/` | The Svelte 5 UI. In a plain browser it runs on sample data from `src/lib/mock.ts`. |
| `worker/` | One Cloudflare Worker: serves the catalog, packages and installer as static assets, and takes bug reports (kept 90 days) that only an admin token can read. |
| `mods/<game>/` | A game is a `mod.json` (catalog entry) plus a `build.mjs` that stages its package. |
| `tools/` | Publish a mod, release the app, read bug reports, and the end-to-end scripts in `tools/e2e/`. |

Design notes are in [docs/design.md](docs/design.md).

## Running it locally

Requirements: Windows 10 or 11, Node 20.11 or newer, and for the Rust parts a stable Rust toolchain with the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
npm ci
npm run dev           # the UI in a browser, on sample data, no backend needed
npm run test:engine   # the engine's tests (cargo test in crates/engine)
```

To run the real app, point it at a Worker that serves a catalog, for example a local `wrangler dev`:

```bash
cd worker && npm ci && npx wrangler dev        # serves worker/public
MODSYNC_BASE_URL=http://127.0.0.1:8787 npm run app:dev
```

Publishing a game package (`npm run publish:mod -- vein`) needs that game's mod sources; for VEIN they live in a separate repo, with the paths set in `mods/vein/mod.json`. The end-to-end scripts (`node tools/e2e/sandbox.mjs vein`) need a built debug app and a published package.

Environment variable names (see `.env.example`; the tools read the shell environment):

- `MODSYNC_BASE_URL`: the Worker's URL. It is baked into release builds and overrides the app's endpoint at runtime.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: password for the updater signing key, if it has one.
- `MODSYNC_STEAM_PATH`, `MODSYNC_DATA_DIR`: point the app at a fake Steam library and a scratch data folder for tests.

Kept out of the repo: `.keys/updater.key` (update signing), `.keys/admin-token` (the Worker's `ADMIN_TOKEN` secret) and `worker/wrangler.local.jsonc` (the real KV namespace id; `npm run deploy` uses it).

## Status

- Personal software for Darren's friend group. Released October 2026; the app is at 0.3.0.
- Built with Tauri, Rust, Svelte and Cloudflare.
- A Rust install engine records every file it touches, so uninstall can undo it.
- Self-updates are signed.
- 53 Rust tests, 51 of them on the install engine, plus end-to-end scripts that drive the real app against a fake Steam library and check that an uninstall leaves the game folder byte-identical.

## How it was built

Designed and directed by Darren Mills, built with AI coding agents. [`CLAUDE.md`](CLAUDE.md) holds the rules the agents work under (layout, release steps, test sandbox safety), and [`.claude/skills/publish-mod/`](.claude/skills/publish-mod/SKILL.md) is the skill that ships a mod.

## License

No license yet.
