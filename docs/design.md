# Mod Sync — design

A Windows app for a group of friends. It lists games that have mods, installs them, keeps them updated, and removes them again, returning the game folder to how it was apart from the player's own config files.

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
      "ownedDirs": ["ue4ss"],                    // swept on uninstall, keeping files that match preserve, but only if the install created it
      "edits": [{ "type": "line", "file": "ue4ss/Mods/mods.txt", "key": "SharedCraftingClient", "line": "SharedCraftingClient : 1", "before": "Keybinds" }]
    }
  }]
}
```

The package zip's entries map 1:1 onto `<game>/<target>/`. An `id` names files in the app's data folder and a `target` is joined onto the game folder, so the app skips a game whose id isn't a plain slug or whose target is absolute or climbs out with `..`.

## Install record

`%APPDATA%\com.modsync.app\installs\<id>.json` lists every file written (with its hash and backup path if it replaced a file), every directory created, and every line edit with what it replaced. Uninstall replays it backwards, so the game folder ends up byte for byte as it was. The one exception is a file matching `preserve` that the player made themselves, such as a `config_local.lua` next to a mod: it stays, along with the folders that hold it. Backups live in `…\backups\<id>\`.

Every install or update runs under an undo log in `…\work\undo-<id>\`. Each step is written to the log and flushed to disk before it touches the game folder, and the new install record is written as the transaction's last step, followed by a commit marker. If anything fails half way, everything already done is rolled back. If the app is killed or the PC loses power mid-sync, the log stays behind: the app rolls it back when it next starts (unless the game is running), and again before the next sync or uninstall of that game. A log that ends in the commit marker is only deleted, never rolled back, so a finished sync survives even when another program held its log open. The record is rolled back with the files, so once a sync or its rollback completes, the record matches the files Mod Sync has in place.

Rolling back is safe to repeat. A step that fails, such as restoring a file the game holds open, keeps the log, and the original's backup, for the next try. A folder the sync created stays if something else has written into it since, as it does on uninstall.

## Self-update

Tauri updater plugin, minisign-signed. Endpoint: `<worker>/app/latest.json`. The signing key is in `.keys/` (gitignored). Back it up: without it, existing installs can't be updated.

## Trust model

App updates are signed: the updater checks every installer against the minisign public key in `tauri.conf.json`, and the private key stays offline in `.keys/`. The catalog and the packages are not signed. The app trusts them because they arrive over HTTPS from the Worker, and it checks each package against the SHA-256 that the catalog lists before installing anything. The catalog also names the log files a bug report collects. So whoever can deploy the Worker decides what friends install, and a package can carry native DLLs (VEIN's loader is a `dwmapi.dll`).

Signing `catalog.json` with the same offline key, and verifying it in the app before using it, would raise that bar: changing what gets installed would take the signing key as well as the Cloudflare account. The package hashes already in the catalog would then cover the packages too.

## Bug reports

The app zips its own log, the install record and the game's log files (last 2 MB each), then `POST`s to `<worker>/reports`. With "Send crash logs" on, it sends one automatically when an install fails or a new crash dump appears. The Worker keeps each report in KV for 90 days, and only the admin token can list, download or delete them.

`POST /reports` accepts a report from anyone who has the Worker's URL, and every installed app has it. The Worker caps a report at 10 MB, and KV expires reports after 90 days. A deployment that wants a tighter limit can add a Workers rate-limiting binding or a WAF rate-limiting rule on `/reports`.
