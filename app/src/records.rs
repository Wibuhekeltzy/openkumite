//! The records screens (mode 16 → 17, record 11 0x800C2434): one page per visit (CHARACTER TIME RECORDS, GREATEST
//! SURVIVORS, CHARACTERS), its rows sliding in page by page over a stage the camera circles, and the name entry
//! for a new Time Attack or Survival record. Layout, strings, colours and timings from screens.json `records`.

use egui::{Color32, Pos2};
use serde_json::Value;

use crate::art::{Art, Screen as View};
use crate::menu::{Key, Roster};
use crate::save::{NewRecord, Records};

/// A page's characters always shown (screens.json `records`).
fn always(v: &Value) -> Vec<usize> {
    v.as_array().into_iter().flatten().filter_map(|c| c.as_u64()).map(|c| c as usize).collect()
}

/// What a records frame wants from the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    None,
    /// Faded out: the attract loop goes on (`request_scene(3)`).
    Done,
    /// START: the main menu (SFX 0x4CC0).
    Menu,
    /// The BGM fades out over this many frames.
    FadeMusic(u32),
}

/// The name being typed (0x800D3108).
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: [u8; 3],
    pub cursor: usize,
    pub letter: usize,
    /// The character id whose name replaces a banned or START-skipped one.
    pub character: usize,
}

pub struct RecordsScreen {
    data: Value,
    /// 0 TA, 1 survival, 2 usage.
    pub kind: usize,
    ss: u32,
    /// Draw state: 0 black, 1 banner, 2 rows, 3 name entry, 4 name entered.
    pub draw: u32,
    cnt: i32,
    scroll: i32,
    top: i32,
    timer: u32,
    hold: u32,
    grow: u32,
    grow_from: i32,
    /// Row → table index (TA / usage: character id; survival: the slot).
    rows: Vec<usize>,
    /// Usage per mille by character id, and the leader's.
    usage: Vec<u32>,
    usage_max: u32,
    new_row: Option<usize>,
    slot: i32,
    /// The new records waiting for the name.
    targets: Vec<NewRecord>,
    pub entry: Option<Entry>,
    countdown: u32,
    pub frame: u32,
    /// The camera's orbit angle (4096 a turn), stepped on drawn frames.
    pub angle: i32,
    /// The stage behind (a fight stage, no fighters).
    pub stage: usize,
    keys: Vec<Key>,
    /// The save's tables as of the last frame (what the rows show).
    tables: Records,
    /// Auto-repeat of the held direction (0x80029F58): frames held.
    held: (i32, u32),
}

fn num(v: &Value) -> i32 {
    v.as_i64().unwrap_or(0) as i32
}

impl RecordsScreen {
    /// Mode 16's setup: the page `records.page` (or the new record's), and the next background stage.
    pub fn new(data: &Value, records: &mut Records) -> Self {
        let bg = &data["background"];
        let n = num(&bg["stages"]).max(1) as usize;
        let skip: Vec<usize> = bg["skip"].as_array().into_iter().flatten().map(|v| v.as_u64().unwrap_or(0) as usize).collect();
        let mut s = records.stage_rotor as usize % n;
        // (at most once round: a list skipping every stage keeps the rotor's)
        for _ in 0..n {
            if !skip.contains(&s) {
                break;
            }
            s = (s + 1) % n;
        }
        records.stage_rotor = (s + 1) as u8;
        Self {
            data: data.clone(),
            kind: records.page as usize % 3,
            ss: 0,
            draw: 0,
            cnt: 0,
            scroll: 0,
            top: 0,
            timer: 0,
            hold: 150,
            grow: 0,
            grow_from: 0,
            rows: Vec::new(),
            usage: vec![0; records.characters()],
            usage_max: 0,
            new_row: None,
            slot: 0,
            targets: records.new.clone(),
            entry: None,
            countdown: 0,
            frame: 0,
            angle: num(&bg["start_angle"]),
            stage: s,
            keys: Vec::new(),
            tables: records.clone(),
            held: (0, 0),
        }
    }

    pub fn key(&mut self, key: Key) {
        self.keys.push(key);
    }

