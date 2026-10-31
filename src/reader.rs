use crate::config::{Config, PageMode, ReaderFont};
use crate::{pill, soft};
use iced::widget::{button, column, container, responsive, row, text, Row};
use iced::{font, Element, Font, Length, Size};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub enum Msg {
    Next,
    Prev,
    Close,
    Settings,
}

pub struct State {
    pub title: String,
    path: PathBuf,
    text: String,
    pos: usize, // byte offset of the current page start: survives font/size/window changes
    error: Option<String>,
    cap: Cell<usize>,  // chars per page, set by the last render
    span: Cell<usize>, // bytes shown by the last render
}

// ---------- progress ----------

#[derive(Serialize, Deserialize, Default)]
struct Progress {
    pos: BTreeMap<String, usize>,
}

fn progress_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("readru/progress.toml"))
}

fn load_progress() -> Progress {
    progress_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_pos(path: &Path, pos: usize) {
    let mut p = load_progress();
    p.pos.insert(path.to_string_lossy().into_owned(), pos);
    if let Some(f) = progress_file() {
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(s) = toml::to_string_pretty(&p) {
            let _ = std::fs::write(f, s);
        }
    }
}

// ---------- loading ----------

fn read_entry(z: &mut zip::ZipArchive<File>, name: &str) -> Option<String> {
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s.trim_start_matches('\u{feff}').to_string())
}

fn epub_text(path: &Path) -> Option<String> {
    let mut z = zip::ZipArchive::new(File::open(path).ok()?).ok()?;
    let container = read_entry(&mut z, "META-INF/container.xml")?;
    let opf_path = roxmltree::Document::parse(&container)
        .ok()?
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))?
        .attribute("full-path")?
        .to_string();
    let opf = read_entry(&mut z, &opf_path)?;
    let doc = roxmltree::Document::parse(&opf).ok()?;
    let manifest: BTreeMap<String, String> = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
        .filter_map(|n| {
            Some((
                n.attribute("id")?.to_string(),
                n.attribute("href")?.to_string(),
            ))
        })
        .collect();
    let dir = opf_path
        .rsplit_once('/')
        .map(|(d, _)| format!("{d}/"))
        .unwrap_or_default();
    let mut out = String::new();
    for it in doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "itemref")
    {
        let Some(href) = it.attribute("idref").and_then(|i| manifest.get(i)) else {
            continue;
        };
        let full = format!("{dir}{}", href.replace("%20", " "));
        if let Some(h) = read_entry(&mut z, &full) {
            out.push_str(&strip_html(&h));
            out.push_str("\n\n");
        }
    }
    Some(out)
}

fn load(path: &Path) -> Result<String, String> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("txt") => std::fs::read(path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| e.to_string()),
        Some("epub") => epub_text(path)
            .filter(|t| !t.trim().is_empty())
            .ok_or_else(|| "Could not read this EPUB".into()),
        Some("pdf") => Err("PDF reading arrives with the MuPDF step.".into()),
        _ => Err("Unsupported format".into()),
    }
}

fn push_ws(out: &mut String) {
    if !out.is_empty() && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
}

fn push_break(out: &mut String) {
    if out.is_empty() {
        return;
    }
    while out.ends_with(' ') {
        out.pop();
    }
    if !out.ends_with("\n\n") {
        out.push_str("\n\n");
    }
}

fn decode_entity(e: &str) -> Option<String> {
    let s = match e {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => " ",
        _ => {
            let n = e.strip_prefix('#')?;
            let code = match n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            return char::from_u32(code).map(|c| c.to_string());
        }
    };
    Some(s.to_string())
}

/// Crude XHTML -> plain text: paragraphs become blank-line separated blocks.
pub fn strip_html(h: &str) -> String {
    let mut out = String::with_capacity(h.len() / 2);
    let mut chars = h.chars().peekable();
    let mut skip: Option<&'static str> = None;
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for t in chars.by_ref() {
                if t == '>' {
                    break;
                }
                tag.push(t);
            }
            let closing = tag.starts_with('/');
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_lowercase();
            if let Some(s) = skip {
                if closing && name == s {
                    skip = None;
                }
                continue;
            }
            match name.as_str() {
                "script" if !closing => skip = Some("script"),
                "style" if !closing => skip = Some("style"),
                "head" if !closing => skip = Some("head"),
                "br" => out.push('\n'),
                "p" | "div" | "li" | "tr" | "blockquote" | "section" if closing => {
                    push_break(&mut out)
                }
                n if closing
                    && n.len() == 2
                    && n.starts_with('h')
                    && n.as_bytes()[1].is_ascii_digit() =>
                {
                    push_break(&mut out)
                }
                _ => {}
            }
        } else if skip.is_none() {
            if c == '&' {
                let mut ent = String::new();
                let mut ok = false;
                while let Some(&n) = chars.peek() {
                    if n == ';' {
                        chars.next();
                        ok = true;
                        break;
                    }
                    if ent.len() > 8 || n.is_whitespace() || n == '&' || n == '<' {
                        break;
                    }
                    ent.push(n);
                    chars.next();
                }
                match ok.then(|| decode_entity(&ent)).flatten() {
                    Some(s) => {
                        for ch in s.chars() {
                            if ch.is_whitespace() {
                                push_ws(&mut out)
                            } else {
                                out.push(ch)
                            }
                        }
                    }
                    None => {
                        out.push('&');
                        out.push_str(&ent);
                        if ok {
                            out.push(';');
                        }
                    }
                }
            } else if c.is_whitespace() {
                push_ws(&mut out);
            } else {
                out.push(c);
            }
        }
    }
    out
}

