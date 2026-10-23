//! THEATER MODE and the arcade ending (record 301, mode 19 0x800FF0C4): the film-strip grid of movies unlocked by
//! the endings seen, the music player (once every movie is there), the ending movie of the champion and the staff
//! roll after it. Lists, layouts, strings and timings from the converted theater.json.

use std::sync::Arc;

use egui::{Color32, Pos2};
use serde_json::Value;

use crate::art::{additive, Art, Screen as View};
use crate::menu::Key;
use crate::movie::{Caption, Params};

fn num(v: &Value) -> i32 {
    v.as_i64().unwrap_or(0) as i32
}

fn rgb(v: &Value) -> Color32 {
    Color32::from_rgb(num(&v[0]) as u8, num(&v[1]) as u8, num(&v[2]) as u8)
}

/// One cell of the grid (0x800BA160).
#[derive(Clone, Debug)]
pub struct Entry {
    pub movie: usize,
    pub movie_alt: usize,
    pub picture: String,
    pub mask: u64,
    pub kind: u32,
    pub lines: [Option<String>; 3],
}

/// theater.json.
pub struct TheaterData {
    pub raw: Value,
    pub entries: Vec<Option<Entry>>,
}

impl TheaterData {
    pub fn load(gamedata: &std::path::Path) -> Option<Self> {
        let raw: Value = serde_json::from_slice(&kumite_fs::read(&gamedata.join("theater.json")).ok()?).ok()?;
        let entries = raw["entries"]
            .as_array()?
            .iter()
            .map(|e| {
                e.is_object().then(|| Entry {
                    movie: e["movie"].as_u64().unwrap_or(0) as usize,
                    movie_alt: e["movie_alt"].as_u64().unwrap_or(0) as usize,
                    picture: e["picture"].as_str().unwrap_or_default().to_string(),
                    mask: e["mask"].as_u64().unwrap_or(0),
                    kind: e["kind"].as_u64().unwrap_or(0) as u32,
                    lines: [0, 1, 2].map(|k| e[format!("line{}", k + 1)].as_str().map(String::from)),
                })
            })
            .collect();
        Some(Self { raw, entries })
    }

    /// An entry's movie, or None when locked (0x801004A8): kind 0 by the endings seen, 1 / 2 the alternate
    /// cut once its alternate ending was seen, 3 only by the alternate (an alternate costume's own character).
    pub fn status(&self, e: &Entry, seen: u64, alt: u32) -> Option<usize> {
        if e.mask == 0 {
            return Some(e.movie);
        }
        let bit = |k: u32| self.raw["status"]["alternate_bits"][k.to_string()].as_u64().unwrap_or(0) as u32;
        match e.kind {
            0 => (seen & e.mask != 0).then_some(e.movie),
            1 | 2 => {
                if alt & (1 << bit(e.kind)) != 0 {
                    Some(e.movie_alt)
                } else {
                    (seen & e.mask != 0).then_some(e.movie)
                }
            }
            _ => (alt & (1 << bit(e.kind)) != 0).then_some(e.movie),
        }
    }

    /// The title's first line: a name variant ('0'..'3', 0x800FFE60) by which of its two endings were seen.
    pub fn name(&self, e: &Entry, seen: u64, alt: u32) -> String {
        let seen = seen as u32;
        let l1 = e.lines[0].clone().unwrap_or_default();
        let v = &self.raw["names"][&l1];
        if !v.is_object() {
            return l1;
        }
        let test = |k: usize| {
            let b = &v["bits"][k];
            let flags = if b[0].as_str() == Some("alternate") { alt } else { seen };
            flags & (1 << num(&b[1])) != 0
        };
        let i = match (test(0), test(1)) {
            (_, false) => 0,
            (false, true) => 1,
            (true, true) => 2,
        };
        v["names"][i].as_str().unwrap_or_default().to_string()
    }

    /// Movie parameters `p` (theater.json `movies`) with their captions.
    pub fn params(&self, p: usize) -> Option<Params> {
        let m = self.raw["movies"].get(p)?;
        let file = m["file"].as_str()?.parse().ok()?;
        let mut captions = Vec::new();
        if let Some(g) = m["captions"].as_u64() {
            let c = &self.raw["captions"];
            let list = c["schedule"][g as usize].as_array().cloned().unwrap_or_default();
            for (k, s) in list.iter().enumerate() {
                let Ok(id) = usize::try_from(num(&s[1])) else { continue };
                let from = num(&s[0]) as usize;
                let to = list.get(k + 1).map_or(usize::MAX, |n| num(&n[0]) as usize);
                let pic = &c["pictures"][id];
                captions.push(Caption {
                    file: pic["picture"].as_str().unwrap_or_default().to_string(),
                    at: [num(&pic["x"]) as f32, num(&pic["y"]) as f32, num(&pic["w"]) as f32, num(&pic["h"]) as f32],
                    frames: (from, to),
                });
            }
        }
        Some(Params {
            file,
            frames: num(&m["frames"]) as usize,
            size: (num(&m["w"]) as f32, num(&m["h"]) as f32),
            looped: m["loop"].as_bool().unwrap_or(false),
            white: m["white"].as_bool().unwrap_or(false),
            captions,
        })
    }

