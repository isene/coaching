//! coaching: the people you coach, in the terminal.
//!
//! Each person is one text file in ~/.coaching: a card (who they are, a
//! picture, a link to the CV), their goals, a roadmap and a timeline of
//! notes. The whole screen takes the person's colour, so it is plain at
//! a glance whose page is open.

mod cal;
mod data;

use crust::{seq, style, Crust, Cursor, Input, Pane};
use data::{Person, CARD, GOALS, HEADS, ROADMAP, TIMELINE};
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

/// The calendar: a fifth part, read from tock and never written.
const CAL: usize = 4;
/// Claude's pane under the page, the last stop of Tab.
const CLAUDE: usize = 5;

/// Claude's CLAUDE.md, written to the people's folder once; edits there stay.
const BRIEF: &str = "# coaching's Claude pane

You run in the lower part of coaching, a terminal app where the user
keeps the people he coaches. The page of one person is above you: a
card, goals, a roadmap, a timeline of notes and a calendar.

When the user moves the keys to this pane, the next prompt starts with
an <on-screen app=\"coaching\"> block: the person on the page, their
file, and the line that was picked. Talk about that person unless he
names another.

## The files

This folder has one text file per person, `<name>.txt`:

- The lines before the first heading are the card, as `key: value`
  (name, colour, started, picture, cv, email, calendar, oca, and any other).
  `oca:` has the ten scores of an OCA test, traits A to J, from -100 to 100.
  `started` is the day the coaching began. The page shows the colour
  everywhere, so the card leaves the `colour` line out.
- `# Goals` and `# Roadmap`: one line each, with `[ ]` or `[x]` in front.
- `# Timeline`: one note a line, the newest first, each starting with
  its date as YYYY-MM-DD.
- A line that starts with two spaces belongs to the line above it.
- A file or web address in a line is a link the user opens with Enter.
  A path with spaces goes in <angle brackets>.

Change a file in place; the page shows the change at once. Keep his
wording when you tidy a note. Add a note at the top of the timeline
with today's date. Never delete a note unless he asks.

The calendar is read from tock and cannot be written here. To make an
appointment, use the calendar skill.

## Skills

Read the skill before work it covers:

- `scientology`: auditing, training, the Bridge, and any roadmap or
  note that uses those terms.
- `who`: finding out about a person from his own mail and the web.
- `cv`: writing or redoing a CV.
- `calendar`: appointments and reminders.
- `kastrup`: his mail and messages with the person, and drafts to them.
- `voice`: text written in his voice.

These files are private. Nothing about a person leaves this machine
unless he asks for it.
";
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
const KEYS: &str = "Ctrl+←→ person · Tab part · + add · n note · a calendar · x tick · Esc Claude · F1 keys · Q quit";
const CAL_KEYS: &str = "Ctrl+←→ person · ←→ day · w/W week · m/M month · y/Y year · t today · ↑↓ appointment · Tab part · Esc Claude · Q quit";

struct App {
    dir: PathBuf,
    people: Vec<(PathBuf, Person, Option<SystemTime>)>,
    who: usize,
    part: usize,
    sel: [usize; 5],
    top: [usize; 5],
    /// The open person's appointments, and what they were read for:
    /// the person, and when tock last wrote.
    appts: Vec<cal::Appt>,
    appts_for: Option<(usize, Option<SystemTime>)>,
    /// The day picked in the calendar; the months start at its month.
    day: cal::Day,
    note: String,
    /// Claude under the page, when it started; `full` gives it the
    /// whole screen (Ctrl+Z).
    claude: Option<deck::app::App>,
    full: bool,
    /// The page must be drawn again.
    dirty: bool,
    display: Option<glow::Display>,
    /// The picture on screen: its file and where it sits.
    shown: Option<(String, u16, u16)>,
    /// The OCA graph in pixels, on a display of its own so that it and
    /// the picture come and go apart: the scores, the colour, the place.
    oca_display: Option<glow::Display>,
    oca_shown: Option<([i32; 10], Rgb, u16, u16)>,
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("-v" | "--version") => return println!("coaching {}", env!("CARGO_PKG_VERSION")),
        Some("-n" | "--no-claude") => {}
        Some(_) => return println!("coaching: the people you coach. One text file per person in ~/.coaching\n(or in COACHING_DIR). F1 lists the keys. -n runs it without the Claude pane."),
        None => {}
    }
    let dir = data::dir();
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir);
    }
    let mut app = App { dir, people: Vec::new(), who: 0, part: TIMELINE, sel: [0; 5], top: [0; 5], appts: Vec::new(), appts_for: None, day: cal::day_of(cal::now()), note: String::new(), claude: None, full: false, dirty: true, display: None, shown: None, oca_display: None, oca_shown: None };
    app.load_all();
    Crust::init();
    Crust::set_app_identity("coaching");
    Crust::clear_screen();
    if std::env::args().nth(1).is_none() {
        match start_claude(&app.dir) {
            Ok(c) => app.claude = Some(c),
            Err(e) => app.note = format!("No Claude pane: {e}"),
        }
        Crust::set_app_identity("coaching");
    }
    app.whole();
    app.run();
    app.drop_picture();
    match app.claude.take() {
        Some(c) => {
            keep(&app.dir, c.session_id());
            deck::app::finish(c);
        }
        None => Crust::cleanup(),
    }
}