// ---------- pagination ----------

/// End byte offset of the page that starts at `start` and holds about `cap` chars.
pub fn page_end(t: &str, start: usize, cap: usize) -> usize {
    let rest = &t[start..];
    let mut end = start
        + rest
            .char_indices()
            .nth(cap)
            .map(|(i, _)| i)
            .unwrap_or(rest.len());
    if end < t.len() {
        if let Some(p) = t[start..end].rfind(char::is_whitespace) {
            if p > cap / 2 {
                end = start + p;
            }
        }
    }
    end
}

impl State {
    pub fn open(path: &Path, title: &str) -> Self {
        let (text, error) = match load(path) {
            Ok(t) => (t, None),
            Err(e) => (String::new(), Some(e)),
        };
        let mut pos = load_progress()
            .pos
            .get(path.to_string_lossy().as_ref())
            .copied()
            .unwrap_or(0)
            .min(text.len());
        while !text.is_char_boundary(pos) {
            pos -= 1;
        }
        Self {
            title: title.to_string(),
            path: path.to_path_buf(),
            text,
            pos,
            error,
            cap: Cell::new(0),
            span: Cell::new(0),
        }
    }

    pub fn next(&mut self) {
        let span = self.span.get();
        if span > 0 && self.pos + span < self.text.len() {
            self.pos += span;
            save_pos(&self.path, self.pos);
        }
    }

    pub fn prev(&mut self) {
        if self.pos == 0 {
            return;
        }
        let step = self.span.get().max(200);
        let mut s0 = self.pos.saturating_sub(step);
        while !self.text.is_char_boundary(s0) {
            s0 -= 1;
        }
        let mut s = s0;
        if s > 0 {
            if let Some(p) = self.text[s..].find(char::is_whitespace) {
                s += p;
            }
        }
        self.pos = if s < self.pos { s } else { s0 };
        save_pos(&self.path, self.pos);
    }
}

// ---------- view ----------

pub fn view<'a>(s: &'a State, cfg: &Config) -> Element<'a, Msg> {
    let font = match cfg.reader_font {
        ReaderFont::Serif => Font {
            family: font::Family::Serif,
            ..Font::DEFAULT
        },
        ReaderFont::Sans => Font {
            family: font::Family::SansSerif,
            ..Font::DEFAULT
        },
        ReaderFont::Mono => Font::MONOSPACE,
        ReaderFont::OpenDyslexic => Font::with_name("OpenDyslexic"),
    };
    let sz = cfg.reader_size as f32;
    let cols: usize = if cfg.page_mode == PageMode::Double {
        2
    } else {
        1
    };
    let pct = if s.text.is_empty() {
        0
    } else {
        s.pos * 100 / s.text.len()
    };

    let body: Element<'a, Msg> = if let Some(e) = &s.error {
        container(text(e.as_str())).center(Length::Fill).into()
    } else {
        responsive(move |size: Size| -> Element<'a, Msg> {
            let (gap, pad) = (32.0_f32, 48.0_f32);
            let col_w =
                ((size.width - pad * 2.0 - gap * (cols as f32 - 1.0)) / cols as f32).max(200.0);
            let h = (size.height - pad).max(100.0);
            let per_line = (col_w / (sz * 0.5)).floor();
            let lines = (h / (sz * 1.45)).floor();
            let cap = ((per_line * lines) * 0.8).max(50.0) as usize;

            let mut start = s.pos;
            let mut pages: Vec<Element<'a, Msg>> = Vec::new();
            for _ in 0..cols {
                let end = page_end(&s.text, start, cap);
                let body = s.text[start..end].trim().to_string();
                pages.push(
                    container(text(body).font(font).size(sz).line_height(1.45))
                        .width(col_w)
                        .height(h)
                        .clip(true)
                        .into(),
                );
                start = end;
            }
            s.cap.set(cap);
            s.span.set(start - s.pos);
            container(Row::with_children(pages).spacing(gap))
                .padding(pad / 2.0)
                .center_x(Length::Fill)
                .into()
        })
        .into()
    };

    column![
        row![
            button("← Library").on_press(Msg::Close).style(soft),
            text(s.title.as_str()).size(16).width(Length::Fill),
            button("⚙").on_press(Msg::Settings).style(soft),
        ]
        .spacing(12)
        .padding(12)
        .align_y(iced::Center),
        body,
        row![
            button("‹ Prev").on_press(Msg::Prev).style(soft),
            container(text(format!("{pct}%"))).center_x(Length::Fill),
            button("Next ›").on_press(Msg::Next).style(pill),
        ]
        .spacing(12)
        .padding(12)
        .align_y(iced::Center),
    ]
    .into()
}
