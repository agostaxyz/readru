mod config;
mod library;
mod meta;
mod reader;
#[cfg(test)]
mod tests;
mod theme;

use config::{Config, PageMode, ReadDir, ReadMode, ReaderFont};
use iced::widget::{
    button, column, container, mouse_area, pick_list, row, scrollable, stack, text, text_input,
    Column, Row,
};
use iced::{
    border, keyboard, Border, Color, Element, Length, Padding, Point, Subscription, Task, Theme,
};
use library::{Book, SortKey};
use meta::{Meta, Store};
use std::path::PathBuf;

/// 1rem
const R: f32 = 16.0;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .theme(App::theme)
        .subscription(App::subscription)
        .run()
}

// ---------- styles (rounded everything + hover states) ----------

pub(crate) fn pill(t: &Theme, s: button::Status) -> button::Style {
    let mut x = button::primary(t, s);
    x.border.radius = R.into();
    x
}

pub(crate) fn soft(t: &Theme, s: button::Status) -> button::Style {
    let mut x = button::secondary(t, s);
    x.border.radius = R.into();
    x
}

fn menu_item(t: &Theme, s: button::Status) -> button::Style {
    let mut x = button::text(t, s);
    x.border.radius = 12.0_f32.into();
    if matches!(s, button::Status::Hovered | button::Status::Pressed) {
        x.background = Some(t.extended_palette().background.strong.color.into());
    }
    x
}

fn input_style(t: &Theme, s: text_input::Status) -> text_input::Style {
    let mut x = text_input::default(t, s);
    x.border.radius = 12.0_f32.into();
    x
}

fn pick_style(t: &Theme, s: pick_list::Status) -> pick_list::Style {
    let mut x = pick_list::default(t, s);
    x.border.radius = 12.0_f32.into();
    x
}

fn panel(t: &Theme) -> container::Style {
    let mut x = container::rounded_box(t);
    x.border.radius = R.into();
    x
}

// ---------- state ----------

struct Editor {
    idx: usize,
    title: String,
    authors: Vec<String>,
    publisher: String,
    published: String,
}

enum Modal {
    Settings,
    Edit(Editor),
    Rename {
        idx: usize,
        name: String,
        err: Option<String>,
    },
}

struct App {
    cfg: Config,
    store: Store,
    books: Vec<Book>,
    search: String,
    themes: Vec<(String, Theme)>,
    cursor: Point,
    hover: Option<usize>,
    menu: Option<(usize, Point)>,
    modal: Option<Modal>,
    reader: Option<reader::State>,
    standalone: bool, // `readru --reader book.epub`: closing the reader quits
}

#[derive(Debug, Clone)]
enum Msg {
    Noop,
    Escape,
    PickFolder,
    Picked(Option<PathBuf>),
    Scanned(Vec<Book>, Store),
    Search(String),
    SortBy(SortKey),
    ToggleOrder,
    ThemeChosen(String),
    FontChosen(ReaderFont),
    FontSize(i16),
    PageModeChosen(PageMode),
    DirChosen(ReadDir),
    ModeChosen(ReadMode),
    OpenSettings,
    OpenBook(usize),
    Reader(reader::Msg),
    Cursor(Point),
    Hover(Option<usize>),
    Context(usize),
    CloseMenu,
    StartEdit,
    StartRename,
    CloseModal,
    EdTitle(String),
    EdAuthor(usize, String),
    EdAddAuthor,
    EdRemoveAuthor(usize),
    EdPublisher(String),
    EdPublished(String),
    EdSave,
    RenameInput(String),
    RenameSave,
}

fn scan_task(root: PathBuf) -> Task<Msg> {
    Task::perform(
        async move {
            let s = Store::load(&root);
            (library::scan(&root, &s), s)
        },
        |(b, s)| Msg::Scanned(b, s),
    )
}

const COVERS: [(u8, u8, u8); 8] = [
    (0x2f, 0x6f, 0x4f),
    (0x6f, 0x2f, 0x3f),
    (0x2f, 0x4f, 0x7f),
    (0x7f, 0x5f, 0x2f),
    (0x5f, 0x2f, 0x7f),
    (0x2f, 0x6f, 0x6f),
    (0x7f, 0x3f, 0x2f),
    (0x4f, 0x4f, 0x5f),
];