    /// The arcade ending of a character id and costume (0x800FEFD4); one character's long cut after a second clear.
    pub fn ending(&self, character: usize, costume: usize, alt: u32) -> Option<usize> {
        let e = &self.raw["ending"];
        // the extra fighters: their own endings, by costume (a second costume may have its own)
        if let Some(row) = e["extra"][character.to_string()].as_array() {
            return row.get(costume).or(row.first()).and_then(|v| v.as_u64()).map(|v| v as usize);
        }
        let sc = &e["second_clear"];
        // (its alternate bit: the second-clear rule's; none, no long cut)
        let bit = e["alternate"]["rules"].as_array().into_iter().flatten().find(|r| r["second_clear"].as_bool() == Some(true)).and_then(|r| r["bit"].as_u64());
        let mask = bit.and_then(|b| 1u32.checked_shl(u32::try_from(b).ok()?)).unwrap_or(0);
        if character == num(&sc["character"]) as usize && alt & mask != 0 {
            return Some(num(&sc["movie"]) as usize);
        }
        let last = e["table"].as_array().map_or(0, |t| t.len().saturating_sub(1));
        let row = &e["table"][character.min(last)];
        let p = row[costume.min(3)].as_u64().filter(|&p| p != 255).or_else(|| row[0].as_u64())?;
        Some(p as usize)
    }

    /// The flags a clear sets (0x800B1E88): the ending seen, or the alternate one (the alternates theater.json names,
    /// the long cut's second clear). The extra fighters' ending bit of a costume (theater.json `ending.extra_bits`).
    pub fn extra_bit(&self, character: usize, costume: usize) -> Option<u32> {
        let row = self.raw["ending"]["extra_bits"][character.to_string()].as_array()?;
        row.get(costume).or(row.first()).and_then(|v| v.as_u64()).map(|v| v as u32)
    }

    /// The Theater's locks: the original's endings (bits 0..31) and the extra fighters' (32..).
    pub fn locks(endings: u32, endings_extra: u32) -> u64 {
        endings as u64 | (endings_extra as u64) << 32
    }

    pub fn clear(&self, character: usize, costume: usize, seen: &mut u32, alt: &mut u32) {
        for r in self.raw["ending"]["alternate"]["rules"].as_array().into_iter().flatten() {
            if num(&r["character"]) as usize != character {
                continue;
            }
            let bit = 1 << num(&r["bit"]);
            let hit = if let Some(c) = r["costume_at_least"].as_u64() {
                costume >= c as usize
            } else if let Some(c) = r["costume"].as_u64() {
                costume == c as usize
            } else {
                *seen & bit != 0
            };
            if hit {
                *alt |= bit;
            } else {
                *seen |= bit;
            }
            return;
        }
        // (the ending bits are the original's characters'; the extra fighters have none)
        if character < 32 && self.raw["ending"]["extra"][character.to_string()].is_null() {
            *seen |= 1 << character;
        }
    }
}

/// Where the cursor is (ctx+0xA): the grid / the list, or a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Grid,
    Exit,
    Music,
    Theater,
    Bgm,
}

/// What the Theater wants from the app.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    None,
    /// Play movie parameters N (the menu comes back after it).
    Movie(usize),
    /// The music player: play BGM id in column (0 remixed, 1 arcade), or stop.
    Music(Option<(usize, usize)>),
    /// EXIT / SELECT: the main menu.
    Exit,
}

#[derive(Clone)]
pub struct Theater {
    pub data: Arc<TheaterData>,
    /// The locks: the original's endings seen, the extra fighters' above bit 32 ([`TheaterData::locks`]).
    seen: u64,
    alt: u32,
    all: bool,
    cursor: usize,
    scroll: usize,
    focus: Focus,
    /// The music player is open, its arcade column, list cursor and scroll.
    music: Option<bool>,
    track: usize,
    track_scroll: usize,
    idle: u32,
    pub frame: u32,
}

impl Theater {
    pub fn new(data: Arc<TheaterData>, seen: u64, alt: u32) -> Self {
        // the music player opens once every original movie is there (the extra fighters' endings are extra)
        let all = data.entries.iter().flatten().filter(|e| e.mask >> 32 == 0).all(|e| data.status(e, seen, alt).is_some());
        Self { data, seen, alt, all, cursor: 0, scroll: 0, focus: Focus::Grid, music: None, track: 0, track_scroll: 0, idle: 0, frame: 0 }
    }

    pub fn tick(&mut self) {
        self.frame += 1;
        self.idle += 1;
    }

    fn columns(&self) -> usize {
        // (from theater.json: in range whatever it says)
        self.data.raw["columns"].as_u64().unwrap_or(6).clamp(1, 64) as usize
    }

    fn rows(&self) -> usize {
        self.data.raw["rows"].as_u64().unwrap_or(4).clamp(1, 64) as usize
    }

    fn cell(&self, i: usize) -> Option<&Entry> {
        self.data.entries.get(i).and_then(|e| e.as_ref())
    }

    fn sfx(&self, k: &str) {
        crate::menu::sfx::play(num(&self.data.raw["screen"]["sfx"][k]) as u16);
    }

    /// A key (0x801021BC / 0x80102740 for the music player).
    pub fn key(&mut self, key: Key) -> Event {
        self.idle = 0;
        if key == Key::Back {
            self.sfx("confirm");
            return Event::Exit;
        }
        let confirm = matches!(key, Key::Start | Key::Button(_));
        let before = (self.cursor, self.focus, self.track);
        let out = match self.music {
            None => self.theater_key(key, confirm),
            Some(arcade) => self.music_key(key, confirm, arcade),
        };
        if (self.cursor, self.focus, self.track) != before && out == Event::None {
            self.sfx("cursor");
        }
        out
    }

