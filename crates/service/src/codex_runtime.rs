use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{Signal, System};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexRuntimeReloadResult {
    pub requested: bool,
    pub matched_process_count: usize,
    pub signaled_process_count: usize,
    pub warnings: Vec<String>,
    pub message: String,
}

impl CodexRuntimeReloadResult {
    pub(crate) fn skipped() -> Self {
        Self {
            requested: false,
            matched_process_count: 0,
            signaled_process_count: 0,
            warnings: Vec::new(),
            message: "Codex runtime reload was disabled; running clients keep their current configuration until restarted".to_string(),
        }
    }
}

pub(crate) fn reload_codex_app_servers(codex_home: &Path) -> CodexRuntimeReloadResult {
    let system = System::new_all();
    let candidate_pids = matching_app_server_pids(&system, codex_home, |_| true);
    signal_root_processes(
        &system,
        candidate_pids,
        "No matching Codex app-server process was running; new clients will read the updated configuration",
        "Sent a reload signal to {count} Codex app-server process(es); owning clients may restart them",
    )
}

/// Reload only app-server processes that started before the active profile's
/// configuration was written. Codex app-server reads provider capabilities at
/// process start, so an already-running process can keep the old
/// `supports_websockets` value even after Manager repairs `config.toml`.
pub(crate) fn reload_stale_codex_app_servers(
    codex_home: &Path,
    config_path: &Path,
) -> CodexRuntimeReloadResult {
    let config_modified_at = match fs::metadata(config_path)
        .and_then(|metadata| metadata.modified())
    {
        Ok(value) => value,
        Err(err) => {
            return CodexRuntimeReloadResult {
                requested: true,
                matched_process_count: 0,
                signaled_process_count: 0,
                warnings: vec![format!(
                    "could not read Codex profile config mtime at {}: {err}",
                    config_path.display()
                )],
                message: "Skipped stale Codex app-server reload because the profile config timestamp was unavailable".to_string(),
            };
        }
    };
    let Some(config_modified_secs) = unix_timestamp_secs(config_modified_at) else {
        return CodexRuntimeReloadResult {
            requested: true,
            matched_process_count: 0,
            signaled_process_count: 0,
            warnings: vec![format!(
                "profile config mtime at {} predates the Unix epoch",
                config_path.display()
            )],
            message: "Skipped stale Codex app-server reload because the profile config timestamp was invalid".to_string(),
        };
    };

    let system = System::new_all();
    let candidate_pids = matching_app_server_pids(&system, codex_home, |process| {
        process_started_at_or_before_config(process.start_time(), config_modified_secs)
    });
    signal_root_processes(
        &system,
        candidate_pids,
        "No stale Codex app-server process was running; current clients already started after the profile config",
        "Sent a stale-runtime reload signal to {count} Codex app-server process(es); owning clients may restart them",
    )
}

fn matching_app_server_pids<F>(
    system: &System,
    codex_home: &Path,
    mut include: F,
) -> HashSet<sysinfo::Pid>
where
    F: FnMut(&sysinfo::Process) -> bool,
{
    system
        .processes()
        .iter()
        .filter_map(|(pid, process)| {
            if !is_codex_app_server_command(process.cmd()) || !include(process) {
                return None;
            }
            let process_home = resolve_codex_home_from_environment(process.environ())?;
            same_path(&process_home, codex_home).then_some(*pid)
        })
        .collect()
}

fn signal_root_processes(
    system: &System,
    candidate_pids: HashSet<sysinfo::Pid>,
    no_match_message: &str,
    signaled_message_template: &str,
) -> CodexRuntimeReloadResult {
    let root_pids = candidate_pids
        .iter()
        .copied()
        .filter(|pid| {
            system
                .process(*pid)
                .and_then(|process| process.parent())
                .is_none_or(|parent| !candidate_pids.contains(&parent))
        })
        .collect::<Vec<_>>();

    let mut signaled_process_count = 0;
    let mut warnings = Vec::new();
    for pid in root_pids {
        let Some(process) = system.process(pid) else {
            continue;
        };
        let signaled = process
            .kill_with(Signal::Term)
            .unwrap_or_else(|| process.kill());
        if signaled {
            signaled_process_count += 1;
        } else {
            warnings.push(format!(
                "failed to signal Codex app-server process {}",
                pid.as_u32()
            ));
        }
    }

    let matched_process_count = candidate_pids.len();
    let message = if matched_process_count == 0 {
        no_match_message.to_string()
    } else if signaled_process_count == 0 {
        "Matching Codex app-server processes were found, but none accepted the reload signal"
            .to_string()
    } else {
        signaled_message_template.replace("{count}", &signaled_process_count.to_string())
    };

    CodexRuntimeReloadResult {
        requested: true,
        matched_process_count,
        signaled_process_count,
        warnings,
        message,
    }
}

