//! Auto-save to one JSON file per user. Writes land in a temp file that is
//! renamed into place, so a crash or power cut mid-save can't eat progress.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::game::SaveData;

/// `%APPDATA%\GemDigger\save.json` on Windows, the platform data dir elsewhere.
/// `GEMDIGGER_SAVE` points it somewhere else, for testing without touching a real save.
pub fn default_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GEMDIGGER_SAVE") {
        return Some(PathBuf::from(p));
    }
    dirs::data_dir().map(|d| d.join("GemDigger").join("save.json"))
}

/// What was found on disk at startup.
#[derive(Debug)]
pub enum Loaded {
    Fresh,
    Resumed(SaveData),
    /// The save didn't parse; it was moved aside to `backup` instead of overwritten.
    Corrupt {
        backup: PathBuf,
    },
}

pub fn load(path: &Path) -> anyhow::Result<Loaded> {
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Loaded::Fresh),
        Err(e) => {
            return Err(e).with_context(|| format!("unable to read save \"{}\"", path.display()));
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(data) => Ok(Loaded::Resumed(data)),
        Err(_) => {
            let backup = path.with_extension("json.corrupt");
            fs::rename(path, &backup).with_context(|| {
                format!("unable to move corrupt save to \"{}\"", backup.display())
            })?;
            Ok(Loaded::Corrupt { backup })
        }
    }
}

pub fn write(path: &Path, data: &SaveData) -> anyhow::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("unable to create \"{}\"", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_vec(data).context("unable to encode save")?;
    fs::write(&tmp, json).with_context(|| format!("unable to write \"{}\"", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("unable to replace \"{}\"", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gemdigger-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// A missing save means a new game, not an error.
    #[test]
    fn missing_save_starts_fresh() {
        let dir = temp_dir("missing");
        assert!(matches!(
            load(&dir.join("save.json")).unwrap(),
            Loaded::Fresh
        ));
    }

    /// What's written is what's read back, including into a new directory.
    #[test]
    fn write_then_load_round_trips() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("nested").join("save.json");
        let data = Game::new(99).to_save();
        write(&path, &data).unwrap();
        let Loaded::Resumed(back) = load(&path).unwrap() else {
            panic!("save did not load");
        };
        assert_eq!(back, data);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "temp file left behind"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    /// A garbled save is kept aside, never silently overwritten.
    #[test]
    fn corrupt_save_is_backed_up() {
        let dir = temp_dir("corrupt");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("save.json");
        fs::write(&path, b"{not json").unwrap();
        let Loaded::Corrupt { backup } = load(&path).unwrap() else {
            panic!("corrupt save not detected");
        };
        assert_eq!(fs::read(&backup).unwrap(), b"{not json");
        assert!(!path.exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
