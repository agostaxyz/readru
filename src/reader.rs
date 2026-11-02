use crate::config::{Config, PageMode, ReadDir, ReadMode, ReaderFont};
use crate::soft;
use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
use iced::widget::{
    button, column, container, image, keyed, mouse_area, responsive, row, scrollable, stack, text,
    Row,
};
use iced::{font, Border, Color, Element, Font, Length, Shadow, Size, Theme, Vector};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Msg {
    Next,  // logical: PageDown
    Prev,  // logical: PageUp
    Left,  // physical: left click zone / left arrow
    Right, // physical: right click zone / right arrow
    Close,
    Settings,
    ToggleDir,
    Scrolled(bool), // scroll mode: reached the bottom of the chunk
}

pub struct State {
    pub title: String,
    path: PathBuf,
    text: String,
    pos: usize, // byte offset of the current page start: survives font/size/window changes
    error: Option<String>,
    cap: Cell<usize>,  // chars per page, set by the last render
    span: Cell<usize>, // bytes (text) or pages (PDF) shown by the last render
    pdf: Option<Pdf>,  // set for PDFs: `pos` is then a page index and `text` stays empty
    spread: RefCell<Option<Spread>>,
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
        _ => Err("Unsupported format".into()),
    }
}

fn is_pdf(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

fn load_pdf(path: &Path) -> Result<Pdf, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let pdf = Pdf::new(Arc::new(bytes)).map_err(|_| "Could not read this PDF".to_string())?;
    if pdf.pages().is_empty() {
        return Err("This PDF has no pages".into());
    }
    Ok(pdf)
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
        let (text, pdf, error) = if is_pdf(path) {
            match load_pdf(path) {
                Ok(p) => (String::new(), Some(p), None),
                Err(e) => (String::new(), None, Some(e)),
            }
        } else {
            match load(path) {
                Ok(t) => (t, None, None),
                Err(e) => (String::new(), None, Some(e)),
            }
        };
        let limit = pdf
            .as_ref()
            .map_or(text.len(), |p| p.pages().len().saturating_sub(1));
        let mut pos = load_progress()
            .pos
            .get(path.to_string_lossy().as_ref())
            .copied()
            .unwrap_or(0)
            .min(limit);
        if pdf.is_none() {
            while !text.is_char_boundary(pos) {
                pos -= 1;
            }
        }
        Self {
            title: title.to_string(),
            path: path.to_path_buf(),
            text,
            pos,
            error,
            cap: Cell::new(0),
            span: Cell::new(0),
            pdf,
            spread: RefCell::new(None),
        }
    }

    fn percent(&self) -> usize {
        match &self.pdf {
            Some(p) => {
                let n = p.pages().len().max(1);
                (self.pos + self.span.get()).min(n) * 100 / n
            }
            None if self.text.is_empty() => 0,
            None => self.pos * 100 / self.text.len(),
        }
    }

    pub fn next(&mut self) {
        let span = self.span.get();
        let end = self
            .pdf
            .as_ref()
            .map_or(self.text.len(), |p| p.pages().len());
        if span > 0 && self.pos + span < end {
            self.pos += span;
            save_pos(&self.path, self.pos);
        }
    }

    pub fn prev(&mut self) {
        if self.pos == 0 {
            return;
        }
        if self.pdf.is_some() {
            self.pos = self.pos.saturating_sub(self.span.get().max(1));
            save_pos(&self.path, self.pos);
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

/// A-series paper (A4, A5, ...) is always 1 : sqrt(2)
const RATIO: f32 = std::f32::consts::SQRT_2;

fn paper(t: &Theme) -> container::Style {
    let p = t.extended_palette();
    container::Style {
        background: Some(p.background.weak.color.into()),
        border: Border {
            color: p.background.strong.color,
            width: 1.0,
            radius: 6.0_f32.into(),
        },
        text_color: Some(p.background.weak.text),
        shadow: Shadow {
            color: Color {
                a: 0.4,
                ..Color::BLACK
            },
            offset: Vector::new(0.0, 8.0),
            blur_radius: 28.0,
        },
        ..Default::default()
    }
}

/// PDF pages are rasterised at this multiple of their on-screen size: crisp on HiDPI,
/// smoothly downscaled on 1x screens.
const SUPERSAMPLE: f32 = 2.0;

/// Rasterised pages of the current spread; re-rendered only when the pages or the fit step change.
struct Spread {
    key: (usize, usize, u32),
    handles: Vec<image::Handle>,
}

fn render_page(pdf: &Pdf, index: usize, scale: f32) -> image::Handle {
    let pixmap = render(
        &pdf.pages()[index],
        &RenderCache::new(),
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    image::Handle::from_rgba(
        pixmap.width() as u32,
        pixmap.height() as u32,
        pixmap.data_as_u8_slice().to_vec(),
    )
}

/// PDF pages sit edge to edge (no padding, no spine) so double-page art stays continuous.
fn pdf_spread<'a>(s: &State, pdf: &Pdf, size: Size, cols: usize, rtl: bool) -> Element<'a, Msg> {
    let pages = pdf.pages();
    let first = s.pos.min(pages.len() - 1);
    let shown = cols.min(pages.len() - first);
    let dims: Vec<(f32, f32)> = (first..first + shown)
        .map(|i| pages[i].render_dimensions())
        .collect();
    let total_w = dims.iter().map(|d| d.0).sum::<f32>().max(1.0);
    let max_h = dims.iter().map(|d| d.1).fold(1.0_f32, f32::max);
    let margin = 24.0_f32;
    let fit = ((size.width - margin * 2.0) / total_w)
        .min((size.height - margin * 2.0) / max_h)
        .max(0.05);

    // render scale moves in 1/8 steps so dragging the window edge doesn't re-render every frame
    let step = (fit * SUPERSAMPLE * 8.0).ceil() as u32;
    let key = (first, shown, step);
    let mut cache = s.spread.borrow_mut();
    if cache.as_ref().map(|c| c.key) != Some(key) {
        let scale = step as f32 / 8.0;
        let handles = (first..first + shown)
            .map(|i| render_page(pdf, i, scale))
            .collect();
        *cache = Some(Spread { key, handles });
    }
    s.span.set(shown);

    let mut items: Vec<(image::Handle, f32, f32)> = cache
        .iter()
        .flat_map(|c| c.handles.iter().cloned())
        .zip(&dims)
        .map(|(h, d)| (h, d.0 * fit, d.1 * fit))
        .collect();
    if rtl {
        items.reverse();
    }
    let spread_row = Row::with_children(
        items
            .into_iter()
            .map(|(h, w, ht)| Element::from(image(h).width(w).height(ht))),
    );
    container(container(spread_row).clip(true).style(paper))
        .center(Length::Fill)
        .into()
}

/// Invisible click zones: left 30% = left, right 70% = right.
fn zones<'a>() -> Element<'a, Msg> {
    let zone = |msg: Msg, portion: u16| {
        mouse_area(
            container(text(""))
                .width(Length::FillPortion(portion))
                .height(Length::Fill),
        )
        .on_press(msg)
        .interaction(iced::mouse::Interaction::Pointer)
    };
    row![zone(Msg::Left, 3), zone(Msg::Right, 7)].into()
}

pub fn view<'a>(s: &'a State, cfg: &Config, standalone: bool) -> Element<'a, Msg> {
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
    let rtl = cfg.read_dir == ReadDir::Rtl;
    let scroll = cfg.read_mode == ReadMode::Scroll && s.pdf.is_none();
    let cols: usize = if cfg.page_mode == PageMode::Double && !scroll {
        2
    } else {
        1
    };
    let pct = s.percent();

    let header = row![
        button(if standalone {
            "✕ Close"
        } else {
            "← Library"
        })
        .on_press(Msg::Close)
        .style(soft),
        text(s.title.as_str()).size(16).width(Length::Fill),
        text(format!("{pct}%")).size(14),
        button(if rtl { "R→L" } else { "L→R" })
            .on_press(Msg::ToggleDir)
            .style(soft),
        button("⚙").on_press(Msg::Settings).style(soft),
    ]
    .spacing(12)
    .padding(12)
    .align_y(iced::Center);

    let body: Element<'a, Msg> = if let Some(e) = &s.error {
        container(text(e.as_str())).center(Length::Fill).into()
    } else {
        let book: Element<'a, Msg> = responsive(move |size: Size| -> Element<'a, Msg> {
            if scroll {
                // vertical scrolling: ~8k-char chunks; hitting the bottom loads the next one
                let sheet_pad = 32.0_f32;
                let col_w = (size.width - 48.0 - sheet_pad * 2.0).clamp(200.0, 720.0);
                let end = page_end(&s.text, s.pos, 8000);
                s.cap.set(8000);
                s.span.set(end - s.pos);
                let chunk = s.text[s.pos..end].trim().to_string();
                let sheet = container(
                    text(chunk)
                        .font(font)
                        .size(sz)
                        .line_height(1.6)
                        .width(col_w),
                )
                .padding(sheet_pad)
                .style(paper);
                let content = container(sheet).padding(24).center_x(Length::Fill);
                let sc = scrollable(content)
                    .on_scroll(|v| {
                        Msg::Scrolled(
                            v.absolute_offset().y + v.bounds().height
                                >= v.content_bounds().height - 4.0,
                        )
                    })
                    .width(Length::Fill)
                    .height(Length::Fill);
                // keyed by position: a new chunk gets a fresh scrollable (back at the top)
                return keyed::Column::with_children(vec![(s.pos, Element::from(sc))])
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into();
            }

            if let Some(pdf) = &s.pdf {
                return pdf_spread(s, pdf, size, cols, rtl);
            }

            let margin = 24.0_f32;
            let avail_w = (size.width - margin * 2.0).max(200.0);
            let avail_h = (size.height - margin * 2.0).max(200.0);
            let n = cols as f32;
            let (mut page_w, mut page_h) = (avail_h / RATIO, avail_h);
            if page_w * n > avail_w {
                page_w = avail_w / n;
                page_h = page_w * RATIO;
            }
            let pad = page_w * 0.1;
            let (tw, th) = (
                (page_w - pad * 2.0).max(100.0),
                (page_h - pad * 2.0).max(100.0),
            );
            let cap =
                (((tw / (sz * 0.5)).floor() * (th / (sz * 1.45)).floor()) * 0.8).max(50.0) as usize;

            let mut start = s.pos;
            let mut texts: Vec<String> = Vec::new();
            for _ in 0..cols {
                let end = page_end(&s.text, start, cap);
                texts.push(s.text[start..end].trim().to_string());
                start = end;
            }
            s.cap.set(cap);
            s.span.set(start - s.pos);
            if rtl {
                texts.reverse(); // manga: first page on the right
            }

            let mut spread: Vec<Element<'a, Msg>> = Vec::new();
            for (i, body) in texts.into_iter().enumerate() {
                if i > 0 {
                    // spine
                    spread.push(
                        container(text(""))
                            .width(1.0)
                            .height(page_h)
                            .style(|t: &Theme| container::Style {
                                background: Some(
                                    t.extended_palette().background.strong.color.into(),
                                ),
                                ..Default::default()
                            })
                            .into(),
                    );
                }
                spread.push(
                    container(
                        container(text(body).font(font).size(sz).line_height(1.45))
                            .width(tw)
                            .height(th)
                            .clip(true),
                    )
                    .padding(pad)
                    .width(page_w)
                    .height(page_h)
                    .into(),
                );
            }
            container(container(Row::with_children(spread)).style(paper))
                .center(Length::Fill)
                .into()
        })
        .into();
        if scroll {
            book
        } else {
            stack![book, zones()].into()
        }
    };

    column![header, body].into()
}
