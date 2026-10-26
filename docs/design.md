# Mod Sync — design

A small Windows app for a friend group. It lists games that have mods, installs them, keeps them updated, and removes them completely.

## Pieces

| Piece | Where | What |
| --- | --- | --- |
| Desktop app | `src/` (Svelte UI), `src-tauri/` (Rust) | Tauri v2. Detects Steam games, installs/updates/uninstalls packages, self-updates, sends bug reports. |
| Worker | `worker/` | One Cloudflare Worker. Static assets: `catalog.json`, `packages/**`, `app/**` (installer + updater manifest), landing page. `POST /reports` stores a zip in KV. |
| Mod sources | `mods/<game>/` | `mod.json` (catalog entry source) plus per-game layer files. |
| Tools | `tools/` | Build a package, release the app, deploy, read reports. |

## Catalog (`/catalog.json`)

```jsonc
{
  "schema": 1,
  "minApp": "0.1.0",               // older apps must self-update before doing anything
  "games": [{
    "id": "vein",
    "name": "VEIN",
    "steamAppId": 1857950,
    "hideIfMissing": false,          // true: only shown when the game is installed (e.g. a dedicated server)
    "target": "Vein/Binaries/Win64", // package root, relative to the Steam install dir
    "processNames": ["Vein.exe", "Vein-Win64-Test.exe", "Vein-Win64-Shipping.exe"],
    "logs": ["{target}/ue4ss/UE4SS.log", "{game}/Vein/Saved/Logs/Vein.log"],
    "crashGlobs": ["{target}/ue4ss/crash_*.dmp"],
    "package": {
      "version": "0.5.0",
      "url": "packages/vein/vein-0.5.0.zip",   // relative to the catalog URL
      "sha256": "…", "size": 123,
      "released": "2026-10-26",
      "mods": [{ "name": "Shared Crafting", "version": "0.5.0", "summary": "Craft from your base's storage" }],
      "notes": ["…"],
      "preserve": ["ue4ss/Mods/mods.txt", "**/config_local.lua"],   // never overwritten if present
      "ownedDirs": ["ue4ss"],                    // removed whole on uninstall, but only if the install created it
      "edits": [{ "type": "line", "file": "ue4ss/Mods/mods.txt", "key": "SharedCraftingClient", "line": "SharedCraftingClient : 1", "before": "Keybinds" }]
    }
  }]
}
```

The package zip's entries map 1:1 onto `<game>/<target>/`.

## Install record

`%APPDATA%\com.modsync.app\installs\<id>.json` lists every file written (with its hash and backup path if it replaced a file), every directory created, and every line edit with what it replaced. Uninstall replays it backwards, so the game folder ends up byte for byte as it was. Backups live in `…\backups\<id>\`.

Every install or update runs under an undo log. If anything fails half way, everything already done is rolled back.

## Self-update

Tauri updater plugin, minisign-signed. Endpoint: `<worker>/app/latest.json`. The signing key is in `.keys/` (gitignored). Back it up: without it, existing installs can't be updated.

## Bug reports

The app zips its own log, the install record and the game's log files (last 2 MB each), then `POST`s to `<worker>/reports`. With "Share crash logs" on, it sends one automatically when an install fails or a new crash dump appears.
