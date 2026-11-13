//! One person, as a plain text file anyone can read and change by hand:
//!
//! ```text
//! name: Alice Example
//! colour: #88aaff
//! started: 2026-11-01
//! picture: ~/Pictures/alice.jpg
//! cv: ~/Documents/alice-cv.pdf
//!
//! # Goals
//! [ ] Speak in front of fifty people
//!
//! # Roadmap
//! [x] 2026-12 The first course
//!
//! # Timeline
//! 2026-11-12 First session. Her plan is in ~/Documents/alice-plan.pdf
//! ```
//!
//! The lines before the first heading are the card. A line under a
//! heading is one item; a line that starts with a space or a tab belongs
//! to the item above it.

use std::path::{Path, PathBuf};

pub const CARD: usize = 0;
pub const GOALS: usize = 1;
pub const ROADMAP: usize = 2;
pub const TIMELINE: usize = 3;
pub const HEADS: [&str; 4] = ["Card", "Goals", "Roadmap", "Timeline"];

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Person {
    /// The card, the goals, the roadmap and the timeline, an item per line.
    pub parts: [Vec<String>; 4],
    /// The `colour:` line's value, kept off the card: the whole page
    /// shows the colour, and `c` changes it.
    pub colour_hex: Option<String>,
}

impl Person {
    pub fn parse(text: &str) -> Person {
        let mut p = Person::default();
        let mut part = CARD;
        for line in text.lines() {
            if let Some(head) = line.strip_prefix("# ") {
                if let Some(i) = HEADS.iter().position(|h| h.eq_ignore_ascii_case(head.trim())) {
                    part = i;
                    continue;
                }
            }
            if line.trim().is_empty() {
                continue;
            }
            if part == CARD && !line.starts_with([' ', '\t']) {
                if let Some((k, v)) = line.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case("colour")) {
                    let _ = k;
                    p.colour_hex = Some(v.trim().to_string());
                    continue;
                }
            }
            // A line that starts with a space goes on with the item above.
            match p.parts[part].last_mut() {
                Some(last) if line.starts_with([' ', '\t']) => {
                    last.push('\n');
                    last.push_str(line.trim());
                }
                _ => p.parts[part].push(line.trim().to_string()),
            }
        }
        p
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        let colour = self.colour_hex.as_ref().map(|c| format!("colour: {c}\n")).unwrap_or_default();
        for (i, items) in self.parts.iter().enumerate() {
            if i != CARD {
                out.push_str(&format!("\n# {}\n", HEADS[i]));
            }
            for (n, item) in items.iter().enumerate() {
                out.push_str(&item.replace('\n', "\n  "));
                out.push('\n');
                // The colour goes back in the file under the first line.
                if i == CARD && n == 0 {
                    out.push_str(&colour);
                }
            }
            if i == CARD && items.is_empty() {
                out.push_str(&colour);
            }
        }
        out
    }

    /// The value of a line `key: value` on the card.
    pub fn field(&self, key: &str) -> Option<&str> {
        if key.eq_ignore_ascii_case("colour") {
            return self.colour_hex.as_deref();
        }
        self.parts[CARD].iter().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(key).then_some(v.trim())
        })
    }

    pub fn set_field(&mut self, key: &str, value: &str) {
        if key.eq_ignore_ascii_case("colour") {
            self.colour_hex = Some(value.to_string());
            return;
        }
        let line = format!("{key}: {value}");
        let at = self.parts[CARD].iter().position(|l| l.split_once(':').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key)));
        match at {
            Some(i) => self.parts[CARD][i] = line,
            None => self.parts[CARD].push(line),
        }
    }

    pub fn colour(&self) -> Option<(u8, u8, u8)> {
        let hex = self.field("colour")?.trim_start_matches('#');
        if hex.len() != 6 || !hex.is_ascii() {
            return None;
        }
        let n = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some((n(0)?, n(2)?, n(4)?))
    }
}

/// The graph of an OCA test: cells wide per trait, and rows high.
pub const OCA_STEP: usize = 3;
pub const OCA_ROWS: usize = 5;

impl Person {
    /// The ten scores of the card's `oca:` line, traits A to J, each from
    /// -100 to 100. Other words on the line (a date, say) are passed over.
    pub fn oca(&self) -> Option<[i32; 10]> {
        let v: Vec<i32> = self.field("oca")?
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter_map(|w| w.parse().ok())
            .filter(|n| (-100..=100).contains(n))
            .collect();
        v.try_into().ok()
    }
}

