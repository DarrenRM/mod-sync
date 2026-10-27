//! App logic, independent of the UI: catalog, game detection, install/update/uninstall, reports.
use crate::catalog::{resolve_url, Catalog, GameEntry, PackageInfo};
use crate::settings::{PublicSettings, Settings};
use crate::util::{self, log, version_lt};
use futures_util::StreamExt;
use modsync_engine as engine;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

#[derive(Clone)]
pub struct Paths {
    pub data: PathBuf,
}

impl Paths {
    pub fn settings(&self) -> PathBuf {
        self.data.join("settings.json")
    }
    pub fn catalog_cache(&self) -> PathBuf {
        self.data.join("catalog.json")
    }
    pub fn record(&self, id: &str) -> PathBuf {
        self.data.join("installs").join(format!("{id}.json"))
    }
    pub fn backups(&self, id: &str) -> PathBuf {
        self.data.join("backups").join(id)
    }
    pub fn downloads(&self) -> PathBuf {
        self.data.join("downloads")
    }
    pub fn work(&self, what: &str, id: &str) -> PathBuf {
        self.data.join("work").join(format!("{what}-{id}"))
    }
    pub fn log(&self) -> PathBuf {
        self.data.join("logs").join("app.log")
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameView {
    pub id: String,
    pub name: String,
    pub steam_app_id: u32,
    pub game: &'static str,
    /// Only meaningful when `game` is "missing": decides Install vs Get on Steam.
    pub owned: bool,
    pub path: Option<String>,
    pub running: bool,
    pub mods: &'static str,
    pub installed_version: Option<String>,
    pub latest: PackageInfo,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppStateView {
    pub app_version: String,
    pub offline: bool,
    pub needs_app_update: bool,
    /// Show Settings once, so people choose Start with Windows and crash logs themselves.
    pub first_run: bool,
    pub settings: PublicSettings,
    pub games: Vec<GameView>,
}

pub struct Core {
    pub paths: Paths,
    pub base_url: String,
    pub app_version: String,
    pub settings: Mutex<Settings>,
    http: reqwest::Client,
    catalog: Mutex<Option<Catalog>>,
    offline: AtomicBool,
    busy: Mutex<HashSet<String>>,
    emit: Emit,
}

struct BusyGuard<'a>(&'a Core, String);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.busy.lock().unwrap().remove(&self.1);
        // Tell the UI the work is over (success or not), including syncs the app started itself.
        self.0.progress(&self.1, "done", 0, 0);
    }
}

enum Detected {
    Installed(PathBuf),
    Updating(PathBuf),
    Missing,
}

impl Core {
    pub fn new(data: PathBuf, base_url: String, app_version: String, emit: Emit) -> Self {
        let paths = Paths { data };
        util::init_log(paths.log());
        let settings = Settings::load(&paths.settings());
        let catalog = std::fs::read(paths.catalog_cache()).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let http = reqwest::Client::builder()
            .user_agent(format!("ModSync/{app_version}"))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("http client");
        log(format!("start v{app_version} data={} base={base_url}", paths.data.display()));
        Self {
            paths,
            base_url,
            app_version,
            settings: Mutex::new(settings),
            http,
            catalog: Mutex::new(catalog),
            offline: AtomicBool::new(false),
            busy: Mutex::new(HashSet::new()),
            emit,
        }
    }

    fn emit(&self, event: &str, payload: Value) {
        (self.emit)(event, payload)
    }

    pub fn toast(&self, text: impl Into<String>) {
        self.emit("toast", Value::String(text.into()));
    }

    // ---------- catalog ----------

