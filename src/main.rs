//! coaching: the people you coach, in the terminal.
//!
//! Each person is one text file in ~/.coaching: a card (who they are, a
//! picture, a link to the CV), their goals, a roadmap and a timeline of
//! notes. The whole screen takes the person's colour, so it is plain at
//! a glance whose page is open.

mod data;

use crust::{seq, style, Crust, Cursor, Input, Pane};
use data::{Person, CARD, GOALS, HEADS, ROADMAP, TIMELINE};
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

type Rgb = (u8, u8, u8);
const BASE: Rgb = (247, 140, 60);
const TEXT: Rgb = (225, 225, 230);
const DIM: Rgb = (140, 140, 150);
const DARK: Rgb = (20, 20, 24);
const BAR_BG: Rgb = (38, 38, 38);
const PICK_BG: Rgb = (58, 58, 66);
/// The card: rows, and the cells its picture gets.
const CARD_H: usize = 8;
const PIC_W: usize = 16;
const KEYS: &str = "←→ person · Tab part · + add · n note · x tick · Enter open/change · F1 all keys · q quit";

struct App {
    dir: PathBuf,
    people: Vec<(PathBuf, Person, Option<SystemTime>)>,
    who: usize,
    part: usize,
    sel: [usize; 4],
    top: [usize; 4],
    note: String,
    display: Option<glow::Display>,
    /// The picture on screen: its file and where it sits.
    shown: Option<(String, u16, u16)>,
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("-v" | "--version") => return println!("coaching {}", env!("CARGO_PKG_VERSION")),
        Some(_) => return println!("coaching: the people you coach. One text file per person in ~/.coaching\n(or in COACHING_DIR). Run it with no arguments; F1 lists the keys."),
        None => {}
    }
    let dir = data::dir();
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir);
    }
    let mut app = App { dir, people: Vec::new(), who: 0, part: TIMELINE, sel: [0; 4], top: [0; 4], note: String::new(), display: None, shown: None };
    app.load_all();
    Crust::init();
    Crust::set_app_identity("coaching");
    Crust::clear_screen();
    app.run();
    app.drop_picture();
    Crust::cleanup();
}