/// The scores as a line of braille dots, a row of text per row of cells.
/// A cell the line does not cross is the empty braille cell.
pub fn oca_graph(v: &[i32; 10]) -> Vec<String> {
    let (w, h) = (10 * OCA_STEP * 2, OCA_ROWS * 4);
    let mut dots = vec![false; w * h];
    let at = |i: usize| ((OCA_STEP + i * OCA_STEP * 2) as i32, (100 - v[i]) * (h as i32 - 1) / 200);
    for i in 0..9 {
        let ((x0, y0), (x1, y1)) = (at(i), at(i + 1));
        for x in x0..=x1 {
            let y = y0 + (y1 - y0) * (x - x0) / (x1 - x0);
            dots[y as usize * w + x as usize] = true;
        }
    }
    const BITS: [[u32; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];
    (0..OCA_ROWS).map(|r| (0..w / 2).map(|c| {
        let mut bits = 0;
        for (dx, col) in BITS.iter().enumerate() {
            for (dy, bit) in col.iter().enumerate() {
                if dots[(r * 4 + dy) * w + c * 2 + dx] {
                    bits |= bit;
                }
            }
        }
        char::from_u32(0x2800 + bits).unwrap_or(' ')
    }).collect()).collect()
}

/// A colour per trait: A B C lighter than the person's colour, D light
/// grey, E F G the colour itself, H I J darker.
pub fn oca_shades(c: (u8, u8, u8)) -> [(u8, u8, u8); 10] {
    let mix = |to: f64, k: f64| {
        let m = |v: u8| (v as f64 + (to - v as f64) * k).round() as u8;
        (m(c.0), m(c.1), m(c.2))
    };
    let (light, dark) = (mix(255.0, 0.5), mix(0.0, 0.45));
    [light, light, light, (205, 205, 210), c, c, c, dark, dark, dark]
}

/// The scores as a smooth line in real pixels, on a see-through canvas
/// of the graph's cells: a dashed line at zero, the line in each trait's
/// shade, a dot on every score.
pub fn oca_canvas(v: &[i32; 10], shades: &[(u8, u8, u8); 10], cell: Option<(u16, u16)>) -> glow::Canvas {
    let mut cv = glow::Canvas::sized((10 * OCA_STEP) as u16, OCA_ROWS as u16, cell);
    cv.see_through();
    let (w, h) = (cv.w as f64, cv.h as f64);
    let step = w / 10.0;
    let p: Vec<(f64, f64)> = (0..10).map(|i| ((i as f64 + 0.5) * step, 5.0 + (100 - v[i]) as f64 / 200.0 * (h - 10.0))).collect();
    let zero = (h / 2.0).round() as i64;
    for x in 0..cv.w as i64 {
        if x % 9 < 5 {
            cv.blend(x, zero, (140, 140, 150), 0.7);
        }
    }
    let m = tangents(&p);
    let (xa, xb) = (p[0].0.floor() as i64, p[9].0.ceil() as i64);
    let ys: Vec<f64> = (xa..=xb).map(|x| spline(&p, &m, x as f64 + 0.5)).collect();
    for i in 1..ys.len() {
        let x = xa as f64 + i as f64;
        let rgb = shades[(((x - p[0].0) / step).round().max(0.0) as usize).min(9)];
        cv.line((x - 0.5, ys[i - 1]), (x + 0.5, ys[i]), 2.4, rgb, 1.0);
    }
    for (i, q) in p.iter().enumerate() {
        cv.disc(q.0, q.1, 3.6, shades[i], 1.0);
    }
    cv
}

/// Slopes at each point for a curve that never swings past the points
/// (Fritsch-Carlson): a smooth line that shows no high the scores had not.
fn tangents(p: &[(f64, f64)]) -> Vec<f64> {
    let n = p.len();
    let d: Vec<f64> = (0..n - 1).map(|i| (p[i + 1].1 - p[i].1) / (p[i + 1].0 - p[i].0)).collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for i in 1..n - 1 {
        m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) / 2.0 };
    }
    for i in 0..n - 1 {
        if d[i] == 0.0 {
            m[i] = 0.0;
            m[i + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[i] / d[i], m[i + 1] / d[i]);
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            m[i] = t * a * d[i];
            m[i + 1] = t * b * d[i];
        }
    }
    m
}