    pub async fn refresh_catalog(&self) {
        let url = format!("{}/catalog.json?t={}", self.base_url, util::unix_now());
        let result: Result<Catalog, String> = async {
            let res = self.http.get(&url).timeout(Duration::from_secs(20)).send().await.map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                return Err(format!("HTTP {}", res.status()));
            }
            res.json::<Catalog>().await.map_err(|e| e.to_string())
        }
        .await;
        match result {
            Ok(cat) => {
                let _ = util::write_json_atomic(&self.paths.catalog_cache(), &cat);
                *self.catalog.lock().unwrap() = Some(cat);
                self.offline.store(false, Ordering::Relaxed);
            }
            Err(e) => {
                log(format!("catalog fetch failed: {e}"));
                self.offline.store(true, Ordering::Relaxed);
            }
        }
    }

    pub fn catalog(&self) -> Option<Catalog> {
        self.catalog.lock().unwrap().clone()
    }

    fn entry(&self, id: &str) -> Result<GameEntry, String> {
        self.catalog()
            .and_then(|c| c.games.into_iter().find(|g| g.id == id))
            .ok_or_else(|| format!("Unknown game: {id}"))
    }

    pub fn needs_app_update(&self) -> bool {
        self.catalog()
            .and_then(|c| c.min_app)
            .map(|min| version_lt(&self.app_version, &min))
            .unwrap_or(false)
    }

    // ---------- detection ----------

    fn detect(&self, g: &GameEntry) -> Detected {
        if let Some(p) = self.settings.lock().unwrap().paths.get(&g.id) {
            let p = PathBuf::from(p);
            if p.join(&g.target).is_dir() {
                return Detected::Installed(p);
            }
        }
        let Some(root) = engine::steam::steam_root() else { return Detected::Missing };
        match engine::steam::find_app(&root, g.steam_app_id) {
            Some(app) if app.fully_installed => Detected::Installed(app.install_dir),
            Some(app) => Detected::Updating(app.install_dir),
            None => Detected::Missing,
        }
    }

    pub fn owns(&self, app_id: u32) -> bool {
        engine::steam::steam_root().is_some_and(|root| engine::steam::likely_owned(&root, app_id))
    }

    fn game_root(&self, g: &GameEntry) -> Result<PathBuf, String> {
        match self.detect(g) {
            Detected::Installed(p) => Ok(p),
            Detected::Updating(_) => Err(format!("Steam is updating {}", g.name)),
            Detected::Missing => Err(format!("{} isn't installed", g.name)),
        }
    }

    fn running(&self, g: &GameEntry) -> bool {
        // Sandbox tests only: the real game may be open on the test machine.
        if std::env::var_os("MODSYNC_SKIP_RUNNING_CHECK").is_some() {
            return false;
        }
        !g.process_names.is_empty() && !engine::running_processes(&g.process_names).is_empty()
    }

    pub fn load_record(&self, id: &str) -> Option<engine::InstallRecord> {
        std::fs::read(self.paths.record(id)).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    pub fn view(&self) -> AppStateView {
        let catalog = self.catalog();
        let mut games = Vec::new();
        for g in catalog.iter().flat_map(|c| c.games.iter()) {
            let Ok(latest) = g.info() else { continue };
            let detected = self.detect(g);
            if g.hide_if_missing && matches!(detected, Detected::Missing) {
                continue;
            }
            let record = self.load_record(&g.id);
            let (game, path) = match &detected {
                Detected::Installed(p) => ("installed", Some(p.display().to_string())),
                Detected::Updating(p) => ("updating", Some(p.display().to_string())),
                Detected::Missing => ("missing", None),
            };
            let mods = match &record {
                None => "none",
                Some(r) if r.files.iter().any(|f| !r.target.join(&f.path).exists()) => "broken",
                Some(r) if r.version != latest.version => "outdated",
                Some(_) => "current",
            };
            games.push(GameView {
                id: g.id.clone(),
                name: g.name.clone(),
                steam_app_id: g.steam_app_id,
                game,
                owned: game != "missing" || self.owns(g.steam_app_id),
                path,
                running: self.running(g),
                mods,
                installed_version: record.map(|r| r.version),
                latest,
            });
        }
        // One lock: a second `lock()` in the same expression would wait on the first forever.
        let (settings, first_run) = {
            let s = self.settings.lock().unwrap();
            (s.public(), !s.welcomed)
        };
        AppStateView {
            app_version: self.app_version.clone(),
            offline: self.offline.load(Ordering::Relaxed),
            needs_app_update: self.needs_app_update(),
            first_run,
            settings,
            games,
        }
    }

    fn claim(&self, id: &str) -> Result<BusyGuard<'_>, String> {
        if !self.busy.lock().unwrap().insert(id.to_string()) {
            return Err("Already working on it".into());
        }
        Ok(BusyGuard(self, id.to_string()))
    }

    fn progress(&self, id: &str, phase: &str, done: u64, total: u64) {
        self.emit("progress", json!({ "id": id, "phase": phase, "done": done, "total": total }));
    }

    // ---------- install / update / repair ----------

    pub async fn install(self: &Arc<Self>, id: &str) -> Result<String, String> {
        let result = self.install_inner(id).await;
        if let Err(e) = &result {
            log(format!("install {id} failed: {e}"));
            if self.settings.lock().unwrap().share_logs {
                let me = self.clone();
                let (id, note) = (id.to_string(), format!("auto: install failed: {e}"));
                tauri::async_runtime::spawn(async move {
                    let _ = me.report(Some(&id), &note).await;
                });
            }
        }
        result
    }

    async fn install_inner(&self, id: &str) -> Result<String, String> {
        if self.needs_app_update() {
            return Err("Update the app first".into());
        }
        let _guard = self.claim(id)?;
        let g = self.entry(id)?;
        let info = g.info()?;
        let spec = g.spec()?;
        let root = self.game_root(&g)?;
        if self.running(&g) {
            return Err(format!("Close {} first", g.name));
        }
        let target = root.join(&g.target);
        if !target.is_dir() {
            return Err(format!("Can't find {} in {}", g.target, root.display()));
        }
        log(format!("install {id} {} into {}", info.version, target.display()));

        let zip = self.download(id, &info).await?;

        let this_paths = self.paths.clone();
        let prev = self.load_record(id);
        let emit = self.emit.clone();
        let id_owned = id.to_string();
        let record = tauri::async_runtime::spawn_blocking(move || -> Result<engine::InstallRecord, String> {
            let stage = this_paths.work("stage", &id_owned);
            let undo = this_paths.work("undo", &id_owned);
            let _ = std::fs::remove_dir_all(&stage);
            let _ = std::fs::remove_dir_all(&undo);
            std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
            engine::extract_zip(&zip, &stage).map_err(|e| format!("Couldn't unpack the download: {e}"))?;
            let mut last = Instant::now() - Duration::from_secs(1);
            let mut on_progress = |done: u64, total: u64| {
                if last.elapsed() > Duration::from_millis(40) || done == total {
                    last = Instant::now();
                    emit("progress", json!({ "id": id_owned, "phase": "install", "done": done, "total": total }));
                }
            };
            let record = engine::apply(
                &target,
                &stage,
                &spec,
                &id_owned,
                prev.as_ref(),
                &this_paths.backups(&id_owned),
                &undo,
                &mut on_progress,
            )
            .map_err(|e| {
                if e.is_file_in_use() {
                    "A game file is in use. Close the game and try again.".to_string()
                } else {
                    e.to_string()
                }
            })?;
            util::write_json_atomic(&this_paths.record(&id_owned), &record)?;
            let _ = std::fs::remove_dir_all(&stage);
            Ok(record)
        })
        .await
        .map_err(|e| e.to_string())??;

        log(format!("installed {id} {} ({} files)", record.version, record.files.len()));
        Ok(record.version)
    }

    /// Download the package (or reuse a verified copy) and return its path.
    async fn download(&self, id: &str, info: &PackageInfo) -> Result<PathBuf, String> {
        let dir = self.paths.downloads();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = dir.join(format!("{id}-{}.zip", info.version));
        if file.exists() && engine::sha256_file(&file).map(|h| h.eq_ignore_ascii_case(&info.sha256)).unwrap_or(false) {
            return Ok(file);
        }
        let url = resolve_url(&self.base_url, &info.url);
        let res = self.http.get(&url).send().await.map_err(|e| format!("Download failed: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("Download failed: HTTP {}", res.status()));
        }
        let total = res.content_length().unwrap_or(info.size);
        let part = file.with_extension("part");
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut last = Instant::now();
        let mut stream = res.bytes_stream();
        self.progress(id, "download", 0, total);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| format!("Download failed: {e}"))?;
            out.write_all(&chunk).map_err(|e| e.to_string())?;
            hasher.update(&chunk);
            done += chunk.len() as u64;
            if last.elapsed() > Duration::from_millis(60) {
                last = Instant::now();
                self.progress(id, "download", done, total);
            }
        }
        drop(out);
        self.progress(id, "download", total, total);
        let hash = hex::encode(hasher.finalize());
        if !hash.eq_ignore_ascii_case(&info.sha256) {
            let _ = std::fs::remove_file(&part);
            return Err("Download was corrupted. Try again.".into());
        }
        std::fs::rename(&part, &file).map_err(|e| e.to_string())?;
        // Drop older downloads of this game.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with(&format!("{id}-")) && e.path() != file {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        Ok(file)
    }

    // ---------- uninstall ----------

    pub async fn uninstall(&self, id: &str) -> Result<(), String> {
        let _guard = self.claim(id)?;
        let Some(record) = self.load_record(id) else { return Ok(()) };
        if let Ok(g) = self.entry(id) {
            if self.running(&g) {
                return Err(format!("Close {} first", g.name));
            }
        }
        self.progress(id, "remove", 0, 0);
        let paths = self.paths.clone();
        let id_owned = id.to_string();
        log(format!("uninstall {id} {}", record.version));
        tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
            match engine::uninstall(&record, &paths.backups(&id_owned)) {
                Ok(()) => {
                    let _ = std::fs::remove_file(paths.record(&id_owned));
                    let _ = std::fs::remove_dir_all(paths.backups(&id_owned));
                    Ok(())
                }
                Err(e) => {
                    util::write_json_atomic(&paths.record(&id_owned), &e.remaining)?;
                    Err(format!("Some files are in use. Close the game and try again. ({})", e.message))
                }
            }
        })
        .await
        .map_err(|e| e.to_string())??;
        log(format!("uninstalled {id}"));
        Ok(())
    }

    // ---------- misc ----------

    pub fn locate(&self, id: &str, path: &str) -> Result<(), String> {
        let g = self.entry(id)?;
        let mut p = PathBuf::from(path);
        // Accept the game root, or any folder inside it (like Binaries\Win64).
        loop {
            if p.join(&g.target).is_dir() {
                break;
            }
            match p.parent() {
                Some(parent) => p = parent.to_path_buf(),
                None => return Err(format!("That doesn't look like the {} folder", g.name)),
            }
        }
        let mut s = self.settings.lock().unwrap();
        s.paths.insert(id.to_string(), p.display().to_string());
        s.save(&self.paths.settings())
    }

    pub fn game_dir(&self, id: &str) -> Result<PathBuf, String> {
        let g = self.entry(id)?;
        match self.detect(&g) {
            Detected::Installed(p) | Detected::Updating(p) => Ok(p),
            Detected::Missing => Err(format!("{} isn't installed", g.name)),
        }
    }

    pub fn steam_app_id(&self, id: &str) -> Result<u32, String> {
        Ok(self.entry(id)?.steam_app_id)
    }

    pub fn set_settings(&self, s: PublicSettings) -> Result<(), String> {
        let mut cur = self.settings.lock().unwrap();
        cur.auto_update = s.auto_update;
        cur.background = s.background;
        cur.share_logs = s.share_logs;
        cur.save(&self.paths.settings())
    }

    pub fn mark_welcomed(&self) -> Result<(), String> {
        let mut cur = self.settings.lock().unwrap();
        cur.welcomed = true;
        cur.save(&self.paths.settings())
    }

    /// Bring every installed game's mods up to date. Returns "<game> synced  ·  <version>" for each update.
    pub async fn auto_update(self: &Arc<Self>) -> Vec<String> {
        let mut done = Vec::new();
        if !self.settings.lock().unwrap().auto_update || self.needs_app_update() {
            return done;
        }
        let view = self.view();
        for g in view.games {
            if g.game == "installed" && !g.running && (g.mods == "outdated" || g.mods == "broken") {
                match self.install(&g.id).await {
                    Ok(v) => done.push(format!("{} synced  ·  {v}", g.name)),
                    Err(e) => log(format!("auto-update {} failed: {e}", g.id)),
                }
            }
        }
        done
    }

    /// With crash logs on: report once for each game that left a new crash dump since the last scan.
    pub async fn scan_crashes(self: &Arc<Self>) {
        let (enabled, since) = {
            let s = self.settings.lock().unwrap();
            (s.share_logs, s.last_crash_scan)
        };
        let now = util::unix_now();
        if !enabled {
            return;
        }
        let Some(catalog) = self.catalog() else { return };
        for g in &catalog.games {
            if self.load_record(&g.id).is_none() {
                continue;
            }
            let Ok(root) = self.game_root(g) else { continue };
            let newest = g
                .crash_globs
                .iter()
                .flat_map(|pat| glob_files(&expand(pat, &root, &g.target)))
                .filter_map(|p| p.metadata().ok()?.modified().ok())
                .filter_map(|t| t.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs()))
                .max();
            if since > 0 && newest.map(|t| t > since).unwrap_or(false) {
                log(format!("new crash dump for {}", g.id));
                let _ = self.report(Some(&g.id), "auto: crash dump found").await;
            }
        }
        let mut s = self.settings.lock().unwrap();
        s.last_crash_scan = now;
        let _ = s.save(&self.paths.settings());
    }

    pub async fn report(&self, id: Option<&str>, note: &str) -> Result<String, String> {
        let entry = id.and_then(|id| self.entry(id).ok());
        let root = entry.as_ref().and_then(|g| self.game_root(g).ok());
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();

        let meta = json!({
            "app": self.app_version,
            "game": id,
            "note": note,
            "time": util::now_rfc3339(),
            "os": std::env::consts::OS,
            "steamRoot": engine::steam::steam_root(),
            "gamePath": root,
            "settings": self.settings.lock().unwrap().public(),
            "view": self.view(),
        });
        files.push(("report.json".into(), serde_json::to_vec_pretty(&meta).unwrap_or_default()));
        if let Some(p) = util::log_path() {
            files.push(("app.log".into(), tail(p, 2_000_000)));
        }
        if let (Some(g), Some(root)) = (&entry, &root) {
            if let Ok(b) = std::fs::read(self.paths.record(&g.id)) {
                files.push(("install-record.json".into(), b));
            }
            for pat in &g.logs {
                let p = expand(pat, root, &g.target);
                if p.is_file() {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    files.push((format!("logs/{name}"), tail(&p, 2_000_000)));
                }
            }
        }
        let body = crate::report::zip_files(&files)?;
        let res = self
            .http
            .post(format!("{}/reports", self.base_url))
            .header("content-type", "application/zip")
            .header("x-game", id.unwrap_or("app"))
            .header("x-app-version", &self.app_version)
            .header("x-note", urlencoding::encode(&note.chars().take(500).collect::<String>()).to_string())
            .timeout(Duration::from_secs(60))
            .body(body)
            .send()
            .await
            .map_err(|e| format!("Couldn't send: {e}"))?;
        if !res.status().is_success() {
            return Err(format!("Couldn't send: HTTP {}", res.status()));
        }
        let v: Value = res.json().await.unwrap_or(Value::Null);
        let rid = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        log(format!("report sent {rid}"));
        Ok(rid)
    }
}