fn card(idx: usize, b: &Book, hovered: bool) -> Element<'static, Msg> {
    let h = b
        .title
        .bytes()
        .fold(7usize, |a, c| a.wrapping_mul(31).wrapping_add(c as usize));
    let (r, g, bl) = COVERS[h % COVERS.len()];
    let color = Color::from_rgb8(r, g, bl);
    let initial = b
        .title
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let cover = container(text(initial).size(44))
        .center_x(130.0)
        .center_y(190.0)
        .style(move |_| container::Style {
            background: Some(color.into()),
            border: border::rounded(12),
            text_color: Some(Color::WHITE),
            ..Default::default()
        });
    let authors = if b.authors.is_empty() {
        "Unknown".to_string()
    } else {
        b.authors.join(", ")
    };
    let extra: Vec<String> = [b.publisher.clone(), b.published.clone()]
        .into_iter()
        .flatten()
        .collect();
    let body = column![
        cover,
        text(b.title.clone()).size(14).width(130.0),
        text(authors).size(12).width(130.0).style(text::secondary),
        text(extra.join(" · "))
            .size(11)
            .width(130.0)
            .style(text::secondary),
    ]
    .spacing(6)
    .width(130.0);

    // CSS-like :hover — tint + accent border
    let tile = container(body).padding(10).style(move |t: &Theme| {
        let p = t.extended_palette();
        if hovered {
            container::Style {
                background: Some(p.background.weak.color.into()),
                border: Border {
                    color: p.primary.base.color,
                    width: 1.5,
                    radius: R.into(),
                },
                ..Default::default()
            }
        } else {
            container::Style {
                border: Border {
                    color: Color::TRANSPARENT,
                    width: 1.5,
                    radius: R.into(),
                },
                ..Default::default()
            }
        }
    });
    mouse_area(tile)
        .on_enter(Msg::Hover(Some(idx)))
        .on_exit(Msg::Hover(None))
        .on_press(Msg::OpenBook(idx))
        .on_right_press(Msg::Context(idx))
        .into()
}

