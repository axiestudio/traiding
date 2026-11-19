//! App-config `.env` storage.
//!
//! The webview never receives a secret. Rust accepts a key once, validates it,
//! then writes it here so the next launch can connect without asking again.
//! SQLite stays for preferences and market data; this file holds no state other
//! than environment variables, and is created with mode 0600 on Unix.

use std::fs;
use std::path::Path;

use tauri::Manager;

/// Merge `KEY=value` pairs into the app-config `.env`.
///
/// Existing values are replaced, missing keys are appended, and every other
/// line (comments, model settings, the other credential pair) is preserved.
pub(crate) fn save_env(app: &tauri::AppHandle, entries: &[(&str, &str)]) -> Result<(), String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|err| format!("no config directory: {err}"))?;
    fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {}: {err}", dir.display()))?;

    let path = dir.join(".env");
    let existing = fs::read_to_string(&path).unwrap_or_default();
    write_private(&path, &merge_env(&existing, entries))
}

fn merge_env(existing: &str, entries: &[(&str, &str)]) -> String {
    let mut lines: Vec<String> = existing.lines().map(str::to_string).collect();

    for (key, value) in entries {
        // A newline would inject a second variable into the file.
        let value = value.replace('\n', "").replace('\r', "");
        let prefix = format!("{key}=");
        let entry = format!("{prefix}{value}");

        match lines
            .iter_mut()
            .find(|line| line.trim_start().starts_with(&prefix))
        {
            Some(line) => *line = entry,
            None => lines.push(entry),
        }
    }

    let mut body = lines.join("\n");
    body.push('\n');
    body
}

fn write_private(path: &Path, body: &str) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(|err| format!("could not save {}: {err}", path.display()))?;
        // `mode` only applies on create; an existing file may still be 0644.
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|err| format!("could not secure {}: {err}", path.display()))?;
        file.write_all(body.as_bytes())
            .map_err(|err| format!("could not save {}: {err}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        fs::write(path, body)
            .map_err(|err| format!("could not save {}: {err}", path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{merge_env, write_private};

    #[cfg(unix)]
    #[test]
    fn writes_a_private_file() {
        use std::os::unix::fs::PermissionsExt;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir()
            .join(format!("traiding-config-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");

        write_private(&path, "A=1\n").unwrap();

        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "A=1\n");

        // An existing file must be tightened, not only a newly created one.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, "A=2\n").unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn replaces_values_and_keeps_other_lines() {
        let existing = "# model\nOPENROUTER_API_KEY=old\nAPCA_API_KEY_ID=PK123\n";
        let merged = merge_env(
            existing,
            &[
                ("OPENROUTER_API_KEY", "new"),
                ("APCA_API_SECRET_KEY", "s3cret"),
            ],
        );

        assert_eq!(
            merged,
            "# model\nOPENROUTER_API_KEY=new\nAPCA_API_KEY_ID=PK123\nAPCA_API_SECRET_KEY=s3cret\n"
        );
    }

    #[test]
    fn appends_missing_keys() {
        assert_eq!(merge_env("", &[("A", "1"), ("B", "2")]), "A=1\nB=2\n");
    }

    #[test]
    fn strips_newlines_from_values() {
        assert_eq!(merge_env("", &[("A", "1\nB=2")]), "A=1B=2\n");
    }
}
