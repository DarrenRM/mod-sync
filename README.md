# Mod Sync

Friends open the app, see which games have mods, install them, and stay up to date automatically. One click removes everything.

- Download page: https://mod-sync.example.invalid
- How it works: [docs/design.md](docs/design.md)
- How to ship a mod or app update: [CLAUDE.md](CLAUDE.md) and the `publish-mod` skill in `.claude/skills/`

```bash
npm install
npm run app:dev                    # run the app
npm run dev                        # UI only, in a browser, with sample data
npm run publish:mod -- vein        # build a package into worker/public
node tools/e2e/sandbox.mjs vein    # install/update/uninstall test in a fake Steam library
npm run deploy                     # push worker/public to Cloudflare
```
