# Mod Sync

A Windows app that installs a group's game mods in one click, keeps every player on the latest version, and takes the mods back out cleanly.

## What it is

Mod Sync ships mods to a group of friends who play together. Without it, each friend copies a proxy `dwmapi.dll` and a `ue4ss/` folder into the game, enables each mod with a line in the loader's `mods.txt`, and does it all again for every new version. With it, friends open the app, see which games have mods and press Sync. New versions then arrive on their own while the game is closed. Uninstall returns the game folder to how it was, keeping only the player's own config files. The first game is VEIN: a UE4SS mod loader plus a set of Lua mods.

Builds reach the group from a private Cloudflare Worker, so there is no public download. The whole interface runs in a browser on sample data: `npm ci && npm run dev`, then add `?first` or `?case=running`, `broken`, `updating`, `appupdate`, `insync` or `offline` to the URL to see each state.

## Design decisions

- **One Sync button.** Install, update and repair are all "Sync". The status line names the state (Not synced, Out of sync, In sync) and the button names the action, so the two never repeat each other.
- **The Steam button follows ownership.** For a game that isn't on the PC, owners get "Install on Steam", which opens Steam's install dialog, and everyone else gets "Get on Steam", which opens the store page. Steam's install link only shows an error for a game you don't own.
- **Each player chooses.** Settings opens once on first launch, so every player decides about starting with Windows and sending crash logs themselves.
- **Never touch a running game.** Background syncs skip a game that is running, and its button reads "Close VEIN first" instead of failing. An outdated app blocks syncing, not playing what is already in sync.
- **Players see their mods, not the plumbing.** The library row lists the mods people play with; the loader appears only on the game page, as "Mod Loader".
- **Removing mods is one confirmed click.** "Uninstall mods" asks once, then replays the install record backwards (below).
- **Every screen can be reviewed without the game.** The browser mock implements the same API interface as the Rust backend, and its `?case=` states put each screen's wording in front of a reviewer.

## What's interesting

- **Uninstall replays a record.** Every install writes down each file it wrote (with its SHA-256 and a backup of anything it replaced), each directory it created, and each line it edited in a shared file along with the line it replaced. Uninstall plays that record backwards. The end-to-end run requires the game folder to come back byte-identical, apart from a config file it adds as the player, which uninstall keeps on purpose.
- **A write-ahead undo log.** The engine logs each step, and flushes it to disk, before it touches the game folder. A failed sync rolls back. A sync cut short by a crash or a power cut is rolled back from its log when the app next starts, or before the next sync or uninstall of that game. The install record is written inside the same transaction, so the record and the folder always agree.
- **Shared files are edited, not owned.** Enabling a mod is a line edit to the loader's `mods.txt`, reversed exactly, trailing newline included. `preserve` globs keep a player's own config files through updates and uninstall. `ownedDirs` are swept on uninstall only when Mod Sync created them.
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

Design notes, including the trust model, are in [docs/design.md](docs/design.md).

## Running it locally

Requirements: Windows 10 or 11, Node 20.11 or newer, and for the Rust parts a stable Rust toolchain with the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```bash
npm ci
npm run dev           # the full UI in a browser, on sample data, no backend needed
npm run test:engine   # the engine's tests (cargo test in crates/engine)
```

To run the real app, point it at a Worker that serves a catalog, for example a local `wrangler dev`:

```bash
cd worker && npm ci && npm run dev              # wrangler dev, serving worker/public
MODSYNC_BASE_URL=http://127.0.0.1:8787 npm run app:dev
```

The examples use bash syntax. In PowerShell, set the variable first: `$env:MODSYNC_BASE_URL = "http://127.0.0.1:8787"`, then `npm run app:dev`.

The catalog in `worker/public` is generated, not committed: `npm run publish:mod -- vein` builds it from the game's mod sources, which for VEIN live in a separate repository (paths in `mods/vein/mod.json`). The end-to-end scripts (`node tools/e2e/sandbox.mjs vein`) need a built debug app and a published package.

`npx tauri build` also signs the updater artifacts, so it needs `TAURI_SIGNING_PRIVATE_KEY`. For a local unsigned build, run `npx tauri build --config '{"bundle":{"createUpdaterArtifacts":false}}'`. Releases go through `npm run release:app`, which signs and sets the updater endpoint.

Environment variables (see `.env.example`; the tools read the shell environment):

- `MODSYNC_BASE_URL`: the Worker's URL for the catalog, packages and bug reports. A build made with it set bakes it in, and setting it at runtime points any build elsewhere. The updater's endpoint is separate: `npm run release:app` writes it into the release build's Tauri config.
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: password for the updater signing key, if it has one.
- `MODSYNC_STEAM_PATH`, `MODSYNC_DATA_DIR`: point the app at a fake Steam library and a scratch data folder for tests.
- `MODSYNC_SKIP_RUNNING_CHECK`: any value makes the app treat every game as closed. The sandbox sets it so a copy of the game open on the test PC doesn't block the run.

Kept out of the repo: `.keys/updater.key` (update signing), `.keys/admin-token` (the Worker's `ADMIN_TOKEN` secret) and `worker/wrangler.local.jsonc` (the real KV namespace id; `npm run deploy` uses it).

## Status

Released to the group in October 2026; the app is at 0.3.0. Built with Tauri 2, Rust, Svelte 5 and Cloudflare Workers. 57 Rust tests, 53 of them on the install engine, plus end-to-end scripts that drive the real app against a fake Steam library.

## How it was built

Darren Mills designed and directed Mod Sync: the product, the interface, and the checking loop the code is built inside. AI coding agents wrote much of the code within that loop.

- [`CLAUDE.md`](CLAUDE.md) holds the rules the agents work under: the layout, the release steps, and never testing installs on the real game without asking.
- The engine's tests pin the failure modes: rollback when a game file is locked, a crash in the middle of an install, an uninstall that only partly succeeds, a zip that tries to write outside its folder.
- A headless `--cli` mode runs the same code as the UI, so `tools/e2e/sandbox.mjs` installs, repairs, updates and uninstalls the real package in a fake Steam library.
- The [`publish-mod` skill](.claude/skills/publish-mod/SKILL.md) makes that sandbox run a gate: every check must pass before a mod is deployed to the group.
- The browser mock puts every screen in every state, so the wording can be reviewed without the game.

## Third-party

`mods/vein/layer/ue4ss/` holds configuration for [UE4SS](https://github.com/UE4SS-RE/RE-UE4SS), the MIT-licensed mod loader the VEIN mods run on: its settings file (based on UE4SS's template), VEIN-specific signatures and memory layouts, and a starting `mods.txt`. The UE4SS binaries are not in this repo; the package build takes them from a UE4SS build and ships its LICENSE alongside them.

## License

MIT. See [LICENSE](LICENSE).