    fn timing(&self, k: &str) -> i32 {
        num(&self.data["timing"][k])
    }

    /// Kind 0's rows (0x800C172C): characters `always` (0-9) and any with a time, by time (stable); the new entry's
    /// row.
    fn ta_build(&mut self, records: &Records, new: Option<usize>) -> Option<usize> {
        let d = &self.data["time_attack"];
        let below = num(&d["shown_at_most"]) as u32;
        let max = (num(&d["max_character"]) as usize).min(records.characters() - 1);
        let always = always(&d["always"]);
        let mut list: Vec<(usize, u32)> = records.time_attack.iter().enumerate().take(max + 1).filter(|(c, r)| always.contains(c) || r.0 <= below).map(|(c, r)| (c, r.0)).collect();
        list.sort_by_key(|e| e.1);
        self.rows = list.iter().map(|e| e.0).collect();
        self.hold = self.timing("hold") as u32;
        new.and_then(|c| self.rows.iter().position(|&r| r == c))
    }

    /// Kind 2's rows (0x800C19B8): characters `always` (0-9) and any played, most played first, as per mille of all
    /// plays.
    fn usage_build(&mut self, records: &Records) {
        let n = |c: usize| records.usage.get(c).map_or(0, |u| u[0] as u32 + u[2] as u32 + u[3] as u32);
        let max = (num(&self.data["usage"]["max_character"]) as usize).min(records.characters() - 1);
        let total: u32 = (0..records.characters()).map(n).sum();
        let always = always(&self.data["usage"]["always"]);
        let mut list: Vec<(usize, u32)> = (0..=max).filter(|&c| always.contains(&c) || n(c) != 0).map(|c| (c, n(c))).collect();
        list.sort_by_key(|e| std::cmp::Reverse(e.1));
        for &(c, k) in &list {
            if let Some(u) = self.usage.get_mut(c) {
                *u = per_mille(k, total);
            }
        }
        self.rows = list.iter().map(|e| e.0).collect();
        self.usage_max = self.rows.first().and_then(|&c| self.usage.get(c)).copied().unwrap_or(0);
        self.hold = self.timing("hold") as u32;
    }

    /// One frame (the mode-17 state machine); `held`: left / right held on the pad.
    pub fn tick(&mut self, records: &mut Records, roster: &Roster, held: [bool; 2]) -> Event {
        let e = self.step(records, roster, held);
        self.tables = records.clone();
        e
    }