fn unix_timestamp_secs(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

fn process_started_at_or_before_config(process_start_secs: u64, config_modified_secs: u64) -> bool {
    // sysinfo exposes process start time with second precision. Treat an
    // equal-second process as stale too: it may have started before the
    // atomic config replacement but cannot be ordered more precisely here.
    process_start_secs > 0 && process_start_secs <= config_modified_secs
}

fn is_codex_app_server_command(command: &[String]) -> bool {
    if !command.iter().any(|arg| arg == "app-server") {
        return false;
    }
    let Some(first) = command.first() else {
        return false;
    };
    if is_codex_executable(first) {
        return true;
    }
    is_node_executable(first)
        && command
            .get(1)
            .is_some_and(|entry| is_codex_executable(entry))
}

fn is_codex_executable(value: &str) -> bool {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name.to_ascii_lowercase().as_str(),
                "codex" | "codex.exe" | "codex.js"
            )
        })
}

fn is_node_executable(value: &str) -> bool {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name.to_ascii_lowercase().as_str(), "node" | "node.exe"))
}

fn resolve_codex_home_from_environment(environment: &[String]) -> Option<PathBuf> {
    if let Some(value) = environment_value(environment, "CODEX_HOME") {
        return Some(PathBuf::from(value));
    }
    if let Some(value) = environment_value(environment, "USERPROFILE") {
        return Some(PathBuf::from(value).join(".codex"));
    }
    if let Some(value) = environment_value(environment, "HOME") {
        return Some(PathBuf::from(value).join(".codex"));
    }
    let home_drive = environment_value(environment, "HOMEDRIVE").unwrap_or_default();
    let home_path = environment_value(environment, "HOMEPATH").unwrap_or_default();
    let combined = format!("{home_drive}{home_path}");
    (!combined.trim().is_empty()).then(|| PathBuf::from(combined).join(".codex"))
}

fn environment_value(environment: &[String], key: &str) -> Option<String> {
    environment.iter().find_map(|entry| {
        let (candidate, value) = entry.split_once('=')?;
        candidate
            .eq_ignore_ascii_case(key)
            .then(|| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

fn same_path(left: &Path, right: &Path) -> bool {
    normalize_path(left) == normalize_path(right)
}

fn normalize_path(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let value = resolved
        .to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string();
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn app_server_detection_accepts_codex_and_node_wrappers() {
        assert!(is_codex_app_server_command(&strings(&[
            "/usr/bin/codex",
            "-c",
            "feature=true",
            "app-server",
            "--listen",
            "unix://",
        ])));
        assert!(is_codex_app_server_command(&strings(&[
            "node",
            "/home/test/.local/bin/codex",
            "app-server",
            "proxy",
        ])));
    }

    #[test]
    fn app_server_detection_rejects_foreground_cli_and_shell_commands() {
        assert!(!is_codex_app_server_command(&strings(&[
            "/usr/bin/codex",
            "--model",
            "gpt-test",
        ])));
        assert!(!is_codex_app_server_command(&strings(&[
            "/bin/sh",
            "-c",
            "codex app-server proxy",
        ])));
        assert!(!is_codex_app_server_command(&strings(&[
            "/usr/bin/codexmanager-service",
            "app-server",
        ])));
    }

    #[test]
    fn environment_resolution_prefers_explicit_codex_home() {
        let environment = strings(&["HOME=/home/test", "CODEX_HOME=/srv/codex-profile"]);
        assert_eq!(
            resolve_codex_home_from_environment(&environment),
            Some(PathBuf::from("/srv/codex-profile"))
        );
    }

    #[test]
    fn environment_resolution_falls_back_to_home() {
        let environment = strings(&["HOME=/home/test"]);
        assert_eq!(
            resolve_codex_home_from_environment(&environment),
            Some(PathBuf::from("/home/test/.codex"))
        );
    }

    #[test]
    fn stale_runtime_predicate_only_matches_processes_started_at_or_before_config() {
        assert!(process_started_at_or_before_config(1, 2));
        assert!(process_started_at_or_before_config(2, 2));
        assert!(!process_started_at_or_before_config(3, 2));
        assert!(!process_started_at_or_before_config(0, 2));
    }

    #[test]
    fn unix_timestamp_rejects_pre_epoch_values() {
        assert_eq!(unix_timestamp_secs(UNIX_EPOCH), Some(0));
        assert_eq!(
            unix_timestamp_secs(UNIX_EPOCH - std::time::Duration::from_secs(1)),
            None
        );
    }

    #[test]
    fn stale_reload_reports_missing_profile_config_without_signaling() {
        let config_path = std::env::temp_dir().join(format!(
            "codexmanager-missing-profile-config-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&config_path);
        let result = reload_stale_codex_app_servers(Path::new("/tmp/does-not-exist"), &config_path);
        assert_eq!(result.matched_process_count, 0);
        assert_eq!(result.signaled_process_count, 0);
        assert!(result.message.contains("timestamp was unavailable"));
    }
}
