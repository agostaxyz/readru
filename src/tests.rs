use crate::config::Config;
use crate::library::{self, SortKey};
use crate::meta::{Meta, Store};
use crate::{reader, theme};
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("readru-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn filename_parsing_splits_multiple_authors() {
    let d = tmp("names");
    std::fs::write(
        d.join("Karl Marx & Friedrich Engels - The Capital.txt"),
        "x",
    )
    .unwrap();
    std::fs::write(d.join("notes.docx"), "x").unwrap();
    std::fs::create_dir_all(d.join(".readru")).unwrap();
    std::fs::write(d.join(".readru/hidden.txt"), "x").unwrap();
    let books = library::scan(&d, &Store::default());
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].title, "The Capital");
    assert_eq!(books[0].authors, vec!["Karl Marx", "Friedrich Engels"]);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn sort_by_author_puts_unknown_last() {
    let d = tmp("sort");
    for f in ["B Author - Two.txt", "A Author - One.txt", "Untitled.txt"] {
        std::fs::write(d.join(f), "x").unwrap();
    }
    let mut books = library::scan(&d, &Store::default());
    library::sort(&mut books, SortKey::Author, true);
    let titles: Vec<&str> = books.iter().map(|b| b.title.as_str()).collect();
    assert_eq!(titles, vec!["One", "Two", "Untitled"]);
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn user_overrides_win_over_file_metadata() {
    let d = tmp("override");
    std::fs::write(d.join("Some Book.txt"), "x").unwrap();
    let mut store = Store::default();
    store.books.insert(
        "Some Book.txt".into(),
        Meta {
            authors: Some(vec!["Karl Marx".into(), "Friedrich Engels".into()]),
            publisher: Some("John Books".into()),
            ..Default::default()
        },
    );
    let books = library::scan(&d, &store);
    assert_eq!(books[0].authors, vec!["Karl Marx", "Friedrich Engels"]);
    assert_eq!(books[0].publisher.as_deref(), Some("John Books"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn metadata_sidecar_roundtrip() {
    let d = tmp("store");
    let mut s = Store::default();
    s.books.insert(
        "a.txt".into(),
        Meta {
            publisher: Some("P".into()),
            ..Default::default()
        },
    );
    s.save(&d);
    let back = Store::load(&d);
    assert_eq!(back.books["a.txt"].publisher.as_deref(), Some("P"));
    let _ = std::fs::remove_dir_all(d);
}

#[test]
fn config_defaults_fill_missing_fields() {
    let c: Config = toml::from_str("library = \"/x\"").unwrap();
    assert_eq!(c.theme, "sharu-dark");
    let s = toml::to_string_pretty(&Config::default()).unwrap();
    assert!(toml::from_str::<Config>(&s).is_ok());
}

#[test]
fn builtin_themes_parse() {
    let names: Vec<String> = theme::load_all().into_iter().map(|(n, _)| n).collect();
    for n in [
        "sharu-dark",
        "github-dark",
        "github-dark-dimmed",
        "github-light",
    ] {
        assert!(names.iter().any(|x| x == n), "missing theme {n}");
    }
}

#[test]
fn html_to_text() {
    let h = "<html><head><title>x</title></head><body><p>Hello&nbsp;&amp; <b>world</b></p><p>Two</p></body></html>";
    assert_eq!(reader::strip_html(h).trim(), "Hello & world\n\nTwo");
}

#[test]
fn pages_break_on_whitespace_and_char_boundaries() {
    let t = "word ".repeat(100);
    let end = reader::page_end(&t, 0, 50);
    assert!(end > 25 && end <= 50);
    assert!(t[end..].starts_with(' '));
    let u = "é".repeat(100);
    assert!(u.is_char_boundary(reader::page_end(&u, 0, 10)));
}