    fn step(&mut self, records: &mut Records, roster: &Roster, held: [bool; 2]) -> Event {
        self.frame += 1;
        let keys = std::mem::take(&mut self.keys);
        // START (not on the first frame): the typed name becomes the character's, then the main menu
        if self.ss >= 2 && keys.contains(&Key::Start) {
            if let Some(e) = self.entry.as_mut() {
                e.name = short_name(roster, e.character);
                e.cursor = 3;
                self.store(records);
                records.new.clear();
            }
            return Event::Menu;
        }
        let pitch = self.timing("slide").max(1);
        let rest = num(&self.data["rows"]["rest_y"]);
        let row_pitch = num(&self.data["rows"]["pitch"]);
        let target = |top: i32| rest - row_pitch * top;
        match self.ss {
            0 | 1 => {
                if self.targets.is_empty() {
                    match self.kind {
                        0 => {
                            self.ta_build(records, None);
                        }
                        1 => {
                            self.rows = (0..records.survival.len().min(10)).collect();
                            self.hold = self.timing("hold") as u32;
                        }
                        _ => self.usage_build(records),
                    }
                    self.new_row = None;
                } else {
                    let ta = self.targets.iter().find_map(|t| match t {
                        NewRecord::TimeAttack(c) => Some(*c),
                        _ => None,
                    });
                    let sv = self.targets.iter().find_map(|t| match t {
                        NewRecord::Survival(i) => Some(*i),
                        _ => None,
                    });
                    if let Some(c) = ta {
                        self.kind = 0;
                        self.new_row = self.ta_build(records, Some(c));
                    } else if let Some(i) = sv {
                        self.kind = 1;
                        self.rows = (0..records.survival.len().min(10)).collect();
                        self.hold = self.timing("hold") as u32;
                        self.new_row = Some(i);
                    }
                    let character = match (ta, sv) {
                        (Some(c), _) => c,
                        (_, Some(i)) => records.survival.get(i).map_or(0, |e| e.0),
                        _ => 0,
                    };
                    self.entry = Some(Entry { name: *b"   ", cursor: 0, letter: 0, character });
                }
                self.timer = 0;
                self.draw = 1;
                self.cnt = 0;
                self.ss = 3;
            }
            3 => {
                self.cnt += self.timing("fade");
                if self.cnt >= 256 {
                    self.ss = if self.new_row.is_some() { 10 } else { 4 };
                }
            }
            4 => {
                let rows = self.rows.len() as i32;
                let r = if rows % 5 == 0 { 5 } else { rows % 5 };
                self.grow_from = rows;
                self.top = (rows - r).max(0);
                self.scroll = target(rows);
                self.draw = 2;
                self.ss = 5;
            }
            5 => {
                self.scroll += pitch;
                if self.scroll >= target(self.top) {
                    self.scroll = target(self.top);
                    self.timer = 0;
                    self.grow = 0;
                    self.grow_from = self.top;
                    self.ss = 6;
                }
            }
            6 => {
                self.grow = self.timer.min(64);
                self.timer += 1;
                if self.timer >= self.hold {
                    if self.top > 0 {
                        self.top -= 5;
                        self.ss = 5;
                    } else {
                        self.cnt = 0;
                        self.ss = 7;
                    }
                }
            }
            7 => {
                self.cnt += 1;
                if self.cnt >= self.timing("end_wait") {
                    self.cnt = 256;
                    self.ss = 9;
                }
            }
            9 => {
                self.cnt -= self.timing("fade");
                if self.cnt <= 0 {
                    self.draw = 0;
                    self.ss = 14;
                }
            }
            10 => {
                let (n, rows) = (self.new_row.unwrap_or(0) as i32, self.rows.len() as i32);
                (self.top, self.slot) = if n < 2 {
                    (0, n)
                } else if n > rows - 3 {
                    ((rows - 5).max(0), 5 - (rows - n))
                } else {
                    (n - 2, 2)
                };
                self.draw = 2;
                self.grow = 64;
                self.grow_from = 0;
                self.scroll = target(rows);
                self.ss = 11;
            }
            11 => {
                let t = target(self.top);
                self.scroll += ((t - self.scroll + 15) >> 4).clamp(1, 16);
                if self.scroll >= t {
                    self.scroll = t;
                    self.countdown = self.timing("entry_frames") as u32;
                    self.draw = 3;
                    self.ss = 12;
                }
            }
            12 => {
                self.countdown = self.countdown.saturating_sub(1);
                let done = self.name_update(&keys, held, self.countdown == 0, records, roster);
                self.store(records);
                if done {
                    records.new.clear();
                    self.targets.clear();
                    self.draw = 4;
                    self.cnt = 0;
                    self.ss = 13;
                }
            }
            13 => {
                self.cnt += 1;
                if self.cnt >= self.timing("entry_end_wait") {
                    self.cnt = 256;
                    self.ss = 9;
                    return Event::FadeMusic(self.timing("bgm_fade") as u32);
                }
            }
            _ => {
                self.draw = 0;
                records.page = ((self.kind + 1) % 3) as u8;
                return Event::Done;
            }
        }
        if self.draw > 0 {
            self.angle += num(&self.data["background"]["turn"]);
        }
        Event::None
    }

    /// The typed name into the new entries (every frame, so the row shows it).
    fn store(&self, records: &mut Records) {
        let Some(e) = &self.entry else { return };
        let name = String::from_utf8_lossy(&e.name).into_owned();
        for t in &self.targets {
            match *t {
                NewRecord::TimeAttack(c) => {
                    if let Some(r) = records.time_attack.get_mut(c) {
                        r.1 = name.clone();
                    }
                }
                NewRecord::Survival(i) => {
                    if let Some(r) = records.survival.get_mut(i) {
                        r.2 = name.clone();
                    }
                }
            }
        }
    }

