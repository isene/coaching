//! The person's appointments, read from tock's calendar.
//!
//! tock's store takes one program at a time, and tock or gluon has it
//! open all day. So this copies the store's two files to a private
//! folder in memory and reads the copy. That is 6 MB or so, done when
//! the calendar is opened and again only when tock has written since.

use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Appt {
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub title: String,
    pub place: String,
}

/// tock's store: `TOCK_STORE`, or `~/.tock/tock.ferrite`.
pub fn store() -> PathBuf {
    std::env::var_os("TOCK_STORE").map(PathBuf::from).unwrap_or_else(|| crate::data::home().join(".tock/tock.ferrite"))
}

/// When tock last wrote: the appointments read before are good until
/// this changes.
pub fn stamp(store: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(store.join("log")).and_then(|m| m.modified()).ok()
}

/// The words that tie an appointment to a person: the card's
/// `calendar:` line (split at commas) when it has one, else the full
/// name, the first name and the `email:` line.
pub fn needles(p: &crate::data::Person) -> Vec<String> {
    let list = |s: &str| s.split(',').map(|w| w.trim().to_lowercase()).filter(|w| !w.is_empty()).collect::<Vec<_>>();
    if let Some(own) = p.field("calendar").map(list).filter(|l| !l.is_empty()) {
        return own;
    }
    let mut out = Vec::new();
    if let Some(name) = p.field("name") {
        out.extend(list(name));
        out.extend(name.split_whitespace().next().map(str::to_lowercase));
    }
    out.extend(p.field("email").map(list).unwrap_or_default());
    out.dedup();
    out
}

/// True when `needle` is in `text` as whole words. Both in small letters.
pub fn has_word(text: &str, needle: &str) -> bool {
    let edge = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    text.match_indices(needle).any(|(i, m)| edge(text[..i].chars().next_back()) && edge(text[i + m.len()..].chars().next()))
}

/// The appointments with one of the needles in the title or among the
/// guests, the earliest first.
pub fn load(store: &Path, needles: &[String]) -> Result<Vec<Appt>, String> {
    if needles.is_empty() {
        return Ok(Vec::new());
    }
    let room = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let copy = room.join(format!("coaching-tock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&copy);
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(&copy).map_err(|e| e.to_string())?;
    }
    let read: Result<Vec<Vec<ferrite::Value>>, String> = (|| {
        for f in ["snapshot", "log"] {
            if store.join(f).exists() {
                std::fs::copy(store.join(f), copy.join(f)).map_err(|e| format!("tock's calendar: {e}"))?;
            }
        }
        let db = ferrite::Db::open(&copy).map_err(|e| format!("tock's calendar: {e}"))?;
        let found = db.query("SELECT title, start_time, end_time, all_day, location, attendees FROM events ORDER BY start_time", &[])
            .map_err(|e| format!("tock's calendar: {e}"))?;
        Ok(found.rows().to_vec())
    })();
    let _ = std::fs::remove_dir_all(&copy);
    let rows = read?;
    let text = |v: &ferrite::Value| match v { ferrite::Value::Text(t) => t.clone(), _ => String::new() };
    let int = |v: &ferrite::Value| v.as_int().unwrap_or(0);
    let mut out: Vec<Appt> = rows.iter().filter(|r| r.len() >= 6).filter_map(|r| {
        let (title, guests) = (text(&r[0]), text(&r[5]).to_lowercase());
        let low = title.to_lowercase();
        let hit = needles.iter().any(|n| has_word(&low, n) || (n.contains('@') && guests.contains(n.as_str())) || (n.contains(' ') && guests.contains(n.as_str())));
        hit.then(|| Appt { start: int(&r[1]), end: int(&r[2]), all_day: int(&r[3]) != 0, title, place: text(&r[4]) })
    }).collect();
    out.dedup();
    Ok(out)
}

/// A moment by the clock on the wall here.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Local {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    /// Monday is 0.
    pub weekday: u32,
}

pub fn local(ts: i64) -> Local {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    Local { year: tm.tm_year + 1900, month: tm.tm_mon as u32 + 1, day: tm.tm_mday as u32, hour: tm.tm_hour as u32, min: tm.tm_min as u32, weekday: (tm.tm_wday as u32 + 6) % 7 }
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub const DAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub fn days_in(year: i32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 31,
    }
}

/// The weekday of the first of a month, Monday as 0 (Sakamoto's rule).
pub fn first_weekday(year: i32, month: u32) -> u32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let sunday0 = (y + y / 4 - y / 100 + y / 400 + T[month as usize - 1] + 1).rem_euclid(7);
    (sunday0 as u32 + 6) % 7
}

/// The month `by` months after (or before) this one.
pub fn month_after(year: i32, month: u32, by: i32) -> (i32, u32) {
    let n = year * 12 + month as i32 - 1 + by;
    (n.div_euclid(12), n.rem_euclid(12) as u32 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn months_have_their_days_and_first_weekdays() {
        assert_eq!(days_in(2024, 2), 29);
        assert_eq!(days_in(2026, 2), 28);
        assert_eq!(days_in(2026, 11), 30);
        // 1 November 2026 is a Sunday, 1 January 2026 a Thursday.
        assert_eq!(first_weekday(2026, 11), 6);
        assert_eq!(first_weekday(2026, 1), 3);
        assert_eq!(first_weekday(2024, 2), 3);
        assert_eq!(month_after(2026, 11, 2), (2027, 1));
        assert_eq!(month_after(2026, 1, -1), (2025, 12));
    }

    #[test]
    fn a_name_matches_as_whole_words_only() {
        assert!(has_word("coaching with alice", "alice"));
        assert!(has_word("alice: session 3", "alice"));
        assert!(!has_word("malice aforethought", "alice"));
        assert!(!has_word("alicent", "alice"));
    }

    #[test]
    fn the_card_says_what_to_look_for() {
        let p = crate::data::Person::parse("name: Alice Example\nemail: alice@example.com\n");
        assert_eq!(needles(&p), vec!["alice example", "alice", "alice@example.com"]);
        let p = crate::data::Person::parse("name: Alice Example\ncalendar: AE coaching, Example\n");
        assert_eq!(needles(&p), vec!["ae coaching", "example"]);
    }

    #[test]
    fn appointments_are_read_from_a_copy_of_the_store() {
        let dir = std::env::temp_dir().join(format!("coaching-test-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        {
            let mut db = ferrite::Db::open(&dir).unwrap();
            db.execute_batch("CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, start_time INTEGER NOT NULL, end_time INTEGER, all_day INTEGER, location TEXT, attendees TEXT);").unwrap();
            for (title, start, guests) in [("Coaching Alice", 2000, ""), ("Dentist", 1500, ""), ("Planning", 1000, "[{\"email\":\"alice@example.com\"}]"), ("Malice", 500, "")] {
                db.execute("INSERT INTO events (title, start_time, end_time, all_day, location, attendees) VALUES (?1, ?2, ?3, 0, 'Room 1', ?4)",
                    &[ferrite::Value::Text(title.into()), ferrite::Value::Int(start), ferrite::Value::Int(start + 60), ferrite::Value::Text(guests.into())]).unwrap();
            }
        }
        let found = load(&dir, &["alice".into(), "alice@example.com".into()]).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(), vec!["Planning", "Coaching Alice"]);
        assert_eq!(found[1].place, "Room 1");
    }
}