    fn theater_key(&mut self, key: Key, confirm: bool) -> Event {
        let (cols, rows) = (self.columns(), self.rows());
        let (row, col) = (self.cursor / cols, self.cursor % cols);
        if self.focus == Focus::Grid {
            if confirm {
                return match self.cell(self.cursor).and_then(|e| self.data.status(e, self.seen, self.alt)) {
                    Some(m) => Event::Movie(m),
                    None => Event::None,
                };
            }
            match key {
                Key::Left if col > 0 && self.cell(self.cursor - 1).is_some() => self.cursor -= 1,
                Key::Right if col + 1 < cols && self.cell(self.cursor + 1).is_some() => self.cursor += 1,
                Key::Up | Key::Down => {
                    // the nearest row up / down with a cell, its cell nearest the column; none below: the buttons
                    let total = self.data.entries.len().div_ceil(cols);
                    let has = |r: usize| (0..cols).any(|k| self.cell(r * cols + k).is_some());
                    let target = if key == Key::Up { (0..row).rev().find(|&r| has(r)) } else { (row + 1..total).find(|&r| has(r)) };
                    match target {
                        None if key == Key::Down => self.focus = if self.all { Focus::Music } else { Focus::Exit },
                        None => {}
                        Some(r) => {
                            let k = (0..cols).filter(|&k| self.cell(r * cols + k).is_some()).min_by_key(|&k| k.abs_diff(col)).unwrap_or(0);
                            let c = r * cols + k;
                            self.cursor = c;
                            self.scroll = self.scroll.min(r).max((r + 1).saturating_sub(rows));
                        }
                    }
                }
                _ => {}
            }
            return Event::None;
        }
        if confirm {
            self.sfx("confirm");
            return match self.focus {
                Focus::Music => {
                    self.music = Some(false);
                    self.focus = Focus::Grid;
                    Event::Music(None)
                }
                _ => Event::Exit,
            };
        }
        match key {
            Key::Up => self.focus = Focus::Grid,
            Key::Left if self.all && self.focus == Focus::Exit => self.focus = Focus::Music,
            Key::Right if self.all && self.focus == Focus::Music => self.focus = Focus::Exit,
            _ => {}
        }
        Event::None
    }