    /// The name entry's frame (0x800C2E90): left / right turn the letter (auto-repeat), a button takes it; '<'
    /// steps back, '=' ends; the third letter ends. `timeout`: the count ran out. True when the name is done.
    fn name_update(&mut self, keys: &[Key], held: [bool; 2], timeout: bool, records: &mut Records, roster: &Roster) -> bool {
        let en = &self.data["entry"];
        // (no letters: blanks)
        let letters: Vec<u8> = en["letters"].as_str().filter(|l| !l.is_empty()).unwrap_or(" ").bytes().collect();
        let banned: Vec<String> = en["banned"].as_array().into_iter().flatten().filter_map(|b| b.as_str().map(String::from)).collect();
        let unlock = en["unlock_name"].as_str().unwrap_or_default().to_string();
        let [first, every] = [num(&en["repeat"][0]).max(1) as u32, num(&en["repeat"][1]).max(1) as u32];
        let n = letters.len().max(1);
        let Some(e) = self.entry.as_mut() else { return true };
        let finish = |e: &mut Entry, banned: &[String]| {
            if banned.iter().any(|b| b.as_bytes() == e.name) {
                e.name = short_name(roster, e.character);
            }
        };
        if timeout {
            for k in e.cursor..3 {
                e.name[k] = b' ';
            }
            e.cursor = 3;
            finish(e, &banned);
            return true;
        }
        // the direction: a press steps at once, held it repeats after `first` frames, then every `every`
        let mut dir = 0i32;
        for k in keys {
            match k {
                Key::Left => dir -= 1,
                Key::Right => dir += 1,
                _ => {}
            }
        }
        let hold = i32::from(held[1]) - i32::from(held[0]);
        if dir != 0 {
            self.held = (dir, 0);
        } else if hold != 0 && hold == self.held.0 {
            self.held.1 += 1;
            if self.held.1 >= first && (self.held.1 - first).is_multiple_of(every) {
                dir = hold;
            }
        } else {
            self.held = (0, 0);
        }
        if dir != 0 {
            crate::menu::sfx::play(num(&self.data["timing"]["letter_sfx"]) as u16);
            e.letter = (e.letter as i32 + dir).rem_euclid(n as i32) as usize;
        }
        if e.cursor < 3 {
            e.name[e.cursor] = letters[e.letter % n];
        }
        if !keys.iter().any(|k| matches!(k, Key::Button(_))) {
            return false;
        }
        crate::menu::sfx::play(num(&self.data["timing"]["confirm_sfx"]) as u16);
        match letters[e.letter % n] {
            b'<' => {
                if e.cursor > 0 {
                    e.name[e.cursor] = b' ';
                    e.cursor -= 1;
                }
                false
            }
            b'=' => {
                for k in e.cursor..3 {
                    e.name[k] = b' ';
                }
                e.cursor = 3;
                finish(e, &banned);
                true
            }
            _ => {
                e.cursor += 1;
                if e.cursor < 3 {
                    return false;
                }
                if e.name == unlock.as_bytes() && !records.name_unlock {
                    records.name_unlock = true;
                }
                finish(e, &banned);
                true
            }
        }
    }