#[derive(Serialize, Clone, Default)]
pub struct ArtView {
    pub hero: Option<String>,
    pub logo: Option<String>,
}

impl Core {
    /// Game art as data URLs: cached copy, else the Worker, else the player's own Steam library cache.
    pub async fn art(&self, id: &str) -> ArtView {
        let Ok(g) = self.entry(id) else { return ArtView::default() };
        let art = g.art.clone().unwrap_or_default();
        let mut view = ArtView::default();
        for (kind, url, steam_name) in [("hero", art.hero, "library_hero.jpg"), ("logo", art.logo, "logo.png")] {
            let bytes = match url {
                Some(u) => self.cached_art(id, kind, &u).await,
                None => None,
            }
            .or_else(|| steam_cache_art(g.steam_app_id, steam_name));
            let Some(bytes) = bytes else { continue };
            let mime = if bytes.starts_with(b"\x89PNG") { "image/png" } else { "image/jpeg" };
            use base64::Engine;
            let url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes));
            match kind {
                "hero" => view.hero = Some(url),
                _ => view.logo = Some(url),
            }
        }
        view
    }

    async fn cached_art(&self, id: &str, kind: &str, url: &str) -> Option<Vec<u8>> {
        let dir = self.paths.data.join("art").join(id);
        let tag = &hex::encode(Sha256::digest(url.as_bytes()))[..12];
        let file = dir.join(format!("{kind}-{tag}"));
        if let Ok(b) = std::fs::read(&file) {
            return Some(b);
        }
        let res = self.http.get(resolve_url(&self.base_url, url)).timeout(Duration::from_secs(20)).send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        let bytes = res.bytes().await.ok()?.to_vec();
        let _ = std::fs::create_dir_all(&dir);
        // Replace older art of this kind.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().starts_with(&format!("{kind}-")) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let _ = std::fs::write(&file, &bytes);
        Some(bytes)
    }
}

