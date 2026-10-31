# readru

**A light ebook library and reader for Linux. Point it at a folder, start reading.**
No database, no import step, your files are never modified.

[![CI](https://img.shields.io/github/actions/workflow/status/agostaxyz/readru/ci.yml?style=flat-square&label=CI&logo=github&labelColor=0B120E)](https://github.com/agostaxyz/readru/actions/workflows/ci.yml)
[![License: AGPL-3.0](https://img.shields.io/badge/license-AGPL--3.0-3a3a3a?style=flat-square&labelColor=0B120E&color=3fb950)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-0B120E?style=flat-square&logo=rust)](https://www.rust-lang.org)
[![iced](https://img.shields.io/badge/iced-0.14-0B120E?style=flat-square&color=3fb950)](https://iced.rs)
[![Linux](https://img.shields.io/badge/Linux-0B120E?style=flat-square&logo=linux&logoColor=white)](#build)

### 📚 Library

Scans a folder for EPUB, PDF and TXT. Sort by title, author, publisher, date published, size or format. Right-click a book to edit its metadata or rename it.

### 👥 Authors are a list

"Karl Marx" and "Friedrich Engels" are two authors, not one string, so a book sorts under each of them.

### 📖 Reader

EPUB and TXT, single or two-page view, font and size settings (OpenDyslexic if installed), and it remembers where you stopped. Run it standalone with `readru book.epub`.

### 🎨 Themes

Dark, light and your own. Built in: `sharu-dark`, `github-dark`, `github-dark-dimmed`, `github-light`.

## Build

```sh
git clone https://github.com/agostaxyz/readru && cd readru
cargo run --release
```

On first launch it asks for your library folder (existing or new). Change it any time in Settings.

## Custom themes

Drop a `.toml` file in `~/.config/readru/themes/`. The file name is the theme name.

```toml
# hello_kitty_dark.toml
background = "#1a1015"
text       = "#f5e1ea"
primary    = "#ff7eb6"
success    = "#3fb950"
warning    = "#d29922"
danger     = "#f85149"
```

## Where data lives

| What | Where |
| --- | --- |
| Settings | `~/.config/readru/config.toml` |
| Reading position | `~/.config/readru/progress.toml` |
| Metadata edits | `<library>/.readru/metadata.toml` |

## Status

Early. Done: library, EPUB metadata, metadata editing, themes, settings, text reader.
Next: highlights, PDF (MuPDF), follow-OS theme, real covers.

## License

AGPL-3.0-or-later.
