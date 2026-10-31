use iced::{theme::Palette, Color, Theme};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Raw {
    background: String,
    text: String,
    primary: String,
    success: String,
    warning: String,
    danger: String,
}

const BUILTIN: [(&str, &str); 4] = [
    ("sharu-dark", r##"background="#0b120e"
text="#e6f0e9"
primary="#3fb950"
success="#3fb950"
warning="#d29922"
danger="#f85149""##),
    ("github-dark", r##"background="#0d1117"
text="#e6edf3"
primary="#1f6feb"
success="#3fb950"
warning="#d29922"
danger="#f85149""##),
    ("github-dark-dimmed", r##"background="#22272e"
text="#adbac7"
primary="#539bf5"
success="#57ab5a"
warning="#c69026"
danger="#e5534b""##),
    ("github-light", r##"background="#ffffff"
text="#1f2328"
primary="#0969da"
success="#1a7f37"
warning="#9a6700"
danger="#d1242f""##),
];

fn hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some(Color::from_rgb8((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn build(name: &str, src: &str) -> Option<Theme> {
    let r: Raw = toml::from_str(src).ok()?;
    Some(Theme::custom(
        name.to_string(),
        Palette {
            background: hex(&r.background)?,
            text: hex(&r.text)?,
            primary: hex(&r.primary)?,
            success: hex(&r.success)?,
            warning: hex(&r.warning)?,
            danger: hex(&r.danger)?,
        },
    ))
}

pub fn dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("readru/themes"))
}

/// Built-ins first, then ~/.config/readru/themes/*.toml (same name overrides).
pub fn load_all() -> Vec<(String, Theme)> {
    let mut v: Vec<(String, Theme)> = BUILTIN
        .iter()
        .filter_map(|(n, s)| build(n, s).map(|t| (n.to_string(), t)))
        .collect();
    if let Some(d) = dir() {
        let _ = std::fs::create_dir_all(&d);
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("toml") {
                    continue;
                }
                let Some(name) = p.file_stem().and_then(|s| s.to_str()) else { continue };
                if let Some(t) = std::fs::read_to_string(&p).ok().and_then(|s| build(name, &s)) {
                    v.retain(|(n, _)| n != name);
                    v.push((name.to_string(), t));
                }
            }
        }
    }
    v
}
