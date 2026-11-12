# coaching

The people you coach, in the terminal. Part of the
[Fe₂O₃ suite](https://isene.org/fe2o3/).

Each person has one page:

- a **card**: name, a picture shown on the page, a link to the CV, and
  any other line you add;
- their **goals**, each with a box to tick;
- a **roadmap**: the steps you plan, each with a box to tick;
- a **timeline**: dated notes, the newest first;
- a **calendar**: the person's appointments in
  [tock](https://github.com/isene/tock), as months with the days lit
  in the person's colour, and as a list;
- **Claude** in a pane under the page, told who is on the page.

The whole page takes the person's colour (the bar, the headings, the
dates), so you see at a glance whose page is open.

## Install

```bash
git clone https://github.com/isene/crust
git clone https://github.com/isene/glow
git clone https://github.com/isene/ferrite
git clone https://github.com/isene/deck
git clone https://github.com/isene/coaching
cd coaching && cargo build --release
ln -s "$PWD/target/release/coaching" ~/bin/coaching
```

## Keys

| Key | What it does |
|---|---|
| ← → | The person before, the next one |
| Tab, Shift+Tab | The next part: card, goals, roadmap, timeline, calendar, Claude |
| ↑ ↓ Home End | The line in the part |
| Shift+↑ Shift+↓ | Move the line up or down |
| + | A new line in this part |
| n | A new note in the timeline, dated today |
| N | A note of many lines, in your editor |
| a | The calendar, in the timeline's place |
| x, Space | Tick a goal or a step, or clear the tick |
| Enter | Open the file or web address in the line; else change the line |
| e | Change the line |
| E | The person's whole file in your editor |
| Del, Ctrl+D | Delete the line |
| c | The person's colour, picked in [prism](https://github.com/isene/prism) |
| p | A new person, then prism for the colour |
| F1 | The keys |
| Esc | The keys to Claude; Tab there comes back to the page |
| Ctrl+Z | Claude on the whole screen, and back |
| q | Quit |

A line with a file or a web address in it has a ↗ in front. Enter opens
it with `xdg-open`.

## The files

A person is one text file in `~/.coaching` (or in the folder
`COACHING_DIR` names). You can write it by hand:

```text
name: Alice Example
colour: #66bbff
picture: ~/Pictures/alice.jpg
cv: ~/Documents/alice-cv.pdf
born: 1990

# Goals
[ ] Speak in front of fifty people without notes
[x] Read a book a month

# Roadmap
[x] 2026-11 The first course
[ ] 2027-01 The second course

# Timeline
2026-11-12 Second session. Her plan is in <~/My Files/plan one.pdf>
2026-11-01 First call
```

- The lines before the first heading are the card.
- A line that starts with a space belongs to the line above it.
- A link is a word that starts with `~/`, `/`, `http://` or `https://`.
  Put a path with spaces in `<angle brackets>`.
- The picture shows in a terminal that shows images (kitty, glass);
  elsewhere the card has text alone.
- To remove a person, delete the file.

## The calendar

`a` shows the appointments tock has for the person: every event with
the person's full name or first name in its title, or the card's
`email:` among its guests. A card line `calendar: word, other word`
names the words to look for instead.

The months start at the picked appointment; ↑ and ↓ walk the list, and
it opens at the next one to come. Appointments are made in tock.
coaching reads a copy of tock's store, so tock can stay open.

## Claude

The lower part of the screen is [deck](https://github.com/isene/deck),
a front for Claude Code. It needs `claude` installed; `coaching -n`
runs without it.

- When the keys move to the pane, Claude is told who is on the page:
  the person's file, the picked line and the appointments.
- Claude works in `~/.coaching`, so it reads and changes the files, and
  the page shows a change at once.
- It is the same session each time, tagged `#coaching`.
- `~/.coaching/CLAUDE.md` says how the files are built and which skills
  to read. It is written once; change it to suit you.

The editor for `N` and `E` is `$VISUAL`, then `$EDITOR`, then scribe.

## License

Public domain ([Unlicense](LICENSE)).