    /// The camera: the eye circling the origin (game units, y down) and the point it looks at.
    pub fn eye(&self) -> ([f32; 3], [f32; 3]) {
        let bg = &self.data["background"];
        let (r, h) = (num(&bg["radius"]) as f32, num(&bg["height"]) as f32);
        let a = (self.angle & 0xFFF) as f32 * std::f32::consts::TAU / 4096.0;
        ([a.cos() * r, -h, a.sin() * r], [0.0, -h, 0.0])
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art, roster: &Roster) {
        let records = &self.tables;
        if self.draw == 0 {
            painter.rect_filled(v.rect, 0.0, Color32::BLACK);
            return;
        }
        let rows = &self.data["rows"];
        // the name entry's band behind the new row (half dark, a little blue)
        let en = &self.data["entry"];
        if self.draw == 3 {
            let c = &en["band"]["colour"];
            let y = num(&en["band"]["y"]) + num(&rows["pitch"]) * self.slot;
            Art::shade(
                painter,
                v,
                [0.0, y as f32, crate::art::SCREEN_W, num(&en["band"]["h"]) as f32],
                [Color32::from_rgba_unmultiplied(num(&c[0]) as u8 * 2, num(&c[1]) as u8 * 2, num(&c[2]) as u8 * 2, 128); 4],
            );
        }
        self.banner(painter, v, art);
        if self.draw >= 3 {
            let cd = &en["countdown"];
            let value = self.countdown.div_ceil(60);
            art.text(painter, v, num(&cd["font"]) as usize, num(&cd["colour"]) as u32, num(&cd["x"]) as f32, num(&cd["y"]) as f32, &format!("{value:2}"), Color32::WHITE);
        }
        if self.draw >= 2 {
            let clip = &rows[if self.draw >= 3 { "clip_entry" } else { "clip_attract" }];
            let clipped = painter.with_clip_rect(v.rect(num(&clip[0]) as f32, num(&clip[1]) as f32, num(&clip[2]) as f32, num(&clip[3]) as f32).intersect(v.rect));
            for (i, &t) in self.rows.iter().enumerate() {
                let y = self.scroll + num(&rows["pitch"]) * i as i32;
                if !(0..552).contains(&y) {
                    continue;
                }
                let flicker = self.draw >= 3 && self.new_row == Some(i) && self.frame & 3 != 0;
                self.row(&clipped, v, art, roster, records, i, t, y as f32, usize::from(flicker));
            }
        }
        // the cursor under the letter being typed
        if let (3, Some(e)) = (self.draw, &self.entry) {
            if e.cursor < 3 {
                let c = &en["cursor"];
                let x = num(&c["x"]) + num(&c["dx"]) * e.cursor as i32;
                let y = num(&c["y"]) + num(&rows["pitch"]) * self.slot;
                let col = Color32::from_rgb(num(&c["colour"][0]) as u8, num(&c["colour"][1]) as u8, num(&c["colour"][2]) as u8);
                painter.rect_filled(v.rect(x as f32, y as f32, num(&c["w"]) as f32, num(&c["h"]) as f32), 0.0, col);
            }
        }
        // the fades (subtractive black over everything)
        let fade = match self.ss {
            3 => 255 - self.cnt.min(255),
            9 => 255 - self.cnt.clamp(0, 255),
            _ => 0,
        };
        if fade > 0 {
            painter.rect_filled(v.rect, 0.0, Color32::from_black_alpha(fade as u8));
        }
    }

    /// The page's banner (0x800C1B18): two opaque strips, then the tail fading into the background.
    fn banner(&self, painter: &egui::Painter, v: &View, art: &Art) {
        let b = &self.data["banner"];
        let Some(file) = b["pictures"][self.kind].as_str() else { return };
        let Some((tex, size)) = art.pic(file) else { return };
        let (y, h) = (num(&b["y"]) as f32, num(&b["h"]) as f32);
        let piece = |x: f32, w: f32, u: f32, vv: f32, shade: [f32; 2]| {
            let (sw, sh) = (size[0] as f32, size[1] as f32);
            let mut mesh = egui::Mesh::with_texture(tex.id());
            let a = |s: f32| Color32::from_white_alpha((s * 255.0).clamp(0.0, 255.0) as u8);
            let corners = [(x, y, u, vv, shade[0]), (x + w, y, u + w, vv, shade[1]), (x, y + h, u, vv + h, shade[0]), (x + w, y + h, u + w, vv + h, shade[1])];
            for (cx, cy, cu, cv, s) in corners {
                mesh.vertices.push(egui::epaint::Vertex { pos: v.pos(cx, cy), uv: Pos2::new(cu / sw, cv / sh), color: a(s) });
            }
            mesh.indices.extend([0, 1, 2, 1, 3, 2]);
            painter.add(egui::Shape::mesh(mesh));
        };
        for o in b["opaque"].as_array().into_iter().flatten() {
            piece(num(&o[0]) as f32, num(&o[1]) as f32, num(&o[2]) as f32, num(&o[3]) as f32, [1.0, 1.0]);
        }
        // back - white * shade + picture * shade: the picture blended in by shade / 0x80
        for p in b["blend"].as_array().into_iter().flatten() {
            let s = |k: usize| num(&p["shade"][k]) as f32 / 128.0;
            piece(num(&p["x"]) as f32, num(&p["w"]) as f32, num(&p["u"]) as f32, num(&p["v"]) as f32, [s(0), s(1)]);
        }
    }