/// Claude: deck in the people's folder, the same session each time (its
/// id is kept in `.session` there), so Claude remembers what came before.
fn start_claude(dir: &std::path::Path) -> Result<deck::app::App, String> {
    let brief = dir.join("CLAUDE.md");
    if !brief.exists() {
        let _ = std::fs::write(&brief, BRIEF);
    }
    let kept = std::fs::read_to_string(dir.join(".session")).unwrap_or_default();
    let id = kept.trim();
    let app = if id.is_empty() {
        deck::app::start(None, dir.to_str(), Some("coaching"))?
    } else {
        match deck::app::start(Some(id), None, Some("coaching")) {
            Err(e) if e.starts_with("No ") => deck::app::start(None, dir.to_str(), Some("coaching"))?,
            other => other?,
        }
    };
    keep(dir, app.session_id());
    Ok(app)
}

/// Keep the session id, and tag a new session #coaching in cc-sessions'
/// bookmarks, where fleet and `cc coaching` find it.
fn keep(dir: &std::path::Path, id: &str) {
    let kept = std::fs::read_to_string(dir.join(".session")).unwrap_or_default();
    if kept.trim() == id {
        return;
    }
    let _ = std::fs::write(dir.join(".session"), id);
    let file = data::home().join(".cc-sessions/bookmarks.json");
    let Some(mut b) = std::fs::read_to_string(&file).ok().and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()) else { return };
    let Some(sessions) = b["sessions"].as_object_mut() else { return };
    sessions.retain(|_, e| !e["tags"].as_array().is_some_and(|t| t.len() == 1 && t[0] == "coaching"));
    for e in sessions.values_mut() {
        if let Some(t) = e["tags"].as_array_mut() {
            t.retain(|t| t != "coaching");
        }
    }
    sessions.insert(id.into(), serde_json::json!({ "path": dir, "tags": ["coaching"] }));
    let tmp = file.with_extension("json.tmp");
    if let Ok(text) = serde_json::to_string_pretty(&b) {
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &file);
        }
    }
}

