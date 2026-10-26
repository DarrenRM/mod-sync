mod catalog;
mod cli;
mod core;
mod report;
mod settings;
mod util;

use crate::core::{AppStateView, Core, Emit};
use crate::settings::PublicSettings;
use std::sync::Arc;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_opener::OpenerExt;

/// Worker URL baked in at build time (`MODSYNC_BASE_URL`); the same env var overrides it at runtime.
const BUILT_BASE_URL: &str = match option_env!("MODSYNC_BASE_URL") {
    Some(v) => v,
    None => "https://mod-sync.example.invalid",
};

fn base_url() -> String {
    std::env::var("MODSYNC_BASE_URL").unwrap_or_else(|_| BUILT_BASE_URL.to_string()).trim_end_matches('/').to_string()
}

type CoreState<'a> = State<'a, Arc<Core>>;

#[tauri::command]
async fn get_state(core: CoreState<'_>, refresh: bool) -> Result<AppStateView, String> {
    if refresh {
        core.refresh_catalog().await;
    }
    Ok(core.view())
}

#[tauri::command]
async fn install(core: CoreState<'_>, id: String) -> Result<AppStateView, String> {
    let before = core.load_record(&id).map(|r| r.version);
    let version = core.install(&id).await?;
    let name = core.view().games.into_iter().find(|g| g.id == id).map(|g| g.name).unwrap_or(id);
    core.toast(match before {
        Some(_) => format!("{name} updated to {version}"),
        None => format!("{name} mods installed"),
    });
    Ok(core.view())
}

#[tauri::command]
async fn uninstall(core: CoreState<'_>, id: String) -> Result<AppStateView, String> {
    core.uninstall(&id).await?;
    let name = core.view().games.into_iter().find(|g| g.id == id).map(|g| g.name).unwrap_or(id);
    core.toast(format!("{name} is back to vanilla"));
    Ok(core.view())
}

#[tauri::command]
async fn play(app: AppHandle, core: CoreState<'_>, id: String) -> Result<(), String> {
    let appid = core.steam_app_id(&id)?;
    app.opener().open_url(format!("steam://rungameid/{appid}"), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_on_steam(app: AppHandle, core: CoreState<'_>, id: String) -> Result<(), String> {
    let appid = core.steam_app_id(&id)?;
    let url = if modsync_engine::steam::steam_root().is_some() {
        format!("steam://install/{appid}")
    } else {
        format!("https://store.steampowered.com/app/{appid}")
    };
    app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
async fn locate(core: CoreState<'_>, id: String, path: Option<String>) -> Result<AppStateView, String> {
    if let Some(p) = path {
        core.locate(&id, &p)?;
    }
    Ok(core.view())
}

#[tauri::command]
async fn open_folder(app: AppHandle, core: CoreState<'_>, id: String) -> Result<(), String> {
    let dir = core.game_dir(&id)?;
    app.opener().open_path(dir.display().to_string(), None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_art(core: CoreState<'_>, id: String) -> Result<crate::core::ArtView, String> {
    Ok(core.art(&id).await)
}

#[tauri::command]
async fn send_report(core: CoreState<'_>, id: Option<String>, note: String) -> Result<(), String> {
    core.report(id.as_deref(), &note).await.map(|_| ())
}

#[tauri::command]
async fn set_settings(app: AppHandle, core: CoreState<'_>, settings: PublicSettings) -> Result<AppStateView, String> {
    let autostart = app.autolaunch();
    let result = if settings.background { autostart.enable() } else { autostart.disable() };
    if let Err(e) = result {
        util::log(format!("autostart: {e}"));
    }
    core.set_settings(settings)?;
    Ok(core.view())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Keeps mods current while the app runs: right away, then every 30 minutes.
fn spawn_sync_loop(app: AppHandle, core: Arc<Core>) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            core.refresh_catalog().await;
            let updated = core.auto_update().await;
            if !updated.is_empty() {
                core.toast(format!("Updated {}", updated.join(", ")));
            }
            let _ = app.emit("state", core.view());
            core.scan_crashes().await;
            tokio::time::sleep(Duration::from_secs(30 * 60)).await;
        }
    });
}

fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("Mod Sync")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let cli_cmd = cli::parse(&args);
    let start_hidden = args.iter().any(|a| a == "--background");

    let mut builder = tauri::Builder::default();
    if cli_cmd.is_none() {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)));
    }
    builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .setup(move |app| {
            let data = match std::env::var("MODSYNC_DATA_DIR") {
                Ok(d) => d.into(),
                Err(_) => app.path().app_data_dir()?,
            };
            let handle = app.handle().clone();
            let emit: Emit = Arc::new(move |event, payload| {
                let _ = handle.emit(event, payload);
            });
            let version = app.package_info().version.to_string();
            let core = Arc::new(Core::new(data, base_url(), version, emit));
            app.manage(core.clone());

            if let Some(cmd) = cli_cmd.clone() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let code = cli::run(&handle, core, cmd).await;
                    handle.exit(code);
                });
                return Ok(());
            }

            setup_tray(app.handle())?;
            if !start_hidden {
                show_main(app.handle());
            }
            spawn_sync_loop(app.handle().clone(), core);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let core = window.state::<Arc<Core>>();
                if core.settings.lock().unwrap().background {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            install,
            uninstall,
            play,
            get_on_steam,
            locate,
            open_folder,
            send_report,
            get_art,
            set_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running Mod Sync");
}