    /// Row `i` (table entry `t`) with its baseline at `y`, colour set `set`.
    #[allow(clippy::too_many_arguments)]
    fn row(&self, painter: &egui::Painter, v: &View, art: &Art, roster: &Roster, records: &Records, i: usize, t: usize, y: f32, set: usize) {
        let rows = &self.data["rows"];
        let page = &self.data[["time_attack", "survival", "usage"][self.kind]];
        let sets = page["colours"].as_array().cloned().unwrap_or_default();
        let colours = sets.get(set).or(sets.first()).cloned().unwrap_or_default();
        let c = |k: usize| num(&colours[k]) as u32;
        let at = |e: &Value| (num(&e["x"]) as f32, y + num(&e["dy"]) as f32);
        // rank: 1ST, 2ND, 3RD, 4TH.. (11-20 TH)
        let rank = &rows["rank"];
        let suffix = {
            let k = i % 10;
            let k = if k >= 3 || (10..=19).contains(&i) { 3 } else { k };
            rank["suffixes"][k].as_str().unwrap_or("TH")
        };
        let (rx, ry) = at(rank);
        art.text(painter, v, num(&rank["font"]) as usize, c(0), rx, ry, &format!("{}{suffix}", (i + 1).min(99)), Color32::WHITE);
        let character = match self.kind {
            1 => records.survival.get(t).map_or(0, |e| e.0),
            _ => t,
        };
        let (fx, fy) = at(&rows["face"]);
        crate::modes::face(painter, v, art, roster, character, fx, fy, 0);
        let name = roster.by_id(character).map_or_else(String::new, |c| c.name.to_uppercase());
        let (nx, ny) = at(&rows["name"]);
        art.text(painter, v, num(&rows["name"]["font"]) as usize, c(1), nx, ny, &name, Color32::WHITE);
        let grey = &rows["underline_colour"];
        let grey = Color32::from_rgb(num(&grey[0]) as u8, num(&grey[1]) as u8, num(&grey[2]) as u8);
        for u in rows["underlines"].as_array().into_iter().flatten() {
            painter.rect_filled(v.rect(num(&u[0]) as f32, y + num(&u[1]) as f32, num(&u[2]) as f32, num(&u[3]) as f32), 0.0, grey);
        }
        let player = &rows["player"];
        let (px, py) = at(player);
        let pf = num(&player["font"]) as usize;
        match self.kind {
            0 => {
                let Some((f, who)) = records.time_attack.get(t) else { return };
                let f = (*f).min(359_999);
                let (mm, ss, cc) = (f / 3600, (f / 60) % 60, (f % 60) * 100 / 60);
                let m = &page["marks"];
                let my = y + num(&m["dy"]) as f32;
                art.text(painter, v, num(&m["font"]) as usize, c(2), num(&m["x"][0]) as f32, my, "'", Color32::WHITE);
                art.text(painter, v, num(&m["font"]) as usize, c(2), num(&m["x"][1]) as f32, my, "\"", Color32::WHITE);
                let d = &page["digits"];
                let dy = y + num(&d["dy"]) as f32;
                for (k, val) in [mm, ss, cc].into_iter().enumerate() {
                    art.text(painter, v, num(&d["font"]) as usize, c(2), num(&d["x"][k]) as f32, dy, &format!("{val:02}"), Color32::WHITE);
                }
                art.text(painter, v, pf, c(3), px, py, who, Color32::WHITE);
            }
            1 => {
                let Some((_, wins, who)) = records.survival.get(t) else { return };
                let wins = (*wins).min(num(&page["max_wins"]) as u32);
                let e = &page[if wins < 2 { "one" } else { "many" }];
                let wy = y + num(&page["dy"]) as f32;
                art.text(painter, v, 1, c(2), num(&e["x"]) as f32, wy, &format!("{wins:3}"), Color32::WHITE);
                art.text(painter, v, 1, c(2), num(&e["word_x"]) as f32, wy, e["word"].as_str().unwrap_or_default(), Color32::WHITE);
                art.text(painter, v, pf, c(3), px, py, who, Color32::WHITE);
            }
            _ => {
                let g = if (i as i32) >= self.grow_from { self.grow } else { 0 };
                let pm = self.usage.get(t).copied().unwrap_or(0);
                let (shown, bar) = match (pm * g * 1000).checked_div(self.usage_max) {
                    Some(b) => (((pm * g) >> 6).min(1000), (b >> 6).min(1000)),
                    None => (0, 0),
                };
                self.bar(painter, v, t, bar, y);
                let p = &page["percent"];
                let py = y + num(&p["whole"]["dy"]) as f32;
                let small = y + num(&p["dot"]["dy"]) as f32;
                let (f1, f0) = (num(&p["whole"]["font"]) as usize, num(&p["dot"]["font"]) as usize);
                if shown == 1000 || shown == 0 {
                    let text = if shown == 1000 { "100" } else { "  0" };
                    art.text(painter, v, f1, c(2), num(&p["whole"]["x_full"]) as f32, py, text, Color32::WHITE);
                    art.text(painter, v, f0, c(2), num(&p["sign"]["x_full"]) as f32, small, "%", Color32::WHITE);
                } else {
                    art.text(painter, v, f1, c(2), num(&p["whole"]["x"]) as f32, py, &format!("{:02}", shown / 10), Color32::WHITE);
                    art.text(painter, v, f0, c(2), num(&p["dot"]["x"]) as f32, small, ".", Color32::WHITE);
                    art.text(painter, v, f0, c(2), num(&p["tenth"]["x"]) as f32, small, &format!("{:x}", shown % 10), Color32::WHITE);
                    art.text(painter, v, f0, c(2), num(&p["sign"]["x"]) as f32, small, "%", Color32::WHITE);
                }
            }
        }
    }