fn menu_btn(label: &'static str, msg: Msg) -> Element<'static, Msg> {
    button(text(label))
        .on_press(msg)
        .width(Length::Fill)
        .style(menu_item)
        .into()
}

fn menu_overlay(at: Point) -> Element<'static, Msg> {
    let menu = container(
        column![
            menu_btn("Edit metadata", Msg::StartEdit),
            menu_btn("Rename file", Msg::StartRename)
        ]
        .spacing(2),
    )
    .padding(6)
    .width(190.0)
    .style(panel);
    mouse_area(
        container(menu)
            .padding(Padding {
                top: at.y,
                left: at.x,
                right: 0.0,
                bottom: 0.0,
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Msg::CloseMenu)
    .on_right_press(Msg::CloseMenu)
    .into()
}

fn section(label: &'static str) -> Element<'static, Msg> {
    text(label).size(13).style(text::secondary).into()
}

impl App {
    fn new() -> (Self, Task<Msg>) {
        let cfg = Config::load();
        // `readru book.epub` opens the reader, closing it shows the library.
        // `readru --reader book.epub` is the standalone reader: no library, closing quits.
        let args: Vec<String> = std::env::args().skip(1).collect();
        let flag = args.iter().any(|a| a == "--reader");
        let reader = args
            .iter()
            .find(|a| !a.starts_with("--"))
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .map(|p| {
                let t = p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                reader::State::open(&p, &t)
            });
        let standalone = flag && reader.is_some();
        let task = if standalone {
            Task::none()
        } else {
            cfg.library
                .clone()
                .map(scan_task)
                .unwrap_or_else(Task::none)
        };
        let app = Self {
            cfg,
            store: Store::default(),
            books: vec![],
            search: String::new(),
            themes: theme::load_all(),
            cursor: Point::ORIGIN,
            hover: None,
            menu: None,
            modal: None,
            reader,
            standalone,
        };
        (app, task)
    }

    fn title(&self) -> String {
        match &self.reader {
            Some(r) => format!("{} — readru", r.title),
            None => "readru".to_string(),
        }
    }

    fn theme(&self) -> Theme {
        self.themes
            .iter()
            .find(|(n, _)| *n == self.cfg.theme)
            .map(|(_, t)| t.clone())
            .unwrap_or(Theme::Dark)
    }

    fn subscription(&self) -> Subscription<Msg> {
        iced::event::listen_with(|event, status, _window| {
            use iced::event::Status;
            use keyboard::key::Named;
            if status != Status::Ignored {
                return None;
            }
            match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => match key {
                    keyboard::Key::Named(Named::ArrowRight) => {
                        Some(Msg::Reader(reader::Msg::Right))
                    }
                    keyboard::Key::Named(Named::ArrowLeft) => Some(Msg::Reader(reader::Msg::Left)),
                    keyboard::Key::Named(Named::PageDown) => Some(Msg::Reader(reader::Msg::Next)),
                    keyboard::Key::Named(Named::PageUp) => Some(Msg::Reader(reader::Msg::Prev)),
                    keyboard::Key::Named(Named::Escape) => Some(Msg::Escape),
                    _ => None,
                },
                _ => None,
            }
        })
    }

    fn resort(&mut self) {
        self.menu = None;
        self.hover = None;
        library::sort(&mut self.books, self.cfg.sort, self.cfg.ascending);
    }

    fn do_rename(&mut self) -> Result<Task<Msg>, String> {
        let Some(Modal::Rename { idx, name, .. }) = &self.modal else {
            return Ok(Task::none());
        };
        let name = name.trim().to_string();
        let idx = *idx;
        let root = self.cfg.library.clone().ok_or("No library folder set")?;
        let b = self.books.get(idx).ok_or("Book not found")?;
        if name.is_empty() || name.contains('/') {
            return Err("Invalid file name".into());
        }
        let ext = b
            .path
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new = b.path.with_file_name(format!("{name}.{ext}"));
        if new != b.path && new.exists() {
            return Err("A file with that name already exists".into());
        }
        std::fs::rename(&b.path, &new).map_err(|e| e.to_string())?;
        let old_rel = b.rel.clone();
        let new_rel = new
            .strip_prefix(&root)
            .unwrap_or(&new)
            .to_string_lossy()
            .into_owned();
        if let Some(m) = self.store.books.remove(&old_rel) {
            self.store.books.insert(new_rel, m);
            self.store.save(&root);
        }
        self.modal = None;
        Ok(scan_task(root))
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Noop => {}
            Msg::Escape => {
                if self.modal.is_some() {
                    self.modal = None;
                } else {
                    self.menu = None;
                }
            }
            Msg::PickFolder => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("Choose your readru library folder")
                            .pick_folder()
                            .await
                            .map(|h| h.path().to_path_buf())
                    },
                    Msg::Picked,
                )
            }
            Msg::Picked(Some(p)) => {
                self.cfg.library = Some(p.clone());
                self.cfg.save();
                return scan_task(p);
            }
            Msg::Picked(None) => {}
            Msg::Scanned(b, s) => {
                self.books = b;
                self.store = s;
                self.resort();
            }
            Msg::Search(s) => {
                self.search = s;
                self.menu = None;
                self.hover = None;
            }
            Msg::SortBy(k) => {
                self.cfg.sort = k;
                self.cfg.save();
                self.resort();
            }
            Msg::ToggleOrder => {
                self.cfg.ascending = !self.cfg.ascending;
                self.cfg.save();
                self.resort();
            }
            Msg::ThemeChosen(n) => {
                self.cfg.theme = n;
                self.cfg.save();
            }
            Msg::FontChosen(f) => {
                self.cfg.reader_font = f;
                self.cfg.save();
            }
            Msg::FontSize(d) => {
                self.cfg.reader_size = (self.cfg.reader_size as i16 + d).clamp(12, 32) as u16;
                self.cfg.save();
            }
            Msg::PageModeChosen(m) => {
                self.cfg.page_mode = m;
                self.cfg.save();
            }
            Msg::DirChosen(d) => {
                self.cfg.read_dir = d;
                self.cfg.save();
            }
            Msg::ModeChosen(d) => {
                self.cfg.read_mode = d;
                self.cfg.save();
            }
            Msg::OpenSettings => {
                self.menu = None;
                self.modal = Some(Modal::Settings);
            }
            Msg::OpenBook(i) => {
                if let Some(b) = self.books.get(i) {
                    self.menu = None;
                    self.hover = None;
                    self.reader = Some(reader::State::open(&b.path, &b.title));
                }
            }
            Msg::Reader(m) => {
                let rtl = self.cfg.read_dir == ReadDir::Rtl;
                match m {
                    reader::Msg::Close => {
                        if self.standalone {
                            return iced::exit();
                        }
                        self.reader = None;
                    }
                    reader::Msg::Settings => self.modal = Some(Modal::Settings),
                    reader::Msg::ToggleDir => {
                        self.cfg.read_dir = if rtl { ReadDir::Ltr } else { ReadDir::Rtl };
                        self.cfg.save();
                    }
                    reader::Msg::Next => {
                        if let Some(r) = &mut self.reader {
                            r.next();
                        }
                    }
                    reader::Msg::Prev => {
                        if let Some(r) = &mut self.reader {
                            r.prev();
                        }
                    }
                    // physical left/right: manga mode swaps what they mean
                    reader::Msg::Left => {
                        if let Some(r) = &mut self.reader {
                            if rtl {
                                r.next()
                            } else {
                                r.prev()
                            }
                        }
                    }
                    reader::Msg::Right => {
                        if let Some(r) = &mut self.reader {
                            if rtl {
                                r.prev()
                            } else {
                                r.next()
                            }
                        }
                    }
                    reader::Msg::Scrolled(at_bottom) => {
                        if at_bottom {
                            if let Some(r) = &mut self.reader {
                                r.next();
                            }
                        }
                    }
                }
            }
            Msg::Cursor(p) => self.cursor = p,
            Msg::Hover(h) => self.hover = h,
            Msg::Context(i) => self.menu = Some((i, self.cursor)),
            Msg::CloseMenu => self.menu = None,
            Msg::CloseModal => self.modal = None,
            Msg::StartEdit => {
                if let Some((i, _)) = self.menu.take() {
                    if let Some(b) = self.books.get(i) {
                        self.modal = Some(Modal::Edit(Editor {
                            idx: i,
                            title: b.title.clone(),
                            authors: if b.authors.is_empty() {
                                vec![String::new()]
                            } else {
                                b.authors.clone()
                            },
                            publisher: b.publisher.clone().unwrap_or_default(),
                            published: b.published.clone().unwrap_or_default(),
                        }));
                    }
                }
            }
            Msg::StartRename => {
                if let Some((i, _)) = self.menu.take() {
                    if let Some(b) = self.books.get(i) {
                        let name = b
                            .path
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        self.modal = Some(Modal::Rename {
                            idx: i,
                            name,
                            err: None,
                        });
                    }
                }
            }
            Msg::EdTitle(s) => {
                if let Some(Modal::Edit(e)) = &mut self.modal {
                    e.title = s;
                }
            }
            Msg::EdAuthor(i, s) => {
                if let Some(Modal::Edit(e)) = &mut self.modal {
                    if let Some(a) = e.authors.get_mut(i) {
                        *a = s;
                    }
                }
            }
            Msg::EdAddAuthor => {
                if let Some(Modal::Edit(e)) = &mut self.modal {
                    e.authors.push(String::new());
                }
            }
            Msg::EdRemoveAuthor(i) => {
                if let Some(Modal::Edit(e)) = &mut self.modal {
                    if i < e.authors.len() {
                        e.authors.remove(i);
                    }
                }
            }
            Msg::EdPublisher(s) => {
                if let Some(Modal::Edit(e)) = &mut self.modal {
                    e.publisher = s;
                }
            }
            Msg::EdPublished(s) => {
                if let Some(Modal::Edit(e)) = &mut self.modal {
                    e.published = s;
                }
            }
            Msg::EdSave => {
                if let Some(Modal::Edit(e)) = self.modal.take() {
                    let authors: Vec<String> = e
                        .authors
                        .iter()
                        .map(|a| a.trim().to_string())
                        .filter(|a| !a.is_empty())
                        .collect();
                    let m = Meta {
                        title: Some(e.title.trim().to_string()).filter(|t| !t.is_empty()),
                        authors: Some(authors),
                        publisher: Some(e.publisher.trim().to_string()),
                        published: Some(e.published.trim().to_string()),
                    };
                    if let Some(b) = self.books.get_mut(e.idx) {
                        library::apply(b, &m);
                        self.store.books.insert(b.rel.clone(), m);
                    }
                    if let Some(r) = &self.cfg.library {
                        self.store.save(r);
                    }
                    self.resort();
                }
            }
            Msg::RenameInput(s) => {
                if let Some(Modal::Rename { name, .. }) = &mut self.modal {
                    *name = s;
                }
            }
            Msg::RenameSave => match self.do_rename() {
                Ok(t) => return t,
                Err(e) => {
                    if let Some(Modal::Rename { err, .. }) = &mut self.modal {
                        *err = Some(e);
                    }
                }
            },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        let base: Element<'_, Msg> = if let Some(r) = &self.reader {
            reader::view(r, &self.cfg, self.standalone).map(Msg::Reader)
        } else if self.cfg.library.is_none() {
            container(
                column![
                    text("Welcome to readru").size(32),
                    text("Pick an existing folder or create a new one for your library."),
                    button("Choose library folder")
                        .on_press(Msg::PickFolder)
                        .style(pill),
                ]
                .spacing(16)
                .align_x(iced::Center),
            )
            .center(Length::Fill)
            .into()
        } else {
            self.library_base()
        };

        let mut layers = stack![base];
        if let Some((_, at)) = self.menu {
            layers = layers.push(menu_overlay(at));
        }
        if let Some(m) = &self.modal {
            layers = layers.push(self.modal_view(m));
        }
        layers.into()
    }

    fn library_base(&self) -> Element<'_, Msg> {
        let q = self.search.to_lowercase();
        let cards: Vec<Element<'static, Msg>> = self
            .books
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                q.is_empty()
                    || b.title.to_lowercase().contains(&q)
                    || b.authors.iter().any(|a| a.to_lowercase().contains(&q))
            })
            .map(|(i, b)| card(i, b, self.hover == Some(i)))
            .collect();

        let bar = row![
            text("readru").size(24).width(Length::Fill),
            text_input("Search…", &self.search)
                .on_input(Msg::Search)
                .style(input_style)
                .width(260.0),
            pick_list(SortKey::ALL, Some(self.cfg.sort), Msg::SortBy).style(pick_style),
            button(if self.cfg.ascending {
                "↑ Asc"
            } else {
                "↓ Desc"
            })
            .on_press(Msg::ToggleOrder)
            .style(soft),
            button("⚙ Settings").on_press(Msg::OpenSettings).style(pill),
        ]
        .spacing(12)
        .align_y(iced::Center);

        let grid = Row::with_children(cards)
            .spacing(12)
            .wrap()
            .vertical_spacing(12);
        mouse_area(column![
            container(bar).padding(16),
            scrollable(container(grid).padding(16))
                .width(Length::Fill)
                .height(Length::Fill),
        ])
        .on_move(Msg::Cursor)
        .into()
    }

    fn modal_view<'a>(&'a self, m: &'a Modal) -> Element<'a, Msg> {
        let (content, width, closable): (Element<'a, Msg>, f32, bool) = match m {
            Modal::Settings => (self.settings_content(), 520.0, true),
            Modal::Edit(e) => {
                let mut authors = Column::new().spacing(6);
                for (i, a) in e.authors.iter().enumerate() {
                    authors = authors.push(
                        row![
                            text_input("Author", a)
                                .on_input(move |s| Msg::EdAuthor(i, s))
                                .style(input_style),
                            button("✕").on_press(Msg::EdRemoveAuthor(i)).style(soft),
                        ]
                        .spacing(6),
                    );
                }
                (
                    column![
                        text("Edit metadata").size(20),
                        section("Title"),
                        text_input("Title", &e.title)
                            .on_input(Msg::EdTitle)
                            .style(input_style),
                        section("Authors (one per row)"),
                        authors,
                        button("+ Add author")
                            .on_press(Msg::EdAddAuthor)
                            .style(soft),
                        section("Publisher"),
                        text_input("Publisher", &e.publisher)
                            .on_input(Msg::EdPublisher)
                            .style(input_style),
                        section("Published (e.g. 1867 or 1867-09-14)"),
                        text_input("Published", &e.published)
                            .on_input(Msg::EdPublished)
                            .style(input_style),
                        row![
                            button("Cancel").on_press(Msg::CloseModal).style(soft),
                            button("Save").on_press(Msg::EdSave).style(pill)
                        ]
                        .spacing(10),
                    ]
                    .spacing(8)
                    .into(),
                    460.0,
                    false,
                )
            }
            Modal::Rename { name, err, .. } => {
                let mut c = column![
                    text("Rename file").size(20),
                    text_input("File name", name)
                        .on_input(Msg::RenameInput)
                        .on_submit(Msg::RenameSave)
                        .style(input_style),
                ]
                .spacing(8);
                if let Some(e) = err {
                    c = c.push(text(e.as_str()).size(13).style(text::danger));
                }
                (
                    c.push(
                        row![
                            button("Cancel").on_press(Msg::CloseModal).style(soft),
                            button("Rename").on_press(Msg::RenameSave).style(pill)
                        ]
                        .spacing(10),
                    )
                    .into(),
                    460.0,
                    false,
                )
            }
        };

        // inner mouse_area swallows clicks so only the backdrop closes the modal
        let dialog = mouse_area(container(content).padding(24).width(width).style(panel))
            .on_press(Msg::Noop);
        let backdrop = container(dialog)
            .center(Length::Fill)
            .style(|_| container::Style {
                background: Some(
                    Color {
                        a: 0.55,
                        ..Color::BLACK
                    }
                    .into(),
                ),
                ..Default::default()
            });
        mouse_area(backdrop)
            .on_press(if closable { Msg::CloseModal } else { Msg::Noop })
            .into()
    }

    fn settings_content(&self) -> Element<'_, Msg> {
        let names: Vec<String> = self.themes.iter().map(|(n, _)| n.clone()).collect();
        let dir = theme::dir()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        let lib = self
            .cfg
            .library
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        column![
            row![
                text("Settings").size(22).width(Length::Fill),
                button("✕").on_press(Msg::CloseModal).style(menu_item),
            ]
            .align_y(iced::Center),
            section("LIBRARY"),
            row![
                text(lib).width(Length::Fill),
                button("Change").on_press(Msg::PickFolder).style(soft),
            ]
            .spacing(12)
            .align_y(iced::Center),
            section("APPEARANCE"),
            pick_list(names, Some(self.cfg.theme.clone()), Msg::ThemeChosen)
                .style(pick_style)
                .width(Length::Fill),
            text(format!(
                "Custom themes: drop .toml files in {dir} (restart to reload)"
            ))
            .size(12)
            .style(text::secondary),
            section("READER"),
            row![
                text("Font").width(Length::Fill),
                pick_list(ReaderFont::ALL, Some(self.cfg.reader_font), Msg::FontChosen)
                    .style(pick_style),
            ]
            .align_y(iced::Center),
            row![
                text("Size").width(Length::Fill),
                button("−").on_press(Msg::FontSize(-1)).style(soft),
                text(format!("{} pt", self.cfg.reader_size)),
                button("+").on_press(Msg::FontSize(1)).style(soft),
            ]
            .spacing(10)
            .align_y(iced::Center),
            row![
                text("Pages").width(Length::Fill),
                pick_list(PageMode::ALL, Some(self.cfg.page_mode), Msg::PageModeChosen)
                    .style(pick_style),
            ]
            .align_y(iced::Center),
            row![
                text("Direction").width(Length::Fill),
                pick_list(ReadDir::ALL, Some(self.cfg.read_dir), Msg::DirChosen).style(pick_style),
            ]
            .align_y(iced::Center),
            row![
                text("Mode").width(Length::Fill),
                pick_list(ReadMode::ALL, Some(self.cfg.read_mode), Msg::ModeChosen)
                    .style(pick_style),
            ]
            .align_y(iced::Center),
        ]
        .spacing(12)
        .into()
    }
}