fn claude_label(focused: bool) -> String {
    if focused { style::bold(&style::fg(" ▸ Claude", 156)) } else { style::fg(" Claude   (Tab or Esc: the keys here · Ctrl+Z: the whole screen)", 240) }
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
            self.dirty = true;
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

    fn count(&self, part: usize) -> usize {
        if part == CAL { self.appts.len() } else { self.items(part).len() }
    }

    /// Read the appointments, when the person or tock's file is new.
    fn load_cal(&mut self) {
        let store = cal::store();
        let key = (self.who, cal::stamp(&store));
        if self.appts_for == Some(key) {
            return;
        }
        let fresh = self.appts_for.map(|k| k.0) != Some(self.who);
        self.appts_for = Some(key);
        let needles = self.person().map(cal::needles).unwrap_or_default();
        match cal::load(&store, &needles) {
            Ok(a) => self.appts = a,
            Err(e) => {
                self.appts.clear();
                self.note = e;
            }
        }
        if fresh {
            // Start at the next appointment to come.
            let now = cal::now();
            self.sel[CAL] = self.appts.iter().position(|a| a.end.max(a.start) >= now).unwrap_or(self.appts.len().saturating_sub(1));
            self.day = cal::day_of(self.appts.get(self.sel[CAL]).map_or(now, |a| a.start));
        }
    }

    fn items(&self, part: usize) -> &[String] {
        if part >= CAL {
            return &[];
        }
        self.person().map(|p| p.parts[part].as_slice()).unwrap_or(&[])
    }

    fn picked(&self) -> Option<String> {
        self.items(self.part).get(self.sel[self.part]).cloned()
    }

    /// The rows the page has: all of them, or those above Claude.
    fn page_rows(&self) -> u16 {
        let (_, rows) = Crust::terminal_size();
        if self.claude.is_none() || rows < 24 {
            return rows;
        }
        rows - (rows * 2 / 5).max(10)
    }

    /// Everything drawn again from nothing: after a resize, an editor,
    /// prism or the help.
    fn whole(&mut self) {
        Crust::clear_screen();
        self.shown = None;
        self.oca_shown = None;
        self.dirty = true;
        let (_, rows) = Crust::terminal_size();
        let top = self.page_rows();
        let (full, keys) = (self.full, self.part == CLAUDE);
        if let Some(c) = self.claude.as_mut() {
            c.set_tab_line(Some(claude_label(keys || full)));
            c.set_area(if full || top == rows { None } else { Some((top, rows - top, 0)) });
            c.set_focus(keys || full);
        }
    }

    /// The keys to a part of the page, or to Claude.
    fn focus(&mut self, part: usize) {
        // With no Claude, Tab goes round the page alone.
        let part = match (part == CLAUDE && self.claude.is_none(), self.part == CARD) {
            (true, true) => CAL,
            (true, false) => CARD,
            _ => part,
        };
        if part == CLAUDE && self.part != CLAUDE {
            self.tell_claude();
        }
        self.part = part;
        self.dirty = true;
        if let Some(c) = self.claude.as_mut() {
            c.set_tab_line(Some(claude_label(part == CLAUDE)));
            c.set_focus(part == CLAUDE);
        }
    }

    /// What is on the page, for Claude's next prompt.
    fn tell_claude(&mut self) {
        let Some((file, person, _)) = self.people.get(self.who) else { return };
        let mut text = format!("The person on the page: {}\nTheir file: {}\n\n{}", Self::name(&self.people[self.who]), file.display(), person.text());
        if let Some(line) = self.picked() {
            text.push_str(&format!("\nThe line picked in {}: {line}\n", HEADS[self.part]));
        }
        if !self.appts.is_empty() && self.appts_for.map(|k| k.0) == Some(self.who) {
            text.push_str("\nAppointments in tock:\n");
            for a in &self.appts {
                let l = cal::local(a.start);
                text.push_str(&format!("{:04}-{:02}-{:02} {:02}:{:02} {}\n", l.year, l.month, l.day, l.hour, l.min, a.title));
            }
        }
        if let Some(c) = self.claude.as_mut() {
            c.attach("coaching", &text);
        }
    }

    fn run(&mut self) {
        let mut size = Crust::terminal_size();
        loop {
            self.reload();
            if self.part == CAL {
                self.load_cal();
            }
            for i in 0..5 {
                self.sel[i] = self.sel[i].min(self.count(i).saturating_sub(1));
            }
            if self.claude.as_mut().is_some_and(|c| c.take_overdraw()) {
                self.dirty = true;
            }
            if self.dirty && !self.full {
                self.dirty = false;
                self.draw();
                if let Some(c) = self.claude.as_ref().filter(|_| self.part == CLAUDE) {
                    c.place_cursor();
                }
            }
            // Claude wakes the wait when its output arrives; with no
            // Claude there is nothing to wake for but a key.
            let wait = self.claude.as_ref().map_or(86400, |c| c.before_wait());
            let key = Input::getchr(Some(wait));
            let claude_keys = self.claude.is_some() && (self.part == CLAUDE || self.full);
            match key.as_deref() {
                None | Some("FOCUS_IN" | "FOCUS_OUT") => {
                    if let Some(c) = self.claude.as_mut() {
                        c.after_wait(key);
                    }
                }
                Some("RESIZE") => {
                    if Crust::terminal_size() != size {
                        size = Crust::terminal_size();
                        self.whole();
                    }
                    if let Some(c) = self.claude.as_mut() {
                        c.after_wait(key);
                    }
                }
                Some("C-Z") if self.claude.is_some() => {
                    self.full = !self.full;
                    if self.full && self.part != CLAUDE {
                        self.tell_claude();
                    }
                    self.whole();
                }
                Some("TAB") if claude_keys && !self.full => self.focus(CARD),
                Some("S-TAB") if claude_keys && !self.full => self.focus(CAL),
                Some(_) if claude_keys => {
                    if let Some(c) = self.claude.as_mut() {
                        c.after_wait(key);
                    }
                }
                Some(k) => {
                    if self.key(k) {
                        return;
                    }
                    self.dirty = true;
                    // Claude's output is read here too, so it never waits.
                    if let Some(c) = self.claude.as_mut() {
                        c.after_wait(None);
                    }
                }
            }
            if self.claude.as_ref().is_some_and(|c| !c.running()) {
                return;
            }
        }
    }

    /// A key for the page. True to quit.
    fn key(&mut self, key: &str) -> bool {
        self.note.clear();
        if self.part == CAL && self.cal_key(key) {
            return false;
        }
        let n = self.count(self.part);
        let s = &mut self.sel[self.part];
        match key {
            "Q" => return true,
            "ESC" if self.claude.is_some() => self.focus(CLAUDE),
            "LEFT" if self.who > 0 => self.turn_to(self.who - 1),
            "RIGHT" if self.who + 1 < self.people.len() => self.turn_to(self.who + 1),
            // Ctrl+← Ctrl+→ go round the people in a ring, from any part.
            "C-LEFT" if !self.people.is_empty() => self.turn_to((self.who + self.people.len() - 1) % self.people.len()),
            "C-RIGHT" if !self.people.is_empty() => self.turn_to((self.who + 1) % self.people.len()),
            "TAB" => self.focus((self.part + 1) % 6),
            "S-TAB" => self.focus((self.part + 5) % 6),
            "a" => self.focus(CAL),
            "UP" => *s = s.saturating_sub(1),
            "DOWN" if *s + 1 < n => *s += 1,
            "HOME" => *s = 0,
            "END" => *s = n.saturating_sub(1),
            "S-UP" if *s > 0 && self.part != CAL => self.shift(-1),
            "S-DOWN" if *s + 1 < n && self.part != CAL => self.shift(1),
            "+" if self.part == CAL => self.note = "An appointment is made in tock; it shows here when its title has the name".into(),
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
        // In the calendar, the day goes with the picked appointment.
        if self.part == CAL && matches!(key, "UP" | "DOWN" | "HOME" | "END") {
            if let Some(a) = self.appts.get(self.sel[CAL]) {
                self.day = cal::day_of(a.start);
            }
        }
        false
    }

    /// A key that moves the picked day in the calendar, as in tock. The
    /// list follows to the first appointment on or after that day.
    fn cal_key(&mut self, key: &str) -> bool {
        let d = self.day;
        self.day = match key {
            "RIGHT" | "d" => cal::add_days(d, 1),
            "LEFT" | "D" => cal::add_days(d, -1),
            "w" => cal::add_days(d, 7),
            "W" => cal::add_days(d, -7),
            "m" => cal::add_months(d, 1),
            "M" => cal::add_months(d, -1),
            "y" => cal::add_months(d, 12),
            "Y" => cal::add_months(d, -12),
            "t" => cal::day_of(cal::now()),
            _ => return false,
        };
        let last = self.appts.len().saturating_sub(1);
        self.sel[CAL] = self.appts.iter().position(|a| cal::day_of(a.start) >= self.day).unwrap_or(last);
        true
    }

    fn turn_to(&mut self, who: usize) {
        self.who = who;
        self.sel = [0; 5];
        self.top = [0; 5];
        self.appts.clear();
        self.appts_for = None;
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
        self.whole();
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
        let (part, at) = (self.part, self.sel[self.part]);
        // The start date is asked for alone, and must be a real date.
        if let Some(old) = item.strip_prefix("started:").filter(|_| part == CARD) {
            let Some(day) = ask(" The coaching started, YYYY-MM-DD: ", old.trim()) else { return };
            if !day.is_empty() && !cal::is_date(&day) {
                self.note = format!("\"{day}\" is not a date as YYYY-MM-DD, such as {}", today());
                return;
            }
            return self.change(|p| p.parts[part][at] = format!("started: {day}"));
        }
        let Some(new) = ask(" ", &item).filter(|t| !t.is_empty()) else { return };
        self.change(|p| p.parts[part][at] = new);
    }

    fn delete(&mut self) {
        let Some(item) = self.picked() else { return };
        let first = item.lines().next().unwrap_or_default().to_string();
        self.note = format!("delete \"{}\"? y/n", take_cells(&first, 50));
        self.draw();
        self.note.clear();
        // Claude's output wakes the wait as a resize: not an answer.
        let mut key = Input::getchr(None);
        while matches!(key.as_deref(), Some("RESIZE" | "FOCUS_IN" | "FOCUS_OUT")) {
            key = Input::getchr(None);
        }
        self.whole();
        if key.as_deref() != Some("y") {
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
        self.whole();
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
        p.parts[CARD] = vec![format!("name: {name}"), format!("started: {}", today()), "picture: ".into(), "cv: ".into()];
        p.colour_hex = Some("#f78c3c".into());
        if let Err(e) = data::save(&path, &p) {
            self.note = format!("not saved: {e}");
            return;
        }
        self.load_all();
        let who = self.people.iter().position(|x| x.0 == path).unwrap_or(0);
        self.turn_to(who);
        self.part = CARD;
        // Straight to prism, so the new person has a colour from the start.
        self.pick_colour();
        if self.note.is_empty() {
            self.note = format!("{name} is in {}", path.display());
        }
    }

    fn drop_picture(&mut self) {
        if self.shown.take().is_some() {
            if let Some(d) = self.display.as_mut() {
                d.clear_all();
            }
        }
        if self.oca_shown.take().is_some() {
            if let Some(d) = self.oca_display.as_mut() {
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

    /// The calendar in the timeline's place: months with the days of
    /// the appointments lit, then the appointments as a list.
    fn cal_rows(&mut self, w: usize, h: usize) -> Vec<String> {
        let colour = self.colour();
        let now = cal::now();
        let today = cal::local(now);
        let sel = self.sel[CAL];
        let head = format!(" Calendar{} ", if self.appts.is_empty() { String::new() } else { format!("  {}/{}", sel + 1, self.appts.len()) });
        let mut rows = vec![format!("{}{}", style::rgb(&head, Some(DARK), Some(colour), "b"), " ".repeat(w.saturating_sub(crust::display_width(&head))))];
        // The months start at the picked day's.
        let at = self.day;
        let days: Vec<cal::Day> = self.appts.iter().map(|a| cal::day_of(a.start)).collect();
        let months = ((w + 2) / 23).clamp(1, 3);
        for r in 0..8 {
            let mut line = String::from(" ");
            for m in 0..months {
                let (y, mo) = cal::month_after(at.0, at.1, m as i32);
                line.push_str(&month_row(y, mo, r, colour, &days, (today.year, today.month, today.day), at));
                line.push_str("  ");
            }
            let used = 1 + months * 23;
            rows.push(format!("{line}{}", " ".repeat(w.saturating_sub(used))));
        }
        rows.push(" ".repeat(w));
        let room = h.saturating_sub(rows.len());
        let top = &mut self.top[CAL];
        *top = (*top).min(sel);
        if room > 0 && sel >= *top + room {
            *top = sel + 1 - room;
        }
        for (i, a) in self.appts.iter().enumerate().skip(*top).take(room) {
            let l = cal::local(a.start);
            let when = if a.all_day { "all day    ".to_string() } else {
                let e = cal::local(a.end.max(a.start));
                format!("{:02}:{:02}-{:02}:{:02}", l.hour, l.min, e.hour, e.min)
            };
            let bg = (i == sel).then_some(PICK_BG);
            let past = a.end.max(a.start) < now;
            let front = format!(" {:04}-{:02}-{:02} {} {when}  ", l.year, l.month, l.day, cal::DAYS[l.weekday as usize]);
            let what = if a.place.is_empty() { a.title.clone() } else { format!("{} · {}", a.title, a.place) };
            let fw = crust::display_width(&front);
            rows.push(format!("{}{}", style::rgb(&front, Some(if past { DIM } else { colour }), bg, ""),
                style::rgb(&fit(&what, w.saturating_sub(fw)), Some(if past { DIM } else { TEXT }), bg, "")));
        }
        if self.appts.is_empty() {
            rows.push(style::rgb(&fit("  no appointment in tock with this name in its title", w), Some(DIM), None, "i"));
        }
        rows.resize(h, " ".repeat(w));
        rows.truncate(h);
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
        let (cols, _) = Crust::terminal_size();
        let rows = self.page_rows();
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
            // The OCA test, when the card has one and the page has room.
            // In real pixels where the terminal shows pictures, in braille elsewhere.
            let oca = self.person().and_then(Person::oca).filter(|_| body >= 20 && left > OCA_W);
            let pixels = oca.is_some() && self.oca_display.get_or_insert_with(glow::Display::new).supported();
            let mut rows_left = match oca {
                Some(v) => oca_rows(&v, colour, left.saturating_sub(1), pixels),
                None => Vec::new(),
            };
            let graph = oca.filter(|_| pixels).map(|v| (v, colour, 7, top as u16 + 1));
            let rest = body - rows_left.len();
            let upper = rest / 2;
            rows_left.extend(self.part_rows(GOALS, left.saturating_sub(1), upper));
            rows_left.extend(self.part_rows(ROADMAP, left.saturating_sub(1), rest - upper));
            let right = if self.part == CAL { self.cal_rows(w.saturating_sub(left), body) } else { self.part_rows(TIMELINE, w.saturating_sub(left), body) };
            for i in 0..body {
                out.push_str(&format!("{}{} {}", Cursor::at(1, (top + i) as u16), rows_left[i], right[i]));
            }
            print!("{out}");
            out.clear();
            self.show_picture(pic, card_h);
            self.show_oca(graph);
        }
        out.push_str(&foot(&self.note, if self.part == CAL { CAL_KEYS } else { KEYS }, w, rows));
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

    /// Put the OCA graph up, only when it is not the one already there.
    fn show_oca(&mut self, want: Option<([i32; 10], Rgb, u16, u16)>) {
        if want == self.oca_shown {
            return;
        }
        let Some(d) = self.oca_display.as_mut() else { return };
        match want {
            Some((v, colour, x, y)) => {
                std::io::stdout().flush().ok();
                if d.swap_canvas(&data::oca_canvas(&v, &data::oca_shades(colour), None), x, y) {
                    self.oca_shown = want;
                }
            }
            None => {
                d.clear_all();
                self.oca_shown = None;
            }
        }
    }

    fn help(&mut self) {
        let colour = self.colour();
        let hdr = |s: &str| style::rgb(s, Some(colour), None, "b");
        let key = |s: &str| style::rgb(&format!("  {s:<18}"), Some(TEXT), None, "b");
        let lines = [
            ("Ctrl+←  Ctrl+→", "the person before, the next one, round in a ring (← → too)"),
            ("Tab  Shift+Tab", "the next part: card, goals, roadmap, timeline, calendar, Claude"),
            ("↑  ↓  Home  End", "the line in the part"),
            ("Shift+↑  Shift+↓", "move the line up or down"),
            ("+", "a new line in this part"),
            ("n", "a new note in the timeline, dated today"),
            ("a", "the calendar: the person's appointments in tock"),
            ("in the calendar", "←→ d D day, w W week, m M month, y Y year, t today, ↑↓ appointment"),
            ("N", "a note of many lines, in your editor"),
            ("x  Space", "tick a goal or a step, or clear the tick"),
            ("Enter", "open the file or web address in the line (↗); else change it"),
            ("e", "change the line"),
            ("E", "the person's whole file in your editor"),
            ("Del  Ctrl+D", "delete the line"),
            ("c", "the person's colour, picked in prism"),
            ("p", "a new person"),
            ("Esc", "the keys to Claude, under the page; Tab there comes back"),
            ("Ctrl+Z", "Claude on the whole screen, and back"),
            ("Q", "quit"),
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
        self.whole();
    }
}

/// The bar along the bottom: what just happened, or the keys.
/// The cells the OCA graph takes with its numbers at the left.
const OCA_W: usize = 6 + 10 * data::OCA_STEP;

/// The OCA test as rows `w` cells wide: a heading, the line of the ten
/// scores with a dim line at zero, the letters of the traits, a gap.
/// Each trait has the shade of its group. With `pixels` the graph's
/// cells stay empty, for the picture that goes over them.
fn oca_rows(v: &[i32; 10], colour: Rgb, w: usize, pixels: bool) -> Vec<String> {
    let shades = data::oca_shades(colour);
    let mut rows = vec![style::rgb(&fit(" OCA ", w), Some(colour), None, "b")];
    for (r, line) in data::oca_graph(v).iter().enumerate() {
        let label = match r {
            0 => "+100",
            r if r == data::OCA_ROWS / 2 => "   0",
            r if r == data::OCA_ROWS - 1 => "-100",
            _ => "    ",
        };
        let mut row = style::rgb(&format!(" {label} "), Some(DIM), None, "");
        for (i, c) in line.chars().enumerate() {
            row.push_str(&match c {
                _ if pixels => " ".into(),
                '\u{2800}' if r == data::OCA_ROWS / 2 => style::rgb("╌", Some(DIM), None, ""),
                '\u{2800}' => " ".into(),
                c => style::rgb(&c.to_string(), Some(shades[i / data::OCA_STEP]), None, ""),
            });
        }
        rows.push(row + &" ".repeat(w.saturating_sub(OCA_W)));
    }
    let mut letters = "      ".to_string();
    for (i, c) in ('A'..='J').enumerate() {
        letters.push_str(&style::rgb(&format!("{c:^w$}", w = data::OCA_STEP), Some(shades[i]), None, "b"));
    }
    rows.push(letters + &" ".repeat(w.saturating_sub(OCA_W)));
    rows.push(" ".repeat(w));
    rows
}

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

/// Row `r` of a month, 21 cells wide: its name, the weekdays, then six
/// weeks. A day with an appointment has the person's colour behind it.
fn month_row(year: i32, month: u32, r: usize, colour: Rgb, days: &[(i32, u32, u32)], today: (i32, u32, u32), picked: (i32, u32, u32)) -> String {
    match r {
        0 => {
            let name = format!("{} {year}", cal::MONTHS[month as usize - 1]);
            let pad = 21usize.saturating_sub(name.len());
            style::rgb(&format!("{}{name}{}", " ".repeat(pad / 2), " ".repeat(pad - pad / 2)), Some(colour), None, "b")
        }
        1 => style::rgb("Mo Tu We Th Fr Sa Su ", Some(DIM), None, ""),
        _ => {
            let first = cal::first_weekday(year, month) as i32;
            let last = cal::days_in(year, month) as i32;
            let mut line = String::new();
            for col in 0..7 {
                let day = (r as i32 - 2) * 7 + col - first + 1;
                if day < 1 || day > last {
                    line.push_str("   ");
                    continue;
                }
                let d = (year, month, day as u32);
                let cell = format!("{day:>2}");
                line.push_str(&if days.contains(&d) {
                    // The picked day is underlined.
                    style::rgb(&cell, Some(DARK), Some(colour), if d == picked { "bu" } else { "b" })
                } else if d == picked {
                    style::rgb(&cell, Some(TEXT), Some(PICK_BG), if d == today { "bu" } else { "b" })
                } else if d == today {
                    style::rgb(&cell, Some(TEXT), None, "bu")
                } else {
                    style::rgb(&cell, Some(DIM), None, "")
                });
                line.push(' ');
            }
            line
        }
    }
}