    fn music_key(&mut self, key: Key, confirm: bool, arcade: bool) -> Event {
        let n = self.data.raw["music"]["tracks"].as_array().map_or(0, Vec::len);
        let window = num(&self.data.raw["screen"]["music_player"]["rows"]).clamp(1, 64) as usize;
        let column = usize::from(arcade);
        if self.focus == Focus::Grid {
            if confirm {
                let id = num(&self.data.raw["music"]["tracks"][self.track]["bgm"]) as usize;
                return Event::Music(Some((id, column)));
            }
            match key {
                Key::Up if self.track > 0 => self.track -= 1,
                Key::Down if self.track + 1 < n => self.track += 1,
                Key::Right => self.focus = Focus::Theater,
                _ => {}
            }
            self.track_scroll = self.track_scroll.min(self.track).max((self.track + 1).saturating_sub(window));
            return Event::None;
        }
        // the buttons, top to bottom: THEATER, BGM SELECT, EXIT
        let order = [Focus::Theater, Focus::Bgm, Focus::Exit];
        let at = order.iter().position(|f| *f == self.focus).unwrap_or(0);
        if confirm {
            self.sfx("confirm");
            return match self.focus {
                Focus::Theater => {
                    self.music = None;
                    self.focus = Focus::Grid;
                    Event::Music(None)
                }
                Focus::Bgm => {
                    self.music = Some(!arcade);
                    Event::Music(None)
                }
                _ => Event::Exit,
            };
        }
        match key {
            Key::Up if at > 0 => self.focus = order[at - 1],
            Key::Down if at + 1 < order.len() => self.focus = order[at + 1],
            Key::Left => self.focus = Focus::Grid,
            _ => {}
        }
        Event::None
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art) {
        let s = &self.data.raw["screen"];
        let bg = &s["background"];
        let full = crate::art::FULL;
        let (top, bottom) = (rgb(&bg["top"]), rgb(&bg["bottom"]));
        Art::shade(painter, v, full, [top, top, bottom, bottom]);
        let p = &s["panel"];
        let panel = [num(&p["x"]) as f32, num(&p["y"]) as f32, num(&p["w"]) as f32, num(&p["h"]) as f32];
        let (pt, pb) = (rgb(&p["top"]), rgb(&p["bottom"]));
        Art::shade(painter, v, panel, [pt, pt, pb, pb].map(|c| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 128)));
        outline(painter, v, panel, 3.0, 4.0, Color32::WHITE);
        let title = self.music.map_or(0, |a| if a { 2 } else { 1 });
        self.header(painter, v, art, title);
        match self.music {
            None => self.draw_theater(painter, v, art),
            Some(arcade) => self.draw_music(painter, v, art, arcade),
        }
    }

    /// The header (0x80102CA8): the title; after 480 idle frames it swaps with PUSH START TO PLAY every 256.
    fn header(&self, painter: &egui::Painter, v: &View, art: &Art, title: usize) {
        let h = &self.data.raw["screen"]["header"];
        let y = num(&h["y"]) as f32;
        let a = self.idle.saturating_sub(num(&h["idle"]) as u32) as i32;
        let sw = crate::art::SCREEN_W;
        let mut cx = if a & 0x100 != 0 { sw * 1.5 } else { sw / 2.0 };
        if a & 0xE0 == 0xE0 {
            // (a screen's width over 32 frames)
            let d = (a & 31) as f32 * (sw / 16.0) / 2.0;
            cx += if a & 0x100 == 0 { d } else { -d };
        }
        let t = h["title"][title].as_str().unwrap_or_default();
        let colour = num(&h["title_colours"][title]) as u32;
        printf(painter, v, art, 1, &[6, colour], cx - art.text_w(1, t) / 2.0, y, t);
        let push = h["push"].as_str().unwrap_or_default();
        let cols: Vec<u32> = h["push_colours"].as_array().into_iter().flatten().map(|c| num(c) as u32).collect();
        printf(painter, v, art, 0, &cols, cx - art.text_w(0, push) / 2.0 - sw, y, push);
    }

    fn pulse(&self) -> Color32 {
        let b = &self.data.raw["screen"]["buttons"]["pulse"];
        let level = 128 + crate::menu::triangle(self.frame * 16) / 2;
        let k = |c: &Value| (num(c) as u32 * level / 256) as u8;
        Color32::from_rgb(k(&b[0]), k(&b[1]), k(&b[2]))
    }

    /// A button (0x80100C2C): its label in a white-edged box, the focused one pulsing blue.
    #[allow(clippy::too_many_arguments)]
    fn button(&self, painter: &egui::Painter, v: &View, art: &Art, label: &str, x: f32, y: f32, focused: bool) {
        let b = &self.data.raw["screen"]["buttons"];
        let (mx, my) = (num(&b["margin"][0]) as f32, num(&b["margin"][1]) as f32);
        let rect = [x - mx, y - my, art.text_w(0, label) + 2.0 * mx, 2.0 * my + num(&b["h"]) as f32];
        let fill = if focused { self.pulse() } else { rgb(&b["fill"]) };
        painter.rect_filled(v.rect(rect[0], rect[1], rect[2], rect[3]), 0.0, fill);
        outline(painter, v, rect, 2.0, 2.0, Color32::WHITE);
        art.text(painter, v, 0, num(&b["colours"][usize::from(focused)]) as u32, x, y, label, Color32::WHITE);
    }

    fn help(&self, painter: &egui::Painter, v: &View, art: &Art, text: &str) {
        let h = &self.data.raw["screen"]["help"];
        art.text(painter, v, 0, num(&h["colour"]) as u32, (crate::art::SCREEN_W - art.text_w(0, text)) / 2.0, num(&h["y"]) as f32, text, Color32::WHITE);
    }

    fn draw_theater(&self, painter: &egui::Painter, v: &View, art: &Art) {
        let s = &self.data.raw["screen"];
        let g = &s["grid"];
        let (cols, rows) = (self.columns(), self.rows());
        let (gx, gy, gw, gh) = (num(&g["x"]) as f32, num(&g["y"]) as f32, num(&g["w"]) as f32, num(&g["h"]) as f32);
        let pic = |file: &str, x: f32, y: f32| {
            if let Some((tex, size)) = art.pic(file) {
                let r = v.rect(x, y, size[0] as f32, size[1] as f32);
                painter.image(tex.id(), r, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
        };
        let frame = g["frame"].as_str().unwrap_or_default();
        for r in self.scroll..(self.scroll + rows) {
            for c in 0..cols {
                let Some(e) = self.cell(r * cols + c) else { continue };
                let (x, y) = (gx + gw * c as f32, gy + gh * (r - self.scroll) as f32);
                match self.data.status(e, self.seen, self.alt) {
                    Some(_) => pic(&e.picture, x, y),
                    None => pic(g["locked"].as_str().unwrap_or_default(), x, y),
                }
                pic(frame, x, y);
            }
        }
        let i = &s["info"];
        if self.focus == Focus::Grid {
            let cur = &s["cursor"];
            if self.frame & num(&cur["blink"]) as u32 != 0 {
                let (r, c) = (self.cursor / cols - self.scroll, self.cursor % cols);
                let rect = [num(&cur["x"]) as f32 + gw * c as f32, num(&cur["y"]) as f32 + gh * r as f32, num(&cur["w"]) as f32, num(&cur["h"]) as f32];
                outline(painter, v, rect, 3.0, 4.0, rgb(&cur["colour"]));
            }
            if let Some(e) = self.cell(self.cursor) {
                if self.data.status(e, self.seen, self.alt).is_some() {
                    pic(&e.picture, num(&i["picture"][0]) as f32, num(&i["picture"][1]) as f32);
                    let n = &i["name"];
                    let name = self.data.name(e, self.seen, self.alt);
                    let x = num(&n["x"]) as f32 + (num(&n["width"]) as f32 - art.text_w(1, &name)) / 2.0;
                    art.text(painter, v, 1, num(&n["colour"]) as u32, x, num(&n["y"]) as f32, &name, Color32::WHITE);
                    let l = &i["line2"];
                    let ys: Vec<f32> = if e.lines[2].is_some() { vec![num(&l["y"][1]) as f32, num(&l["y"][2]) as f32] } else { vec![num(&l["y"][0]) as f32] };
                    for (text, y) in e.lines[1..].iter().flatten().zip(ys) {
                        let x = num(&l["x"]) as f32 + (num(&l["width"]) as f32 - art.text_w(0, text)) / 2.0;
                        art.text(painter, v, 0, num(&l["colour"]) as u32, x, y, text, Color32::WHITE);
                    }
                } else {
                    let l = &i["locked"];
                    art.text(painter, v, 1, num(&l["colour"]) as u32, num(&l["x"]) as f32, num(&l["y"]) as f32, l["text"].as_str().unwrap_or("???"), Color32::WHITE);
                }
            }
        }
        let b = &s["buttons"];
        let y = num(&b["y"]) as f32;
        let label = |k: &str| b["labels"][k].as_str().unwrap_or_default().to_string();
        if self.all {
            self.button(painter, v, art, &label("music"), num(&b["theater_x"]["all"][1]) as f32, y, self.focus == Focus::Music);
            self.button(painter, v, art, &label("exit"), num(&b["theater_x"]["all"][2]) as f32, y, self.focus == Focus::Exit);
        } else {
            self.button(painter, v, art, &label("exit"), num(&b["theater_x"]["exit_only"]) as f32, y, self.focus == Focus::Exit);
        }
        let h = &s["help"]["theater"];
        match self.focus {
            Focus::Exit => self.help(painter, v, art, h["exit"].as_str().unwrap_or_default()),
            Focus::Music => self.help(painter, v, art, h["music"].as_str().unwrap_or_default()),
            _ => {}
        }
    }

    fn draw_music(&self, painter: &egui::Painter, v: &View, art: &Art, arcade: bool) {
        let s = &self.data.raw["screen"];
        let m = &s["music_player"];
        let pic = |file: &str, x: f32, y: f32| -> Option<f32> {
            let (tex, size) = art.pic(file)?;
            painter.image(tex.id(), v.rect(x, y, size[0] as f32, size[1] as f32), egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            Some(size[0] as f32)
        };
        pic("theater/cover.png", num(&m["cover"][0]) as f32, num(&m["cover"][1]) as f32);
        pic("theater/spine.png", num(&m["spine"][0]) as f32, num(&m["spine"][1]) as f32);
        let f = &m["frame"];
        outline(painter, v, [num(&f[0]) as f32, num(&f[1]) as f32, num(&f[2]) as f32, num(&f[3]) as f32], 2.0, 2.0, Color32::WHITE);
        let row = &m["row"];
        let tracks = self.data.raw["music"]["tracks"].as_array().cloned().unwrap_or_default();
        let window = num(&m["rows"]).clamp(1, 64) as usize;
        let list = self.focus == Focus::Grid;
        for (k, t) in tracks.iter().enumerate().skip(self.track_scroll).take(window) {
            let y = num(&row["y"]) as f32 + num(&row["dy"]) as f32 * (k - self.track_scroll) as f32;
            if list && k == self.track {
                let b = &m["bar"];
                let dy = y - num(&row["y"]) as f32;
                painter.rect_filled(v.rect(num(&b[0]) as f32, num(&b[1]) as f32 + dy, num(&b[2]) as f32, num(&b[3]) as f32), 0.0, self.pulse());
            }
            let colour = if list && k == self.track { num(&row["colours"][2]) } else { num(&row["colours"][usize::from(arcade)]) };
            let text = format!("{:2}:{}", k + 1, t["name"].as_str().unwrap_or_default());
            art.text(painter, v, num(&row["font"]) as usize, colour as u32, num(&row["x"]) as f32, y, &text, Color32::WHITE);
        }
        let b = &s["buttons"];
        let bx = num(&m["buttons_x"]) as f32;
        for (key, focus) in [("theater", Focus::Theater), ("bgm", Focus::Bgm), ("exit", Focus::Exit)] {
            self.button(painter, v, art, b["labels"][key].as_str().unwrap_or_default(), bx, num(&m["buttons_y"][key]) as f32, self.focus == focus);
        }
        if list {
            let n = &m["name"];
            let name = tracks.get(self.track).and_then(|t| t["name"].as_str()).unwrap_or_default();
            let x = (crate::art::SCREEN_W - art.text_w(1, name)) / 2.0 + num(&n["dx"]) as f32;
            pic("theater/cd.png", x + num(&m["cd"][0]) as f32, num(&m["cd"][1]) as f32);
            art.text(painter, v, 1, num(&n["colour"]) as u32, x, num(&n["y"]) as f32, name, Color32::WHITE);
            let kd = &m["kind"];
            let kind = kd[if arcade { "arcade" } else { "remixed" }].as_str().unwrap_or_default();
            art.text(painter, v, num(&kd["font"]) as usize, num(&kd["colour"]) as u32, num(&kd["x"]) as f32, num(&kd["y"]) as f32, kind, Color32::WHITE);
        } else {
            let h = &s["help"]["music"];
            let key = match self.focus {
                Focus::Theater => "theater",
                Focus::Bgm => "bgm",
                _ => "exit",
            };
            self.help(painter, v, art, h[key].as_str().unwrap_or_default());
        }
    }
}

/// Text with `%c` colour switches (the game's printf): `colours[0]` first, the next at each `%c`.
#[allow(clippy::too_many_arguments)]
fn printf(painter: &egui::Painter, v: &View, art: &Art, font: usize, colours: &[u32], x: f32, y: f32, text: &str) {
    let advance = art.fonts.get(font).map_or(9.0, |f| f.advance);
    let mut cx = x;
    for (k, part) in text.split("%c").enumerate() {
        let c = colours.get(k).or(colours.last()).copied().unwrap_or(6);
        art.text(painter, v, font, c, cx, y, part, Color32::WHITE);
        cx += advance * part.len() as f32;
    }
}

/// A box outline of TILEs (0x8010063C): bars `side` wide left and right, `top` tall above and below.
fn outline(painter: &egui::Painter, v: &View, r: [f32; 4], side: f32, top: f32, c: Color32) {
    let [x, y, w, h] = r;
    for [a, b, cw, ch] in [[x, y, w, top], [x, y, side, h], [x, y + h - top, w, top], [x + w - side, y, side, h]] {
        painter.rect_filled(v.rect(a, b, cw, ch), 0.0, c);
    }
}

// ---------------------------------------------------------------- the staff roll (0x80102EEC)

/// What a staff roll frame wants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RollEvent {
    None,
    /// The sound at its start (SePlay code, record 301's bank).
    Sfx(u16),
    /// The staff roll music (BGM id).
    Music(usize),
    /// Over (or skipped): the records screen.
    Done,
}

pub struct StaffRoll {
    data: Arc<TheaterData>,
    sub: u32,
    t: u32,
    /// The list: the top line's index and its y; the frame parity (1 and 2 px a frame).
    index: usize,
    y_top: i32,
    parity: bool,
    /// The end: phase (1 hold, 2 and 3 fade) and its counter.
    end: (u32, u32),
    skip: bool,
    /// Ball and the character of the kind-5 lines are unlocked (their lines show).
    pub ball: bool,
    pub unlocked_character: bool,
}

impl StaffRoll {
    pub fn new(data: Arc<TheaterData>) -> Self {
        Self { data, sub: 0, t: 0, index: 0, y_top: 480, parity: false, end: (0, 0), skip: false, ball: true, unlocked_character: true }
    }

    fn timing(&self, k: &str) -> u32 {
        self.data.raw["staff_roll"]["timing"][k].as_u64().unwrap_or(0) as u32
    }

    fn lines(&self) -> &[Value] {
        self.data.raw["staff_roll"]["lines"].as_array().map_or(&[], Vec::as_slice)
    }

    /// START or a face button ends it on the next frame.
    pub fn key(&mut self, key: Key) {
        if matches!(key, Key::Start | Key::Button(_) | Key::Back) {
            self.skip = true;
        }
    }

    pub fn tick(&mut self) -> RollEvent {
        if self.skip {
            return RollEvent::Done;
        }
        self.t += 1;
        match self.sub {
            0 if self.t >= self.timing("sfx_at") => {
                self.sub = 1;
                self.t = 0;
                return RollEvent::Sfx(self.timing("sfx") as u16);
            }
            1 if self.t >= self.timing("music_at") => {
                self.sub = 3;
                self.t = 0;
                return RollEvent::Music(self.timing("music") as usize);
            }
            3 if self.t >= self.timing("gap") => {
                self.sub = 4;
                self.t = 0;
            }
            4 if self.t >= self.timing("title_frames") - 1 => self.sub = 5,
            5 => self.scroll(),
            6 if self.t >= self.timing("end") => {
                self.sub = 7;
                self.t = 0;
            }
            7 if self.t >= self.timing("logo_frames") => return RollEvent::Done,
            _ => {}
        }
        RollEvent::None
    }

    /// The list moves up 1.5 px a frame (1, 2, 1, 2 ...), 18 px a line, until its end is on screen; then the
    /// hold and the fade (the visible headings speed the hold up).
    fn scroll(&mut self) {
        let pitch = self.timing("pitch") as i32;
        let n = self.lines().len();
        // the list's end (its NULL entry) not on screen yet
        let more = self.y_top + pitch * (n - self.index) as i32 >= 480;
        if self.end.0 == 0 {
            if more {
                self.parity = !self.parity;
                self.y_top -= if self.parity { 2 } else { 1 };
                if self.y_top < -pitch {
                    self.index += 1;
                    self.y_top += pitch;
                }
            } else {
                self.end = (1, 0);
            }
            return;
        }
        let headings = self.visible().filter(|(l, _)| num(&l["type"]) == 0).count() as u32;
        match self.end.0 {
            1 => {
                self.end.1 += headings.max(1);
                if self.end.1 >= 620 {
                    self.end = (2, 0);
                }
            }
            2 | 3 => {
                self.end.1 += headings.max(1);
                if self.end.1 >= 80 {
                    self.end = (self.end.0 + 1, 0);
                }
            }
            _ => self.sub = 6,
        }
    }

    /// The lines on screen with their row y.
    fn visible(&self) -> impl Iterator<Item = (&Value, i32)> {
        let pitch = self.timing("pitch") as i32;
        let y_top = self.y_top;
        self.lines().iter().skip(self.index).enumerate().map(move |(k, l)| (l, y_top + pitch * k as i32)).take_while(|(_, y)| *y < 480)
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art) {
        painter.rect_filled(v.rect, 0.0, Color32::BLACK);
        let sr = &self.data.raw["staff_roll"];
        let flare = &sr["flare"];
        let base = [num(&flare[0]), num(&flare[1]), num(&flare[2])];
        match self.sub {
            4 => {
                let t = self.t as i32;
                if t >= 210 {
                    return;
                }
                let b = if t < 30 {
                    80
                } else if t <= 180 {
                    90
                } else {
                    (210 - t) * 3 + 1
                };
                let f = (t < 37).then(|| [base[0] - (7 * t).min(base[0]), base[1] - (6 * t).min(base[1]), base[2] - (5 * t).min(base[2])]);
                let title = &sr["title"];
                let ty = |k: usize| num(&sr["title_y"][k]) as f32;
                self.line(painter, v, art, false, title[0].as_str().unwrap_or_default(), ty(0), b, f);
                self.line(painter, v, art, true, title[1].as_str().unwrap_or_default(), ty(1), b, f);
            }
            5 => {
                for (l, y) in self.visible() {
                    let kind = num(&l["type"]);
                    let shown = match kind {
                        0 | 1 => true,
                        // BALL PROGRAM in place of PROGRAMMING SUPPORT once Ball is unlocked (0x80103510)
                        3 => self.ball,
                        4 => !self.ball,
                        5 => self.unlocked_character,
                        _ => false,
                    };
                    if !shown {
                        continue;
                    }
                    let large = matches!(kind, 1 | 5);
                    let dy = match kind {
                        3 => 20,
                        0 | 4 => 4,
                        _ => 0,
                    };
                    let yl = y + dy;
                    let (b, f) = self.brightness(yl, if large { 0 } else { 4 }, base);
                    self.line(painter, v, art, large, l["text"].as_str().unwrap_or_default(), yl as f32, b, f);
                }
            }
            7 => {
                let t = self.t as i32;
                let lg = &sr["logo"];
                // (0x801040A8: up by `in` a frame for in_frames, held to frame `hold`, then down by `out`)
                // (no in_frames: up to full brightness, 128)
                let step = num(&lg["in"]);
                let up = lg["in_frames"].as_i64().map_or((128 + step - 1) / step.max(1), |v| v as i32);
                let hold = num(&lg["hold"]);
                let level = if t < up {
                    step * t
                } else if t <= hold {
                    step * up
                } else {
                    step * up - num(&lg["out"]) * (t - hold)
                }
                .max(0);
                if let Some((tex, size)) = art.pic(lg["picture"].as_str().unwrap_or_default()) {
                    let tint = Color32::from_gray((level * 2).min(255) as u8);
                    let r = v.rect(num(&lg["x"]) as f32, num(&lg["y"]) as f32, size[0] as f32, size[1] as f32);
                    painter.image(tex.id(), r, egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
                }
            }
            _ => {}
        }
    }

    /// A line's brightness and flare by where it is (heading thresholds 4 px above a name's): entering and
    /// leaving at 1 with the full flare, brightening in the bottom band, 80 in the middle, fading in the top band.
    fn brightness(&self, yl: i32, off: i32, base: [i32; 3]) -> (i32, Option<[i32; 3]>) {
        let (bottom, top) = (440 - off, 20 - off);
        let mut b = if yl >= bottom || yl < top {
            (1, Some(base))
        } else if yl >= bottom - 80 {
            let d = bottom - yl;
            let b = if d < 45 { 90 } else { (90 - (2 * d + 1 - 90) / 2).max(80) };
            (b, Some([base[0] - (7 * d).min(base[0]), base[1] - (6 * d).min(base[1]), base[2] - (5 * d).min(base[2])]))
        } else if yl < top + 80 {
            let d = top + 80 - yl;
            ((yl - (top - 1)) & 0xFF, Some([(7 * d / 2).min(base[0]), (3 * d).min(base[1]), (5 * d / 2).min(base[2])]))
        } else {
            (80, None)
        };
        // the end: held at 90, then dimming with the flare growing, then gone with it fading
        match self.end {
            (1, _) => b = (90, None),
            (2, n) => {
                let n = n as i32;
                b = (80 - n.min(79), Some([(7 * n / 2).min(base[0]), (3 * n).min(base[1]), (5 * n / 2).min(base[2])]));
            }
            (3, n) => {
                let n = 80 - (n as i32).min(80);
                b = (1, Some([(7 * n / 2).min(base[0]), (3 * n).min(base[1]), (5 * n / 2).min(base[2])]));
            }
            _ => {}
        }
        b
    }

    /// A line centred on x 184 in the small or large credits font at `brightness` / 128, and its gold flare.
    #[allow(clippy::too_many_arguments)]
    fn line(&self, painter: &egui::Painter, v: &View, art: &Art, large: bool, text: &str, y: f32, brightness: i32, flare: Option<[i32; 3]>) {
        let sr = &self.data.raw["staff_roll"];
        let f = &sr["fonts"][if large { "large" } else { "small" }];
        let Some((tex, size)) = art.pic(f["picture"].as_str().unwrap_or_default()) else { return };
        let (cw, ch, per_row) = (num(&f["cell"][0]) as f32, num(&f["cell"][1]) as f32, num(&f["per_row"]).clamp(1, 64) as usize);
        let advance: Vec<i32> = f["advance"].as_array().into_iter().flatten().map(num).collect();
        // ('@' is the copyright sign's glyph)
        let glyph = |c: char| -> Option<usize> {
            let c = c as u32;
            if c <= 0x20 {
                return None;
            }
            Some(if c < 0x31 { c - 0x21 } else { c - 0x22 } as usize)
        };
        let space = num(&f["space"]);
        let width: i32 = text.chars().map(|c| glyph(c).map_or(space, |g| advance.get(g).copied().unwrap_or(space))).sum();
        let x0 = num(&sr["centre"]) as f32 - width as f32 / 2.0;
        if let Some(c) = flare {
            let gold = |k: f32| additive(Color32::from_rgb((c[0] as f32 * k) as u8, (c[1] as f32 * k) as u8, (c[2] as f32 * k) as u8));
            let (xl, xr, ym) = (x0, x0 + width as f32, y + ch / 2.0 - 2.0);
            let mid = (xl + xr) / 2.0;
            for (a, b, ka, kb) in [(xl - 16.0, xl, 0.0, 0.5), (xl, mid, 0.5, 1.0), (mid, xr, 1.0, 0.5), (xr, xr + 40.0, 0.5, 0.0)] {
                Art::shade(painter, v, [a, ym, b - a, 4.0], [gold(ka), gold(kb), gold(ka), gold(kb)]);
            }
        }
        let tint = Color32::from_gray((brightness.clamp(0, 127) * 2) as u8);
        let mut x = x0;
        for c in text.chars() {
            let Some(g) = glyph(c) else {
                x += space as f32;
                continue;
            };
            let (u, vv) = ((g % per_row) as f32 * cw, (g / per_row) as f32 * ch);
            let uv = egui::Rect::from_min_max(Pos2::new(u / size[0] as f32, vv / size[1] as f32), Pos2::new((u + cw) / size[0] as f32, (vv + ch) / size[1] as f32));
            painter.image(tex.id(), v.rect(x, y, cw, ch), uv, tint);
            x += advance.get(g).copied().unwrap_or(space) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Option<TheaterData> {
        TheaterData::load(&crate::test_assets().join("gamedata"))
    }

    #[test]
    fn locks_and_endings() {
        let Some(d) = data() else { return };
        let e2 = d.entries[2].as_ref().unwrap();
        assert_eq!(d.status(e2, 0, 0), None);
        assert_eq!(d.status(e2, 1 << 9, 0), Some(12));
        // a character with an alternate, then its alternate's cut once the alternate ending was seen (both names)
        let e12 = d.entries[12].as_ref().unwrap();
        assert_eq!(d.status(e12, 1 << 11, 0), Some(14));
        assert_eq!(d.status(e12, 1 << 11, 1 << 11), Some(23));
        let both = d.name(e12, 1 << 11, 1 << 11);
        assert!(both.contains(", ") && both != d.name(e12, 1 << 11, 0), "{both}");
        // the long cut's character: the second clear sets the alternate bit and plays the long cut
        let (mut seen, mut alt) = (0, 0);
        d.clear(16, 0, &mut seen, &mut alt);
        assert_eq!((seen, alt), (1 << 16, 0));
        assert_eq!(d.ending(16, 0, alt), Some(19));
        d.clear(16, 0, &mut seen, &mut alt);
        assert_eq!(d.ending(16, 0, alt), Some(24));
        // an alternate costume's own character, the unlockable ones
        assert_eq!(d.ending(8, 2, 0), Some(22));
        assert_eq!(d.ending(19, 0, 0), Some(7));
        assert_eq!(d.ending(20, 0, 0), Some(17));
        let p = d.params(20).unwrap();
        assert!(p.looped && p.frames == 1002 && p.file == 18);
        assert_eq!(d.params(14).unwrap().captions.len(), 1);
    }

    /// The extra fighters end with their own movies, by costume: two costumes' differ for some (one
    /// character each, two costumes); their clears set no ending bit.
    #[test]
    fn extra_fighters_have_their_own_endings() {
        let Some(d) = data() else { return };
        let ending = |c: usize, k: usize| d.ending(c, k, 0);
        for c in [33, 36] {
            if d.raw["ending"]["extra"][c.to_string()].is_null() {
                return;
            }
            let (a, b) = (ending(c, 0).unwrap(), ending(c, 1).unwrap());
            assert_ne!(a, b, "character {c}: two endings");
            for m in [a, b] {
                assert!(d.params(m).is_some(), "movie parameters {m}");
            }
        }
        let (mut seen, mut alt) = (0, 0);
        d.clear(33, 1, &mut seen, &mut alt);
        assert_eq!((seen, alt), (0, 0));
        // their Theater cells: locked until cleared, two costumes of one each their own
        let (b0, b1) = (d.extra_bit(33, 0).unwrap(), d.extra_bit(33, 1).unwrap());
        assert_ne!(b0, b1);
        let cell = |b: u32| d.entries.iter().flatten().find(|e| e.mask == 1 << (32 + b)).expect("a cell");
        assert_eq!(d.status(cell(b1), 0, 0), None);
        assert_eq!(d.status(cell(b1), TheaterData::locks(0, 1 << b1), 0), Some(ending(33, 1).unwrap()));
        assert_eq!(d.status(cell(b0), TheaterData::locks(0, 1 << b1), 0), None);
    }

    #[test]
    fn staff_roll_runs_to_the_logo() {
        let Some(d) = data() else { return };
        let mut s = StaffRoll::new(Arc::new(d));
        let mut events = Vec::new();
        let mut n = 0;
        loop {
            let e = s.tick();
            n += 1;
            if e == RollEvent::Done {
                break;
            }
            if e != RollEvent::None {
                events.push(e);
            }
            assert!(n < 20_000);
        }
        assert_eq!(events, vec![RollEvent::Sfx(0xC7C0), RollEvent::Music(4)]);
        // 31 + 240 + 60 + 9060 + 180 frames, about 159.5 s
        assert!((9500..9650).contains(&n), "{n}");
    }
}