fn mtime(path: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Today as 2026-11-12, by the clock on the wall here.
fn today() -> String {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    format!("{:04}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
}

fn ask(prompt: &str, initial: &str) -> Option<String> {
    let (cols, rows) = Crust::terminal_size();
    let mut p = Pane::new(1, rows, cols, 1, 255, 236);
    p.scroll = false;
    p.ask_or_cancel(prompt, initial).map(|s| s.trim().to_string())
}

fn editor() -> String {
    std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).unwrap_or_else(|_| "scribe".into())
}

impl App {
    fn load_all(&mut self) {
        self.people = data::files(&self.dir).into_iter().map(|f| {
            let p = Person::parse(&std::fs::read_to_string(&f).unwrap_or_default());
            let m = mtime(&f);
            (f, p, m)
        }).collect();
        self.who = self.who.min(self.people.len().saturating_sub(1));
    }

    /// Read the open person's file again when something else changed it.
    fn reload(&mut self) {
        let Some((f, p, m)) = self.people.get_mut(self.who) else { return };
        let now = mtime(f);
        if now != *m {
            *p = Person::parse(&std::fs::read_to_string(&*f).unwrap_or_default());
            *m = now;
        }
    }

    fn person(&self) -> Option<&Person> {
        self.people.get(self.who).map(|p| &p.1)
    }

    fn colour(&self) -> Rgb {
        self.person().and_then(Person::colour).unwrap_or(BASE)
    }

    fn name(p: &(PathBuf, Person, Option<SystemTime>)) -> String {
        p.1.field("name").map(String::from)
            .unwrap_or_else(|| p.0.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
    }

    /// Change the open person and write the file.
    fn change(&mut self, edit: impl FnOnce(&mut Person)) {
        self.reload();
        let Some((f, p, m)) = self.people.get_mut(self.who) else { return };
        edit(p);
        match data::save(f, p) {
            Ok(()) => *m = mtime(f),
            Err(e) => self.note = format!("not saved: {e}"),
        }
    }

    fn items(&self, part: usize) -> &[String] {
        self.person().map(|p| p.parts[part].as_slice()).unwrap_or(&[])
    }

    fn picked(&self) -> Option<String> {
        self.items(self.part).get(self.sel[self.part]).cloned()
    }

    fn run(&mut self) {
        loop {
            self.reload();
            for i in 0..4 {
                self.sel[i] = self.sel[i].min(self.items(i).len().saturating_sub(1));
            }
            self.draw();
            let Some(key) = Input::getchr(None) else { continue };
            self.note.clear();
            let n = self.items(self.part).len();
            let s = &mut self.sel[self.part];
            match key.as_str() {
                "q" | "ESC" | "C-Q" => return,
                "LEFT" if self.who > 0 => self.turn_to(self.who - 1),
                "RIGHT" if self.who + 1 < self.people.len() => self.turn_to(self.who + 1),
                "TAB" => self.part = (self.part + 1) % 4,
                "S-TAB" => self.part = (self.part + 3) % 4,
                "UP" => *s = s.saturating_sub(1),
                "DOWN" if *s + 1 < n => *s += 1,
                "HOME" => *s = 0,
                "END" => *s = n.saturating_sub(1),
                "S-UP" if *s > 0 => self.shift(-1),
                "S-DOWN" if *s + 1 < n => self.shift(1),
                "+" => self.add(self.part),
                "n" => self.add(TIMELINE),
                "N" => self.long_note(),
                "x" | " " => self.tick(),
                "ENTER" => self.open_or_edit(),
                "e" => self.edit(),
                "E" => self.whole_file(),
                "DEL" | "C-D" => self.delete(),
                "c" => self.pick_colour(),
                "p" => self.new_person(),
                "F1" | "?" => self.help(),
                _ => {}
            }
        }
    }

    fn turn_to(&mut self, who: usize) {
        self.who = who;
        self.sel = [0; 4];
        self.top = [0; 4];
    }

    fn shift(&mut self, by: isize) {
        let (part, at) = (self.part, self.sel[self.part]);
        let to = (at as isize + by) as usize;
        self.change(|p| p.parts[part].swap(at, to));
        self.sel[part] = to;
    }

    fn add(&mut self, part: usize) {
        if self.people.is_empty() {
            return self.new_person();
        }
        let prompt = [" New line on the card, as key: value  ", " New goal: ", " New step on the roadmap: ", " Note: "][part];
        let Some(text) = ask(prompt, "").filter(|t| !t.is_empty()) else { return };
        self.part = part;
        match part {
            TIMELINE => {
                let line = format!("{} {text}", today());
                self.change(|p| p.parts[TIMELINE].insert(0, line));
                self.sel[part] = 0;
            }
            CARD => {
                self.change(|p| p.parts[CARD].push(text));
                self.sel[part] = self.items(part).len().saturating_sub(1);
            }
            _ => {
                self.change(|p| p.parts[part].push(format!("[ ] {text}")));
                self.sel[part] = self.items(part).len().saturating_sub(1);
            }
        }
    }

    /// A note of many lines, written in the editor.
    fn long_note(&mut self) {
        if self.people.is_empty() {
            return;
        }
        let tmp = self.dir.join(".note");
        let _ = std::fs::write(&tmp, "");
        self.in_editor(&tmp);
        let text = std::fs::read_to_string(&tmp).unwrap_or_default();
        let _ = std::fs::remove_file(&tmp);
        let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        if lines.is_empty() {
            self.note = "no note written".into();
            return;
        }
        let item = format!("{} {}", today(), lines.join("\n"));
        self.change(|p| p.parts[TIMELINE].insert(0, item));
        self.part = TIMELINE;
        self.sel[TIMELINE] = 0;
    }

    fn in_editor(&mut self, path: &std::path::Path) {
        self.drop_picture();
        Crust::cleanup();
        let ran = std::process::Command::new(editor()).arg(path).status();
        Crust::init();
        Crust::clear_screen();
        if let Err(e) = ran {
            self.note = format!("{}: {e}", editor());
        }
    }

    fn whole_file(&mut self) {
        let Some(f) = self.people.get(self.who).map(|p| p.0.clone()) else { return };
        self.in_editor(&f);
    }

    fn tick(&mut self) {
        let (part, at) = (self.part, self.sel[self.part]);
        if !(part == GOALS || part == ROADMAP) || self.picked().is_none() {
            return;
        }
        self.change(|p| p.parts[part][at] = data::ticked(&p.parts[part][at]));
    }

    fn open_or_edit(&mut self) {
        let Some(item) = self.picked() else { return };
        let Some(link) = data::link(&item) else { return self.edit() };
        let target = data::real_path(&link);
        if !link.starts_with("http") && !std::path::Path::new(&target).exists() {
            self.note = format!("no such file: {link}");
            return;
        }
        use std::process::Stdio;
        let ran = std::process::Command::new("xdg-open").arg(&target)
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        self.note = match ran {
            Ok(_) => format!("opened {link}"),
            Err(e) => format!("xdg-open: {e}"),
        };
    }

    fn edit(&mut self) {
        let Some(item) = self.picked() else { return };
        if item.contains('\n') {
            // Many lines: the editor, on a file of its own.
            let tmp = self.dir.join(".note");
            let _ = std::fs::write(&tmp, &item);
            self.in_editor(&tmp);
            let text = std::fs::read_to_string(&tmp).unwrap_or_default();
            let _ = std::fs::remove_file(&tmp);
            let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
            if lines.is_empty() {
                return;
            }
            let (part, at, new) = (self.part, self.sel[self.part], lines.join("\n"));
            return self.change(|p| p.parts[part][at] = new);
        }
        let Some(new) = ask(" ", &item).filter(|t| !t.is_empty()) else { return };
        let (part, at) = (self.part, self.sel[self.part]);
        self.change(|p| p.parts[part][at] = new);
    }

    fn delete(&mut self) {
        let Some(item) = self.picked() else { return };
        let first = item.lines().next().unwrap_or_default().to_string();
        self.note = format!("delete \"{}\"? y/n", take_cells(&first, 50));
        self.draw();
        self.note.clear();
        if Input::getchr(None).as_deref() != Some("y") {
            return;
        }
        let (part, at) = (self.part, self.sel[self.part]);
        self.change(|p| { p.parts[part].remove(at); });
    }

    /// The person's colour, picked in prism and kept on the card.
    fn pick_colour(&mut self) {
        if self.people.is_empty() {
            return;
        }
        let out = std::env::temp_dir().join(format!("coaching_pick_{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let (r, g, b) = self.colour();
        self.drop_picture();
        Crust::cleanup();
        let ran = std::process::Command::new("prism").arg("--pick").arg(format!("--out={}", out.display()))
            .arg(format!("#{r:02x}{g:02x}{b:02x}")).status();
        Crust::init();
        Crust::clear_screen();
        let text = std::fs::read_to_string(&out).unwrap_or_default();
        let _ = std::fs::remove_file(&out);
        let hex = text.lines().find_map(|l| {
            let h = l.strip_prefix("fg=").unwrap_or(l).trim();
            style::parse_hex_color(h).map(|_| h.to_string())
        });
        match (ran, hex) {
            (Err(_), _) => self.note = "prism is not installed".into(),
            (_, None) => self.note = "no colour picked".into(),
            (_, Some(hex)) => self.change(|p| p.set_field("colour", &hex)),
        }
    }

    fn new_person(&mut self) {
        let Some(name) = ask(" Name of the new person: ", "").filter(|t| !t.is_empty()) else { return };
        let slug = data::slug(&name);
        let path = self.dir.join(format!("{slug}.txt"));
        if slug.is_empty() || path.exists() {
            self.note = format!("there is a {slug}.txt already");
            return;
        }
        let mut p = Person::default();
        p.parts[CARD] = vec![format!("name: {name}"), "colour: #f78c3c".into(), "picture: ".into(), "cv: ".into()];
        if let Err(e) = data::save(&path, &p) {
            self.note = format!("not saved: {e}");
            return;
        }
        self.load_all();
        let who = self.people.iter().position(|x| x.0 == path).unwrap_or(0);
        self.turn_to(who);
        self.part = CARD;
        self.note = format!("{name} is in {}; c picks a colour", path.display());
    }

    fn drop_picture(&mut self) {
        if self.shown.take().is_some() {
            if let Some(d) = self.display.as_mut() {
                d.clear_all();
            }
        }
    }

    /// One part as rows of text: its heading, then the items from the
    /// first one in view, the picked one lit.
    fn part_rows(&mut self, part: usize, w: usize, h: usize) -> Vec<String> {
        let colour = self.colour();
        let items: Vec<String> = self.items(part).to_vec();
        let focus = part == self.part;
        let count = if items.is_empty() { String::new() } else { format!("  {}/{}", self.sel[part] + 1, items.len()) };
        let head = format!(" {}{} ", HEADS[part], if focus { count } else { String::new() });
        let mut rows = vec![if focus {
            format!("{}{}", style::rgb(&head, Some(DARK), Some(colour), "b"), " ".repeat(w.saturating_sub(crust::display_width(&head))))
        } else {
            style::rgb(&fit(&head, w), Some(colour), None, "b")
        }];
        let room = h.saturating_sub(1);
        let blocks: Vec<Vec<String>> = items.iter().enumerate().map(|(i, it)| self.item_rows(part, it, w, focus && i == self.sel[part])).collect();
        // Scroll so the picked item is in view, whole when it can be.
        let (sel, top) = (self.sel[part], &mut self.top[part]);
        *top = (*top).min(sel);
        while *top < sel && blocks[*top..=sel].iter().map(Vec::len).sum::<usize>() > room {
            *top += 1;
        }
        for row in blocks.iter().skip(*top).flatten() {
            if rows.len() > room {
                break;
            }
            rows.push(row.clone());
        }
        if items.is_empty() {
            rows.push(style::rgb(&fit("  nothing yet; + adds a line", w), Some(DIM), None, "i"));
        }
        rows.resize(h, " ".repeat(w));
        rows
    }

    fn item_rows(&self, part: usize, item: &str, w: usize, picked: bool) -> Vec<String> {
        let colour = self.colour();
        let bg = picked.then_some(PICK_BG);
        let mark = if data::link(item).is_some() { "↗" } else { " " };
        let lead = |s: &str| style::rgb(s, Some(colour), bg, "");
        match part {
            CARD => {
                let (k, v) = item.split_once(':').unwrap_or(("", item));
                let v = v.trim();
                let text = fit(&take_cells(v, w.saturating_sub(15)), w.saturating_sub(13));
                // The name is the card's headline.
                let attr = if k.trim().eq_ignore_ascii_case("name") { "b" } else { "" };
                vec![format!("{}{}{}", lead(&format!(" {mark}")), lead(&fit(k.trim(), 11)), style::rgb(&text, Some(TEXT), bg, attr))]
            }
            GOALS | ROADMAP => {
                let (done, text) = data::boxed(item);
                let fg = if done { DIM } else { TEXT };
                data::wrap(text, w.saturating_sub(5)).iter().enumerate().map(|(i, l)| {
                    let front = if i == 0 { format!(" {mark}{} ", if done { "✔" } else { "☐" }) } else { "    ".into() };
                    format!("{}{}", lead(&front), style::rgb(&fit(l, w.saturating_sub(4)), Some(fg), bg, ""))
                }).collect()
            }
            _ => {
                // A note starts with its date, in the person's colour.
                let (date, text) = match item.split_once(' ') {
                    Some((d, t)) if d.len() == 10 && d.starts_with(|c: char| c.is_ascii_digit()) => (d, t),
                    _ => ("", item),
                };
                data::wrap(text, w.saturating_sub(14)).iter().enumerate().map(|(i, l)| {
                    let front = if i == 0 { format!(" {mark}{date:<10} ") } else { " ".repeat(13) };
                    format!("{}{}", lead(&front), style::rgb(&fit(l, w.saturating_sub(13)), Some(TEXT), bg, ""))
                }).collect()
            }
        }
    }

    fn draw(&mut self) {
        let (cols, rows) = Crust::terminal_size();
        let (w, h) = (cols as usize, rows as usize);
        let colour = self.colour();
        let mut out = String::new();

        // The bar: every person in their own colour, the open one lit.
        let mut bar = style::rgb(" coaching  ", Some(DIM), Some(BAR_BG), "");
        let mut used = 11;
        for (i, p) in self.people.iter().enumerate() {
            let c = p.1.colour().unwrap_or(BASE);
            let piece = format!(" {} ", Self::name(p));
            if used + crust::display_width(&piece) + 1 > w {
                break;
            }
            used += crust::display_width(&piece) + 1;
            bar.push_str(&if i == self.who { style::rgb(&piece, Some(DARK), Some(c), "b") } else { style::rgb(&piece, Some(c), Some(BAR_BG), "") });
            bar.push_str(&style::rgb(" ", None, Some(BAR_BG), ""));
        }
        bar.push_str(&style::rgb(&" ".repeat(w.saturating_sub(used)), None, Some(BAR_BG), ""));
        out.push_str(&format!("{}{}", Cursor::at(1, 1), bar));
        // A line in the person's colour under the bar.
        out.push_str(&format!("{}{}", Cursor::at(1, 2), style::rgb(&"▔".repeat(w), Some(colour), None, "")));

        if self.people.is_empty() {
            for r in 3..h {
                out.push_str(&format!("{}{}", Cursor::at(1, r as u16), seq::ERASE_EOL));
            }
            out.push_str(&format!("{}{}", Cursor::at(3, 4), style::rgb("Nobody here yet. p makes the first person.", Some(DIM), None, "i")));
        } else {
            // The card, with the picture at its left when there is one.
            let pic = self.person().and_then(|p| p.field("picture")).map(data::real_path)
                .filter(|p| !p.is_empty() && std::path::Path::new(p).is_file());
            let card_x = if pic.is_some() { PIC_W + 4 } else { 1 };
            let card_h = CARD_H.min(h.saturating_sub(8));
            let card = self.part_rows(CARD, w.saturating_sub(card_x), card_h);
            for (i, row) in card.iter().enumerate() {
                out.push_str(&format!("{}{}", Cursor::at(card_x as u16 + 1, 3 + i as u16), row));
            }
            // Goals over the roadmap at the left, the timeline at the right.
            let top = 3 + card_h + 1;
            out.push_str(&format!("{}{}", Cursor::at(1, top as u16 - 1), seq::ERASE_EOL));
            let body = h.saturating_sub(top);
            let left = (w * 2 / 5).max(20).min(w);
            let upper = body / 2;
            let mut rows_left = self.part_rows(GOALS, left.saturating_sub(1), upper);
            rows_left.extend(self.part_rows(ROADMAP, left.saturating_sub(1), body - upper));
            let right = self.part_rows(TIMELINE, w.saturating_sub(left), body);
            for i in 0..body {
                out.push_str(&format!("{}{} {}", Cursor::at(1, (top + i) as u16), rows_left[i], right[i]));
            }
            print!("{out}");
            out.clear();
            self.show_picture(pic, card_h);
        }
        out.push_str(&foot(&self.note, KEYS, w, rows));
        print!("{out}");
        std::io::stdout().flush().ok();
    }

    /// Put the picture up, only when it is not the one already there.
    fn show_picture(&mut self, pic: Option<String>, card_h: usize) {
        let want = pic.map(|p| (p, PIC_W as u16, card_h as u16));
        if want == self.shown {
            return;
        }
        self.drop_picture();
        // The cells the picture sat in, for a person with none.
        let mut blank = String::new();
        for r in 0..card_h {
            blank.push_str(&format!("{}{}", Cursor::at(1, 3 + r as u16), " ".repeat(PIC_W + 4)));
        }
        print!("{blank}");
        let Some((path, pw, ph)) = want else { return };
        let d = self.display.get_or_insert_with(glow::Display::new);
        if d.supported() && d.show(&path, 3, 3, pw, ph) {
            self.shown = Some((path, pw, ph));
        }
    }

    fn help(&mut self) {
        let colour = self.colour();
        let hdr = |s: &str| style::rgb(s, Some(colour), None, "b");
        let key = |s: &str| style::rgb(&format!("  {s:<18}"), Some(TEXT), None, "b");
        let lines = [
            ("←  →", "the person before, the next one"),
            ("Tab  Shift+Tab", "the next part: card, goals, roadmap, timeline"),
            ("↑  ↓  Home  End", "the line in the part"),
            ("Shift+↑  Shift+↓", "move the line up or down"),
            ("+", "a new line in this part"),
            ("n", "a new note in the timeline, dated today"),
            ("N", "a note of many lines, in your editor"),
            ("x  Space", "tick a goal or a step, or clear the tick"),
            ("Enter", "open the file or web address in the line (↗); else change it"),
            ("e", "change the line"),
            ("E", "the person's whole file in your editor"),
            ("Del  Ctrl+D", "delete the line"),
            ("c", "the person's colour, picked in prism"),
            ("p", "a new person"),
            ("q  Esc", "quit"),
        ];
        self.drop_picture();
        Crust::clear_screen();
        let mut t = format!("{}{}\n\r\n\r", Cursor::at(2, 2), hdr("coaching: the keys"));
        for (k, what) in lines {
            t.push_str(&format!("{}{}\n\r", key(k), style::rgb(what, Some(DIM), None, "")));
        }
        t.push_str(&format!("\n\r  {}\n\r  {}\n\r", style::rgb("A person is one text file in ~/.coaching. On the card, picture: and cv: name files;", Some(DIM), None, ""),
            style::rgb("a path with spaces goes in <angle brackets>. Any key goes back.", Some(DIM), None, "")));
        print!("{t}");
        std::io::stdout().flush().ok();
        Input::getchr(None);
        Crust::clear_screen();
    }
}

/// The bar along the bottom: what just happened, or the keys.
fn foot(note: &str, keys: &str, w: usize, rows: u16) -> String {
    let version = format!("v{} ", env!("CARGO_PKG_VERSION"));
    let text = if note.is_empty() { keys } else { note };
    let text = format!(" {}", take_cells(text, w.saturating_sub(version.len() + 3)));
    let pad = w.saturating_sub(crust::display_width(&text) + version.len());
    format!("{}{}{}", Cursor::at(1, rows),
        style::rgb(&format!("{text}{}", " ".repeat(pad)), Some((200, 200, 205)), Some(BAR_BG), ""),
        style::rgb(&version, Some(DIM), Some(BAR_BG), ""))
}

/// Exactly `w` cells: cut with a mark when too long, padded when short.
fn fit(s: &str, w: usize) -> String {
    if crust::display_width(s) <= w {
        return format!("{s}{}", " ".repeat(w - crust::display_width(s)));
    }
    let mut t = take_cells(s, w.saturating_sub(1));
    t.push('…');
    let pad = w.saturating_sub(crust::display_width(&t));
    format!("{t}{}", " ".repeat(pad))
}

/// The first `max` cells of `s`, never cutting a glyph in two.
fn take_cells(s: &str, max: usize) -> String {
    let mut walker = crust::WidthWalker::new();
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let add = walker.push(c);
        if w + add > max {
            break;
        }
        w += add;
        out.push(c);
    }
    out
}