    /// The usage bar (0x800C0B34): a box `bar` per mille of 129 px long, its right end at x 273, front, side, top.
    fn bar(&self, painter: &egui::Painter, v: &View, character: usize, bar: u32, y: f32) {
        let b = &self.data["usage"]["bar"];
        let w = (bar * num(&b["max_w"]) as u32 / 1000) as f32;
        if w <= 0.0 {
            return;
        }
        let last = b["colours"].as_array().map_or(0, |c| c.len().saturating_sub(1));
        let cols = &b["colours"][character.min(last)];
        let col = |k: usize| Color32::from_rgb(num(&cols[k][0]) as u8, num(&cols[k][1]) as u8, num(&cols[k][2]) as u8);
        let (right, top, h) = (num(&b["right"]) as f32, y + num(&b["dy"]) as f32, num(&b["h"]) as f32);
        let (sx, sy) = (num(&b["side"][0]) as f32, num(&b["side"][1]) as f32);
        let tx = b["top_right_dx"].as_f64().map_or(sx, |v| v as f32); // (before top_right_dx: the side's)
        let left = right - w;
        let quad = |p: [(f32, f32); 4], c: [Color32; 4]| {
            let mut mesh = egui::Mesh::default();
            for (k, (x, y)) in p.into_iter().enumerate() {
                mesh.colored_vertex(v.pos(x, y), c[k]);
            }
            mesh.indices.extend([0, 1, 2, 1, 3, 2]);
            painter.add(egui::Shape::mesh(mesh));
        };
        quad([(left + sx, top + sy), (right + tx, top + sy), (left, top), (right, top)], [col(3), col(4), col(3), col(4)]);
        quad([(left + sx, top + sy), (left, top), (left + sx, top + h + sy), (left, top + h)], [col(0); 4]);
        quad([(left, top), (right, top), (left, top + h), (right, top + h)], [col(1), col(2), col(1), col(2)]);
    }
}