/// Steam keeps library art in `appcache/librarycache/<appid>/` (newer clients add a hash subfolder).
fn steam_cache_art(app_id: u32, name: &str) -> Option<Vec<u8>> {
    let dir = engine::steam::steam_root()?.join("appcache").join("librarycache").join(app_id.to_string());
    let direct = dir.join(name);
    if direct.is_file() {
        return std::fs::read(direct).ok();
    }
    std::fs::read_dir(&dir).ok()?.flatten().map(|e| e.path().join(name)).find(|p| p.is_file()).and_then(|p| std::fs::read(p).ok())
}

/// `{game}`, `{target}`, `{localappdata}` and `{appdata}` in catalog paths.
fn expand(pat: &str, root: &Path, target: &str) -> PathBuf {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    let s = pat
        .replace("{target}", &root.join(target).display().to_string())
        .replace("{game}", &root.display().to_string())
        .replace("{localappdata}", &env("LOCALAPPDATA"))
        .replace("{appdata}", &env("APPDATA"));
    PathBuf::from(s)
}

/// Files matching a pattern whose wildcards are only in the last segment.
fn glob_files(pattern: &Path) -> Vec<PathBuf> {
    let (Some(dir), Some(name)) = (pattern.parent(), pattern.file_name()) else { return vec![] };
    let Ok(glob) = globset::GlobBuilder::new(&name.to_string_lossy()).case_insensitive(true).build() else {
        return vec![];
    };
    let m = glob.compile_matcher();
    std::fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.file_name().map(|n| m.is_match(n)).unwrap_or(false)).collect())
        .unwrap_or_default()
}

/// The last `max` bytes of a file, read with sharing so a running game's log works.
fn tail(path: &Path, max: u64) -> Vec<u8> {
    use std::io::{Read, Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(path) else { return vec![] };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len > max {
        let _ = f.seek(SeekFrom::Start(len - max));
    }
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    buf
}