/// The curve's height at `x`.
fn spline(p: &[(f64, f64)], m: &[f64], x: f64) -> f64 {
    let i = p.partition_point(|q| q.0 <= x);
    if i == 0 {
        return p[0].1;
    }
    if i >= p.len() {
        return p[p.len() - 1].1;
    }
    let (a, b) = (p[i - 1], p[i]);
    let h = b.0 - a.0;
    let t = (x - a.0) / h;
    let (t2, t3) = (t * t, t * t * t);
    (2.0 * t3 - 3.0 * t2 + 1.0) * a.1 + (t3 - 2.0 * t2 + t) * h * m[i - 1] + (-2.0 * t3 + 3.0 * t2) * b.1 + (t3 - t2) * h * m[i]
}

/// A goal or a step with its box: (done, the text without the box).
pub fn boxed(item: &str) -> (bool, &str) {
    for (mark, done) in [("[x]", true), ("[X]", true), ("[ ]", false), ("[_]", false)] {
        if let Some(rest) = item.strip_prefix(mark) {
            return (done, rest.trim_start());
        }
    }
    (false, item)
}

/// The item with its box ticked, or cleared when it was ticked.
pub fn ticked(item: &str) -> String {
    let (done, text) = boxed(item);
    format!("{} {text}", if done { "[ ]" } else { "[x]" })
}

/// The first file or web address in a line: a word that starts with
/// `~/`, `/`, `http://` or `https://`, or anything inside `<...>` (for a
/// path with spaces in it).
pub fn link(item: &str) -> Option<String> {
    if let Some(a) = item.find('<') {
        if let Some(b) = item[a..].find('>') {
            let inner = item[a + 1..a + b].trim();
            if is_link(inner) {
                return Some(inner.to_string());
            }
        }
    }
    item.split_whitespace()
        .map(|w| w.trim_matches(|c: char| matches!(c, ',' | ';' | '(' | ')' | '"' | '\'')).trim_end_matches('.'))
        .find(|w| is_link(w))
        .map(String::from)
}

fn is_link(w: &str) -> bool {
    w.len() > 2 && (w.starts_with("~/") || w.starts_with('/') || w.starts_with("http://") || w.starts_with("https://"))
}

/// A path with `~/` as the home folder.
pub fn real_path(link: &str) -> String {
    match link.strip_prefix("~/") {
        Some(rest) => home().join(rest).to_string_lossy().into_owned(),
        None => link.to_string(),
    }
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

/// Where the people are: `COACHING_DIR`, or `~/.coaching`.
pub fn dir() -> PathBuf {
    std::env::var_os("COACHING_DIR").map(PathBuf::from).unwrap_or_else(|| home().join(".coaching"))
}

/// The files of the people, by name.
pub fn files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    v.sort();
    v
}

/// A file name for a person: letters and digits, the rest as `-`.
pub fn slug(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    s.trim_matches('-').to_string()
}

/// Write through a temporary file, so a crash never leaves half a file.
pub fn save(path: &Path, p: &Person) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, p.text())?;
    std::fs::rename(&tmp, path)
}