/// n * 1000 / total (0x8004CD20): 1000 when n > total, 0 when nothing was played.
fn per_mille(n: u32, total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    if n > total {
        return 1000;
    }
    (n as u64 * 1000 / total as u64) as u32
}

/// The first three letters of the character's name (the replacement name).
fn short_name(roster: &Roster, character: usize) -> [u8; 3] {
    let name = roster.by_id(character).map_or_else(|| "???".to_string(), |c| c.name.to_uppercase());
    let mut out = *b"   ";
    for (k, b) in name.bytes().take(3).enumerate() {
        out[k] = b;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Option<Roster> {
        let gd = crate::test_assets().join("gamedata");
        kumite_fs::exists(&gd.join("screens.json")).then(|| Roster::load(&gd))
    }

    fn defaults(r: &Roster) -> Records {
        let m = &r.modes;
        let mut rec = Records { time_attack: m.ta_defaults.clone(), survival: m.survival_defaults.clone(), ..Default::default() };
        rec.new.clear();
        rec
    }

    /// The attract path of a 10-row page: fade in 32, slide 30, hold 150, slide 30, hold 150, wait 60, fade out
    /// 32 (one frame per state change aside), then the next page kind.
    #[test]
    fn attract_page_timeline() {
        let Some(r) = roster() else { return };
        let mut rec = defaults(&r);
        rec.page = 1;
        let mut s = RecordsScreen::new(&r.screens["records"], &mut rec);
        let mut n = 0;
        while s.tick(&mut rec, &r, [false; 2]) != Event::Done {
            n += 1;
            assert!(n < 2000);
        }
        assert!((480..=500).contains(&n), "{n} frames");
        assert_eq!(rec.page, 2);
        assert!(rec.stage_rotor >= 1);
    }

    /// Typing: a button takes the letter; '<' steps back; a banned name becomes the character's.
    #[test]
    fn name_entry() {
        let Some(r) = roster() else { return };
        let mut rec = defaults(&r);
        assert!(rec.time_attack(7, 100));
        let mut s = RecordsScreen::new(&r.screens["records"], &mut rec);
        let step = |s: &mut RecordsScreen, rec: &mut Records, keys: &[Key]| {
            for k in keys {
                s.key(*k);
            }
            s.tick(rec, &r, [false; 2])
        };
        for _ in 0..200 {
            step(&mut s, &mut rec, &[]);
        }
        assert_eq!(s.draw, 3, "name entry");
        // from A, left to ' ', '.', '=', '<'; '<' at the first slot does nothing
        step(&mut s, &mut rec, &[Key::Left]); // ' '
        step(&mut s, &mut rec, &[Key::Left]); // '.'
        step(&mut s, &mut rec, &[Key::Left]); // '='
        step(&mut s, &mut rec, &[Key::Left]); // '<'
        step(&mut s, &mut rec, &[Key::Right]); // '='
        step(&mut s, &mut rec, &[Key::Left]); // '<'
        step(&mut s, &mut rec, &[Key::Button(0)]); // at the first letter: nothing
        assert_eq!(s.entry.as_ref().unwrap().cursor, 0);
        for _ in 0..4 {
            step(&mut s, &mut rec, &[Key::Right]); // back to A
        }
        step(&mut s, &mut rec, &[Key::Button(0)]);
        step(&mut s, &mut rec, &[Key::Button(0)]);
        step(&mut s, &mut rec, &[Key::Button(0)]);
        assert_eq!(rec.time_attack[7].1, "XIA", "AAA is banned");
        assert_eq!(s.draw, 4);
        assert!(rec.new.is_empty());
    }

    #[test]
    fn usage_per_mille() {
        assert_eq!(per_mille(1, 4), 250);
        assert_eq!(per_mille(5, 4), 1000);
        assert_eq!(per_mille(3, 0), 0);
    }
}
