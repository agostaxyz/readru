use crate::library::SortKey;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ReaderFont {
    #[default]
    Serif,
    Sans,
    Mono,
    OpenDyslexic,
}

impl ReaderFont {
    pub const ALL: &'static [ReaderFont] =
        &[ReaderFont::Serif, ReaderFont::Sans, ReaderFont::Mono, ReaderFont::OpenDyslexic];
}

impl fmt::Display for ReaderFont {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            ReaderFont::Serif => "Serif",
            ReaderFont::Sans => "Sans",
            ReaderFont::Mono => "Monospace",
            ReaderFont::OpenDyslexic => "OpenDyslexic",
        })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum PageMode {
    #[default]
    Single,
    Double,
}

impl PageMode {
    pub const ALL: &'static [PageMode] = &[PageMode::Single, PageMode::Double];
}

impl fmt::Display for PageMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            PageMode::Single => "Single page",
            PageMode::Double => "Two pages",
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    pub library: Option<PathBuf>,
    pub theme: String,
    pub sort: SortKey,
    pub ascending: bool,
    pub reader_font: ReaderFont,
    pub reader_size: u16,
    pub page_mode: PageMode,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            library: None,
            theme: "sharu-dark".into(),
            sort: SortKey::Title,
            ascending: true,
            reader_font: ReaderFont::Serif,
            reader_size: 18,
            page_mode: PageMode::Single,
        }
    }
}

fn path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("readru/config.toml"))
}

impl Config {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = path() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(s) = toml::to_string_pretty(self) {
                let _ = std::fs::write(p, s);
            }
        }
    }
}
