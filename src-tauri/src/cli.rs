//! Headless mode for testing and scripting, using the same code paths as the UI.
//!
//!   mod-sync.exe --cli status
//!   mod-sync.exe --cli install vein
//!   mod-sync.exe --cli uninstall vein
//!   mod-sync.exe --cli report vein "note"
//!   mod-sync.exe --cli check-update
//!   mod-sync.exe --cli self-update
//!   add `--out <file>` to also write the output to a file
use crate::core::Core;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Debug)]
pub struct CliCmd {
    pub args: Vec<String>,
    pub out: Option<String>,
}

pub fn parse(args: &[String]) -> Option<CliCmd> {
    let i = args.iter().position(|a| a == "--cli")?;
    let mut rest = Vec::new();
    let mut out = None;
    let mut it = args[i + 1..].iter();
    while let Some(a) = it.next() {
        if a == "--out" {
            out = it.next().cloned();
        } else {
            rest.push(a.clone());
        }
    }
    Some(CliCmd { args: rest, out })
}

fn emit_line(cmd: &CliCmd, line: &str) {
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
    println!("{line}");
    if let Some(path) = &cmd.out {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
}

pub async fn run(app: &AppHandle, core: Arc<Core>, cmd: CliCmd) -> i32 {
    let a: Vec<&str> = cmd.args.iter().map(|s| s.as_str()).collect();
    let result: Result<String, String> = async {
        match a.as_slice() {
            ["status"] => {
                core.refresh_catalog().await;
                serde_json::to_string_pretty(&core.view()).map_err(|e| e.to_string())
            }
            ["install", id] => {
                core.refresh_catalog().await;
                core.install(id).await.map(|v| format!("installed {id} {v}"))
            }
            ["uninstall", id] => {
                core.refresh_catalog().await;
                core.uninstall(id).await.map(|_| format!("uninstalled {id}"))
            }
            ["report", id, note @ ..] => {
                core.refresh_catalog().await;
                core.report(Some(id), &note.join(" ")).await.map(|rid| format!("report {rid}"))
            }
            ["check-update"] => match app.updater().map_err(|e| e.to_string())?.check().await {
                Ok(Some(u)) => Ok(format!("update {}", u.version)),
                Ok(None) => Ok(format!("current {}", core.app_version)),
                Err(e) => Err(e.to_string()),
            },
            ["self-update"] => {
                let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
                match update {
                    Some(u) => {
                        emit_line(&cmd, &format!("installing {}", u.version));
                        u.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
                        Ok(format!("installed {}", u.version))
                    }
                    None => Ok(format!("current {}", core.app_version)),
                }
            }
            ["version"] => Ok(core.app_version.clone()),
            _ => Err("usage: --cli status | install <id> | uninstall <id> | report <id> <note> | check-update | self-update | version".into()),
        }
    }
    .await;
    match result {
        Ok(s) => {
            emit_line(&cmd, &s);
            0
        }
        Err(e) => {
            emit_line(&cmd, &format!("error: {e}"));
            1
        }
    }
}