/// Lines of at most `w` cells, broken between words.
pub fn wrap(text: &str, w: usize) -> Vec<String> {
    let w = w.max(8);
    let mut out = Vec::new();
    for para in text.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let need = crust::display_width(&line) + crust::display_width(word) + usize::from(!line.is_empty());
            if need > w && !line.is_empty() {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        out.push(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALICE: &str = "name: Alice Example\ncolour: #88aaff\ncv: ~/Documents/alice-cv.pdf\n\n# Goals\n[ ] Speak in front of fifty people\n[x] Read a book a month\n\n# Roadmap\n[ ] 2026-12 The first course\n\n# Timeline\n2026-11-12 First session\n  She wants a plan, see <~/My Files/plan one.pdf>\n2026-11-01 We met\n";

    #[test]
    fn a_file_reads_and_writes_back_the_same() {
        let p = Person::parse(ALICE);
        assert_eq!(p.field("name"), Some("Alice Example"));
        assert_eq!(p.colour(), Some((0x88, 0xaa, 0xff)));
        assert_eq!(p.parts[GOALS].len(), 2);
        assert_eq!(p.parts[TIMELINE].len(), 2);
        assert_eq!(p.parts[TIMELINE][0], "2026-11-12 First session\nShe wants a plan, see <~/My Files/plan one.pdf>");
        assert_eq!(p.text(), ALICE);
        assert_eq!(Person::parse(&p.text()), p);
    }

    #[test]
    fn a_field_is_changed_in_place_or_added() {
        let mut p = Person::parse(ALICE);
        p.set_field("colour", "#112233");
        p.set_field("phone", "555 0100");
        // The colour stays off the card, and goes back under the name.
        assert_eq!(p.parts[CARD], vec!["name: Alice Example", "cv: ~/Documents/alice-cv.pdf", "phone: 555 0100"]);
        assert_eq!(p.colour(), Some((0x11, 0x22, 0x33)));
        assert!(p.text().starts_with("name: Alice Example\ncolour: #112233\ncv:"));
    }

    #[test]
    fn an_oca_line_gives_ten_scores_and_a_graph() {
        let mut p = Person::parse(ALICE);
        assert_eq!(p.oca(), None);
        p.set_field("oca", "2026-11-10 100, 100 100 100 100 -100 -100 -100 -100 -100");
        let v = p.oca().unwrap();
        assert_eq!(v, [100, 100, 100, 100, 100, -100, -100, -100, -100, -100]);
        let g = oca_graph(&v);
        assert_eq!(g.len(), OCA_ROWS);
        assert!(g.iter().all(|r| r.chars().count() == 10 * OCA_STEP));
        // The line runs along the top at the left and along the bottom at the right.
        let cell = |r: usize, c: usize| g[r].chars().nth(c).unwrap();
        assert_ne!(cell(0, 4), '\u{2800}');
        assert_eq!(cell(OCA_ROWS - 1, 4), '\u{2800}');
        assert_ne!(cell(OCA_ROWS - 1, 25), '\u{2800}');
        assert_eq!(cell(0, 25), '\u{2800}');
        // Nine scores, or one out of range, is no test.
        p.set_field("oca", "1 2 3 4 5 6 7 8 9");
        assert_eq!(p.oca(), None);
        p.set_field("oca", "1 2 3 4 5 6 7 8 9 150");
        assert_eq!(p.oca(), None);
    }

    #[test]
    fn the_pixel_graph_has_a_shade_per_group_and_stays_in_its_box() {
        let shades = oca_shades((0xff, 0x88, 0x00));
        assert_eq!(shades[0], shades[2]);
        assert_eq!(shades[4], (0xff, 0x88, 0x00));
        assert_eq!(shades[3], (205, 205, 210));
        assert_ne!(shades[0], shades[4]);
        assert_ne!(shades[4], shades[7]);
        let v = [35, -20, 60, 10, -45, 70, 25, -10, 40, 55];
        let cv = oca_canvas(&v, &shades, Some((13, 28)));
        assert_eq!((cv.w, cv.h), (390, 140));
        let at = |x: usize, y: usize| { let i = (y * cv.w + x) * 4; (cv.rgba[i], cv.rgba[i + 1], cv.rgba[i + 2], cv.rgba[i + 3]) };
        // The dot of D is light grey, and the corner is see-through.
        let (dx, dy) = (136, (5.0 + 90.0 / 200.0 * 130.0) as usize);
        assert_eq!(at(dx, dy), (205, 205, 210, 255));
        assert_eq!(at(0, 0).3, 0);
        if let Ok(file) = std::env::var("OCA_DUMP") {
            std::fs::write(file, cv.png()).unwrap();
        }
    }

    #[test]
    fn a_box_is_ticked_and_cleared() {
        assert_eq!(boxed("[x] Read"), (true, "Read"));
        assert_eq!(boxed("Read"), (false, "Read"));
        assert_eq!(ticked("[ ] Read"), "[x] Read");
        assert_eq!(ticked("[x] Read"), "[ ] Read");
        assert_eq!(ticked("Read"), "[x] Read");
    }

    #[test]
    fn the_link_in_a_line_is_found() {
        assert_eq!(link("cv: ~/Documents/alice-cv.pdf").as_deref(), Some("~/Documents/alice-cv.pdf"));
        assert_eq!(link("See (https://example.com/a), then go on").as_deref(), Some("https://example.com/a"));
        assert_eq!(link("the plan <~/My Files/plan one.pdf> is new").as_deref(), Some("~/My Files/plan one.pdf"));
        assert_eq!(link("It is in /tmp/notes.txt.").as_deref(), Some("/tmp/notes.txt"));
        assert_eq!(link("a / b, and 3 < 4 > 2"), None);
    }

    #[test]
    fn long_text_breaks_between_words() {
        assert_eq!(wrap("one two three four", 9), vec!["one two", "three", "four"]);
        assert_eq!(wrap("a\nb", 20), vec!["a", "b"]);
        assert_eq!(slug(" Alice Example! "), "alice-example");
    }
}
