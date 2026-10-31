use crate::meta::{Meta, Store};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use walkdir::WalkDir;

const EXTS: [&str; 3] = ["epub", "pdf", "txt"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortKey {
    #[default]
    Title,
    Author,
    Published,
    Publisher,
    Added,
    Size,
    Format,
}

impl SortKey {
    pub const ALL: &'static [SortKey] = &[
        SortKey::Title,
        SortKey::Author,
        SortKey::Published,
        SortKey::Publisher,
        SortKey::Added,
        SortKey::Size,
        SortKey::Format,
    ];
}

impl fmt::Display for SortKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            SortKey::Title => "Title",
            SortKey::Author => "Author",
            SortKey::Published => "Date published",
            SortKey::Publisher => "Publisher",
            SortKey::Added => "Date added",
            SortKey::Size => "Size",
            SortKey::Format => "Format",
        })
    }
}

#[derive(Debug, Clone)]
pub struct Book {
    pub path: PathBuf,
    pub rel: String, // path relative to library root = sidecar key
    pub title: String,
    pub authors: Vec<String>, // multi-valued on purpose
    pub publisher: Option<String>,
    pub published: Option<String>,
    pub format: String,
    pub size: u64,
    pub added: SystemTime,
}

/// Fallback metadata source: "Karl Marx & Friedrich Engels - The Capital.epub"
fn parse_name(stem: &str) -> (String, Vec<String>) {
    match stem.split_once(" - ") {
        Some((a, t)) => (
            t.trim().to_string(),
            a.split(['&', '+', ';'])
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        ),
        None => (stem.to_string(), vec![]),
    }
}

struct EpubMeta {
    title: Option<String>,
    authors: Vec<String>,
    publisher: Option<String>,
    published: Option<String>,
}

fn read_entry(z: &mut zip::ZipArchive<File>, name: &str) -> Option<String> {
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s.trim_start_matches('\u{feff}').to_string())
}

fn read_epub(path: &Path) -> Option<EpubMeta> {
    let mut z = zip::ZipArchive::new(File::open(path).ok()?).ok()?;
    let container = read_entry(&mut z, "META-INF/container.xml")?;
    let cdoc = roxmltree::Document::parse(&container).ok()?;
    let opf_path = cdoc
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))?
        .attribute("full-path")?
        .to_string();
    let opf = read_entry(&mut z, &opf_path)?;
    let doc = roxmltree::Document::parse(&opf).ok()?;
    let all = |tag: &str| -> Vec<String> {
        doc.descendants()
            .filter(|n| n.is_element() && n.tag_name().name() == tag)
            .filter_map(|n| n.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    };
    Some(EpubMeta {
        title: all("title").into_iter().next(),
        authors: all("creator"),
        publisher: all("publisher").into_iter().next(),
        published: all("date")
            .into_iter()
            .next()
            .map(|d| d.chars().take(10).collect()),
    })
}

/// Apply user overrides on top of file-derived metadata.
pub fn apply(b: &mut Book, m: &Meta) {
    if let Some(t) = &m.title {
        if !t.is_empty() {
            b.title = t.clone();
        }
    }
    if let Some(a) = &m.authors {
        b.authors = a.clone();
    }
    if let Some(p) = &m.publisher {
        b.publisher = if p.is_empty() { None } else { Some(p.clone()) };
    }
    if let Some(p) = &m.published {
        b.published = if p.is_empty() { None } else { Some(p.clone()) };
    }
}

pub fn scan(root: &Path, store: &Store) -> Vec<Book> {
    WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.file_name().to_string_lossy() != ".readru")
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let p = e.path();
            let ext = p.extension()?.to_str()?.to_lowercase();
            if !EXTS.contains(&ext.as_str()) {
                return None;
            }
            let meta = e.metadata().ok()?;
            let (mut title, mut authors) = parse_name(&p.file_stem()?.to_string_lossy());
            let (mut publisher, mut published) = (None, None);
            if ext == "epub" {
                if let Some(m) = read_epub(p) {
                    if let Some(t) = m.title {
                        title = t;
                    }
                    if !m.authors.is_empty() {
                        authors = m.authors;
                    }
                    publisher = m.publisher;
                    published = m.published;
                }
            }
            let rel = p
                .strip_prefix(root)
                .unwrap_or(p)
                .to_string_lossy()
                .into_owned();
            let mut b = Book {
                path: p.to_path_buf(),
                rel,
                title,
                authors,
                publisher,
                published,
                format: ext,
                size: meta.len(),
                added: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            };
            if let Some(m) = store.books.get(&b.rel) {
                apply(&mut b, m);
            }
            Some(b)
        })
        .collect()
}

pub fn sort(books: &mut [Book], key: SortKey, asc: bool) {
    const LAST: &str = "\u{10ffff}"; // missing values sort to the end
    let opt = |o: &Option<String>| {
        o.as_ref()
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| LAST.into())
    };
    books.sort_by(|a, b| {
        let o = match key {
            SortKey::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            SortKey::Author => {
                let f = |b: &Book| {
                    b.authors
                        .first()
                        .map(|s| s.to_lowercase())
                        .unwrap_or_else(|| LAST.into())
                };
                f(a).cmp(&f(b))
            }
            SortKey::Published => opt(&a.published).cmp(&opt(&b.published)),
            SortKey::Publisher => opt(&a.publisher).cmp(&opt(&b.publisher)),
            SortKey::Added => a.added.cmp(&b.added),
            SortKey::Size => a.size.cmp(&b.size),
            SortKey::Format => a.format.cmp(&b.format),
        };
        if asc {
            o
        } else {
            o.reverse()
        }
    });
}
