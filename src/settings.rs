//! Player preferences, kept apart from the save so starting a new game never
//! resets them and a bad settings file never touches progress.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What the ESC menu toggles.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub music: bool,
    pub sound: bool,
    pub shake: bool,
    pub fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            music: true,
            sound: true,
            shake: true,
            fullscreen: false,
        }
    }
}

/// `settings.json` beside the save file.
pub fn path_beside(save: &Path) -> PathBuf {
    save.with_file_name("settings.json")
}

/// Missing or unreadable settings fall back to defaults; they're preferences, not progress.
pub fn load(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A settings file from an older build, missing newer keys, keeps what it has.
    #[test]
    fn missing_keys_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"music": false}"#).unwrap();
        assert_eq!(
            s,
            Settings {
                music: false,
                ..Settings::default()
            }
        );
    }

    /// A garbled or absent file means defaults, never a crash.
    #[test]
    fn unreadable_file_means_defaults() {
        let dir = std::env::temp_dir().join(format!("gemdigger-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        assert_eq!(load(&path), Settings::default());
        std::fs::write(&path, b"{nope").unwrap();
        assert_eq!(load(&path), Settings::default());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
