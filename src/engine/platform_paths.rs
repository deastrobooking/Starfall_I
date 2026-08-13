//! Platform-owned writable paths and bounded diagnostic-file helpers.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const APP_DATA_DIR: &str = "starfall_i";
const CRASH_LOG_PREFIX: &str = "starfall_crash_";
const CRASH_LOG_SUFFIX: &str = ".log";
const MAX_CRASH_LOGS: usize = 5;

/// Runtime asset root. Development keeps the repository `assets/` directory;
/// packaged builds can place `assets/` beside the executable or inject an
/// explicit root without retaining the build machine's manifest path.
pub fn asset_root() -> PathBuf {
    if let Some(path) = std::env::var_os("STARFALL_ASSET_ROOT").filter(|path| !path.is_empty()) {
        return PathBuf::from(path);
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            let adjacent = parent.join("assets");
            if adjacent.is_dir() {
                return adjacent;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// Platform-appropriate writable application directory.
pub fn data_root() -> PathBuf {
    dirs::data_dir()
        .map(|dir| dir.join(APP_DATA_DIR))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Removes local build/install prefixes from a report users may share.
pub fn sanitize_crash_report(report: &str) -> String {
    let mut sanitized = report.replace(env!("CARGO_MANIFEST_DIR"), "<install>");
    if let Some(home) = std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        sanitized = sanitized.replace(home.to_string_lossy().as_ref(), "~");
    }
    sanitized
}

/// Writes one timestamped crash log and retains only the newest bounded set.
pub fn write_crash_report(report: &str) -> io::Result<PathBuf> {
    let root = data_root();
    fs::create_dir_all(&root)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let path = root.join(format!(
        "{CRASH_LOG_PREFIX}{timestamp:020}{CRASH_LOG_SUFFIX}"
    ));
    fs::write(&path, sanitize_crash_report(report))?;
    prune_crash_logs(&root, MAX_CRASH_LOGS)?;
    Ok(path)
}

fn prune_crash_logs(root: &Path, keep: usize) -> io::Result<()> {
    let mut logs = fs::read_dir(root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(CRASH_LOG_PREFIX) && name.ends_with(CRASH_LOG_SUFFIX)
                })
        })
        .collect::<Vec<_>>();
    logs.sort_unstable();
    let remove_count = logs.len().saturating_sub(keep);
    for path in logs.into_iter().take(remove_count) {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_report_sanitizes_project_and_home_paths() {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let report = format!("at {manifest}/src/main.rs");
        let sanitized = sanitize_crash_report(&report);
        assert_eq!(sanitized, "at <install>/src/main.rs");
        assert!(!sanitized.contains(manifest));
    }

    #[test]
    fn crash_log_pruning_keeps_newest_bounded_set() {
        let root = std::env::temp_dir().join(format!(
            "starfall_crash_prune_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        for index in 0..7 {
            fs::write(
                root.join(format!("{CRASH_LOG_PREFIX}{index:020}{CRASH_LOG_SUFFIX}")),
                "test",
            )
            .unwrap();
        }

        prune_crash_logs(&root, 5).unwrap();
        let remaining = fs::read_dir(&root).unwrap().count();
        assert_eq!(remaining, 5);
        assert!(!root
            .join(format!("{CRASH_LOG_PREFIX}{:020}{CRASH_LOG_SUFFIX}", 0))
            .exists());
        let _ = fs::remove_dir_all(root);
    }
}
