use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// Which of `names` (exe file names like `Vein.exe`, case-insensitive) are
/// currently running. Returns the matching entries of `names`, deduplicated.
pub fn running_processes(names: &[String]) -> Vec<String> {
    if names.is_empty() {
        return Vec::new();
    }
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let running: std::collections::HashSet<String> = sys
        .processes()
        .values()
        .map(|p| p.name().to_string_lossy().to_lowercase())
        .collect();
    let mut out: Vec<String> = Vec::new();
    for n in names {
        let lower = n.to_lowercase();
        if running.contains(&lower) && !out.iter().any(|o| o.to_lowercase() == lower) {
            out.push(n.clone());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_own_process_case_insensitively() {
        let exe = std::env::current_exe().unwrap();
        let name = exe.file_name().unwrap().to_string_lossy().to_uppercase();
        let found = running_processes(&[name.clone(), "definitely-not-running-xyz.exe".into()]);
        assert_eq!(found, vec![name]);
        assert!(running_processes(&[]).is_empty());
    }
}
