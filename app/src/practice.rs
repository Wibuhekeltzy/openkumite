//! Practice mode like the original (record 6): the MODE SELECT menu that opens on the first fighting frame and
//! with START, the 1P FREESTYLE / VS CPU / COMBO TRAINING sub-menus, the training dummy, and the attack data
//! over the fight (TOTAL DMG, DMG:nnn(nnn%), COUNTER, the combo box, HIGH / MID / LOW markers at the hit).
//! Layout, strings and colours come from screens.json `hud.practice`.

use crate::art::{int as u, num as n, pair as xy, Art, Screen as View, Tex};
use crate::menu::{sfx, Key};
use egui::Color32;
use kumite_sim::input::Pad;
use kumite_sim::HitEvent;
use serde_json::Value;

fn rect(v: &Value) -> [f32; 4] {
    [n(&v[0]), n(&v[1]), n(&v[2]), n(&v[3])]
}
fn strings(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect()
}

/// One menu line.
#[derive(Clone, Debug, Default)]
struct Item {
    text: String,
    y: f32,
    values: Vec<String>,
    /// TRAINING DUMMY: the value goes on the next line.
    value_y: Option<f32>,
    ok_cancel: bool,
}

#[derive(Clone, Debug, Default)]
struct Menu {
    title: String,
    items: Vec<Item>,
}

/// A text with its font, colour and place.
#[derive(Clone, Debug, Default)]
struct Text {
    text: String,
    font: usize,
    colour: u32,
    x: f32,
    y: f32,
}

impl Text {
    fn from(v: &Value) -> Self {
        Self { text: v["text"].as_str().unwrap_or_default().to_string(), font: u(&v["font"]) as usize, colour: u(&v["colour"]), x: n(&v["x"]), y: n(&v["y"]) }
    }
}

/// The practice tables (screens.json `hud.practice`).
#[derive(Clone, Debug, Default)]
pub struct PracticeData {
    border_rgb: [u8; 3],
    panel_rgb: [u8; 3],
    boxes: [([f32; 4], [f32; 4]); 2],
    title_font: usize,
    title_colour: u32,
    title_y: f32,
    item_colour: u32,
    selected_colour: u32,
    value_right: f32,
    menu_x: [f32; 2],
    mode_select: Menu,
    menus: Vec<Menu>,
    quick_roll: [String; 2],
    /// The letters the menu's printer draws as arrows: the letter, its picture (uv), size and page.
    arrows: Vec<(char, [u32; 2])>,
    arrow_size: [u32; 2],
    arrow_page: [u32; 2],
    startup: u32,
    total: Text,
    total_x: [f32; 2],
    last_hit: Text,
    last_colours: [u32; 3],
    counter: Text,
    clean: Text,
    combo_frames: u32,
    hits: Text,
    combo_label: Text,
    combo_label_x: [f32; 2],
    damage: Text,
    damage_label: Text,
    damage_dx: [f32; 3],
    negative_dx: f32,
    marker_frames: u32,
    marker_ring: usize,
    marker_page: [u32; 2],
    marker_kinds: Vec<(u32, usize)>,
    /// The kind of the masks not in `marker_kinds`, and the masks that draw nothing (0x800B6554).
    marker_default: usize,
    no_marker: Vec<u32>,
    labels: Vec<(u32, u32, [u32; 2], u32, u32)>,
    burst: (u32, u32, [u32; 2], [u32; 2]),
    burst_size: [u32; 2],
    intro: Text,
    /// The dummy of this move set takes no actions, nor holds up to get up (0x800B81A8).
    pub inactive_moveset: Option<i64>,
}

impl PracticeData {
    pub fn load(screens: &Value) -> Self {
        let p = &screens["hud"]["practice"];
        let menu = |m: &Value| Menu {
            title: m["title"].as_str().unwrap_or_default().to_string(),
            items: m["items"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|i| Item {
                    text: i["text"].as_str().unwrap_or_default().to_string(),
                    y: n(&i["y"]),
                    values: strings(&i["values"]),
                    value_y: i["value_y"].as_f64().map(|v| v as f32),
                    ok_cancel: i["ok_cancel"].as_bool().unwrap_or(false),
                })
                .collect(),
        };
        let rgb = |v: &Value| [u(&v[0]) as u8, u(&v[1]) as u8, u(&v[2]) as u8];
        let h = &p["hud"];
        let c = &h["combo"];
        let m = &h["markers"];
        let roll = &p["menus"][0]["items"][2]["value_names"];
        Self {
            border_rgb: rgb(&p["box"]["border_rgb"]),
            panel_rgb: rgb(&p["box"]["panel_rgb"]),
            boxes: [(rect(&p["box"]["mode_select"]["border"]), rect(&p["box"]["mode_select"]["panel"])), (rect(&p["box"]["sub"]["border"]), rect(&p["box"]["sub"]["panel"]))],
            title_font: u(&p["text"]["title_font"]) as usize,
            title_colour: u(&p["text"]["title_colour"]),
            title_y: n(&p["text"]["title_y"]),
            item_colour: u(&p["text"]["item_colour"]),
            selected_colour: u(&p["text"]["selected_colour"]),
            value_right: n(&p["text"]["value_right"]),
            menu_x: [n(&p["text"]["x"]["mode_select"]), n(&p["text"]["x"]["sub"])],
            mode_select: menu(&p["mode_select"]),
            menus: p["menus"].as_array().into_iter().flatten().map(menu).collect(),
            quick_roll: ["6", "7"].map(|k| roll[k].as_str().unwrap_or_default().to_string()),
            arrows: {
                let a = &p["text"]["arrows"];
                (0..2).filter_map(|k| Some((a["chars"][k].as_str()?.chars().next()?, xy(&a["uv"][k])))).collect()
            },
            arrow_size: [u(&p["text"]["arrows"]["w"]), u(&p["text"]["arrows"]["h"])],
            arrow_page: xy(&p["text"]["arrows"]["page"]),
            startup: h["startup_frames"].as_u64().unwrap_or(0) as u32,
            total: Text::from(&h["total"]),
            total_x: [n(&h["total"]["x"][0]), n(&h["total"]["x"][1])],
            last_hit: Text::from(&h["last_hit"]),
            last_colours: [u(&h["last_hit"]["colour_equal"]), u(&h["last_hit"]["colour_less"]), u(&h["last_hit"]["colour_more"])],
            counter: Text::from(&h["counter"]),
            clean: Text::from(&h["clean"]),
            combo_frames: c["frames"].as_u64().unwrap_or(0) as u32,
            hits: Text::from(&c["hits"]),
            combo_label: Text::from(&c["label"]),
            combo_label_x: [n(&c["label"]["x"][0]), n(&c["label"]["x"][1])],
            damage: Text::from(&c["damage"]),
            damage_label: Text::from(&c["damage_label"]),
            damage_dx: [0, 1, 2].map(|k| n(&c["damage_label"]["dx"][k])),
            negative_dx: n(&c["damage_label"]["negative_dx"]),
            marker_frames: m["frames"].as_u64().unwrap_or(0) as u32,
            marker_ring: m["ring"].as_u64().unwrap_or(4).max(1) as usize,
            marker_page: xy(&m["page"]),
            marker_kinds: m["kinds"].as_object().into_iter().flatten().filter_map(|(k, v)| Some((u32::from_str_radix(k.trim_start_matches("0x"), 16).ok()?, v.as_u64()? as usize))).collect(),
            marker_default: m["default_kind"].as_u64().unwrap_or(0) as usize,
            no_marker: m["no_marker"].as_array().into_iter().flatten().filter_map(|v| Some(v.as_u64()? as u32)).collect(),
            labels: m["labels"].as_array().into_iter().flatten().map(|l| (u(&l["u"]), u(&l["v"]), xy(&l["clut"]), u(&l["w"]), u(&l["h"]))).collect(),
            burst: (u(&m["burst"]["u"]), u(&m["burst"]["v"]), xy(&m["burst"]["clut"]), xy(&m["burst"]["special_clut"])),
            burst_size: [u(&m["burst"]["w"]), u(&m["burst"]["h"])],
            intro: Text::from(&screens["hud"]["intro"]["practice"]),
            inactive_moveset: p["dummy"]["inactive_moveset"].as_i64(),
        }
    }
}

/// Practice modes (MODE SELECT items 0..2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticeMode {
    Freestyle,
    VsCpu,
    ComboTraining,
}

/// What the app must do after a menu key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// The menu closed: apply the settings (the CPU for VS CPU).
    Closed,
    /// PLAYER SELECT: back to character select.
    PlayerSelect,
    /// RESET: back to the main menu.
    Reset,
}

/// The hits taken by one side (0x800B3A84's 32-byte record).
#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    total: i32,
    combo_damage: i32,
    base: i32,
    actual: i32,
    timer: u32,
    count: u32,
    shown: u32,
    counter: bool,
    clean: bool,
}

#[derive(Clone, Debug)]
struct Marker {
    /// None: the hit takes a place in the ring but draws nothing.
    kind: Option<usize>,
    timer: u32,
    /// Hit point in metres (world).
    point: glam::Vec3,
    /// Where it lands on the screen, fixed when first drawn (the game projects it once, at the hit).
    screen: std::cell::Cell<Option<(f32, f32)>>,
}

/// The dummy's settings value names (TRAINING DUMMY list order).
const STAND: usize = 0;
const CROUCH: usize = 1;
const STAND_GUARD: usize = 2;
const CROUCH_GUARD: usize = 3;
const GUARD_ALL: usize = 4;
const AUTO_GUARD: usize = 5;
const QUICK_ROLL_DOWN: usize = 6;
const QUICK_ROLL_UP: usize = 7;
const CONTROLLER: usize = 8;

pub struct Practice {
    pub data: PracticeData,
    pub mode: PracticeMode,
    /// The menu is open (the fight is paused).
    pub open: bool,
    mode_select: bool,
    cursor_mode: usize,
    cursor_sub: [usize; 3],
    /// Item 0 reads OK once a setting changed, CANCEL before.
    changed: bool,
    /// Settings per sub-menu item (value index), by mode.
    values: [[usize; 8]; 3],
    /// The menu opened by itself once.
    opened_once: bool,
    startup: u32,
    stats: [Stats; 2],
    markers: Vec<Marker>,
    frame: u32,
}

impl Practice {
    pub fn new(data: PracticeData) -> Self {
        // defaults (0x800B2894): ATTACK DATA on, CPU MEDIUM / STAGE 3, dummy STAND
        let mut values = [[0; 8]; 3];
        for v in &mut values {
            v[4] = 1;
        }
        values[1][2] = 1;
        values[1][3] = 2;
        values[1][4] = 1;
        let startup = data.startup;
        Self {
            data,
            mode: PracticeMode::Freestyle,
            open: false,
            mode_select: true,
            cursor_mode: 0,
            cursor_sub: [0; 3],
            changed: false,
            values,
            opened_once: false,
            startup,
            stats: [Stats::default(); 2],
            markers: Vec::new(),
            frame: 0,
        }
    }

    fn mode_index(&self) -> usize {
        self.mode as usize
    }

    /// ATTACK DATA (item 4 in every sub-menu).
    fn attack_data(&self) -> bool {
        self.values[self.mode_index()][4] == 1
    }

    /// The dummy's TRAINING DUMMY setting (freestyle only; combo training stands).
    pub fn dummy(&self) -> usize {
        if self.mode == PracticeMode::Freestyle {
            self.values[0][2]
        } else {
            STAND
        }
    }

    /// VS CPU: (difficulty 0..2, level 0..9).
    pub fn cpu(&self) -> Option<(usize, usize)> {
        (self.mode == PracticeMode::VsCpu).then_some((self.values[1][2], self.values[1][3]))
    }

    /// The dummy is played from the second pad / keyboard layout.
    pub fn dummy_on_pad2(&self) -> bool {
        self.dummy() == CONTROLLER
    }

    pub fn open_menu(&mut self) {
        self.open = true;
        self.changed = false;
        self.cursor_sub = [0; 3];
    }

    /// Per fight step (also while paused): the menu opens by itself on the first fighting frame.
    pub fn step(&mut self, fighting: bool) {
        self.frame += 1;
        if fighting && !self.opened_once {
            self.opened_once = true;
            self.open_menu();
        }
        if self.open {
            return;
        }
        // nothing is drawn before the first menu and for 30 frames after it (0x800954BC grace)
        if self.opened_once {
            self.startup = self.startup.saturating_sub(1);
        }
        for s in &mut self.stats {
            s.timer = s.timer.saturating_sub(1);
        }
        for m in &mut self.markers {
            m.timer = m.timer.saturating_sub(1);
        }
        self.markers.retain(|m| m.timer > 0);
    }

    /// The hits of the last step. `neutral[v]`: side v acts freely again (its combo ends).
    pub fn hits(&mut self, hits: &[HitEvent], neutral: [bool; 2]) {
        for (v, free) in neutral.iter().enumerate() {
            if *free {
                self.stats[v].count = 0;
            }
        }
        for h in hits.iter().filter(|h| !h.blocked && h.damage > 0) {
            let s = &mut self.stats[h.victim];
            s.total += h.damage;
            s.base = h.base_damage;
            s.actual = h.damage;
            s.counter = h.counter;
            s.clean = h.clean;
            s.count = if s.count >= 99 { 0 } else { s.count + 1 };
            s.combo_damage = if s.count >= 2 { (s.combo_damage + h.damage).clamp(-999, 999) } else { h.damage };
            if s.count >= 2 {
                s.shown = s.count;
                s.timer = self.data.combo_frames;
            }
            // markers on the dummy, at the hit point
            if h.victim == 1 {
                let d = &self.data;
                let kind = d.marker_kinds.iter().find(|(m, _)| *m == h.mask).map_or(d.marker_default, |k| k.1);
                let kind = (!d.no_marker.contains(&h.mask)).then_some(kind);
                if self.markers.len() >= d.marker_ring {
                    self.markers.remove(0);
                }
                self.markers.push(Marker { kind, timer: d.marker_frames, point: h.point / kumite_game::UNITS, screen: Default::default() });
            }
        }
    }

    /// The dummy's pad (freestyle actions, 0x800B81A8); `facing_right`: the dummy faces +x; `low`: the attacker's
    /// current move hits low; `down`: the dummy's next move is a lying one, so it holds up to get up instead of
    /// its action (0x800B82E4; not with the quick rolls, nor for `inactive_moveset`).
    pub fn dummy_pad(&self, facing_right: bool, low: bool, down: bool) -> Pad {
        let back = |p: Pad| if facing_right { Pad { left: true, ..p } } else { Pad { right: true, ..p } };
        if down && !matches!(self.dummy(), QUICK_ROLL_DOWN | QUICK_ROLL_UP) {
            return Pad { up: true, ..Pad::default() };
        }
        match self.dummy() {
            CROUCH => Pad { down: true, ..Pad::default() },
            STAND_GUARD => back(Pad::default()),
            CROUCH_GUARD => back(Pad { down: true, ..Pad::default() }),
            GUARD_ALL | AUTO_GUARD => back(Pad { down: low, ..Pad::default() }),
            _ => Pad::default(),
        }
    }

    /// A menu key while the menu is open.
    pub fn key(&mut self, key: Key) -> Action {
        // the practice menu's sounds (record 6): cursor and value changes 0x546C, confirm 0x4CEB
        match key {
            Key::Up | Key::Down | Key::Left | Key::Right => sfx::play(sfx::cursor()),
            Key::Start | Key::Button(_) => sfx::play(sfx::back()),
            Key::Back => {}
        }
        if self.mode_select {
            let n = self.data.mode_select.items.len().max(1);
            match key {
                Key::Up | Key::Down => {
                    self.cursor_mode = if key == Key::Down { (self.cursor_mode + 1) % n } else { (self.cursor_mode + n - 1) % n };
                    // landing on a mode switches to it at once (0x800B44FC)
                    self.mode = match self.cursor_mode {
                        1 => PracticeMode::VsCpu,
                        2 => PracticeMode::ComboTraining,
                        0 => PracticeMode::Freestyle,
                        _ => self.mode,
                    };
                }
                Key::Start | Key::Button(_) => match self.cursor_mode {
                    0..=2 => {
                        self.mode_select = false;
                        self.changed = false;
                        self.cursor_sub = [0; 3];
                    }
                    3 => {
                        self.open = false;
                        return Action::PlayerSelect;
                    }
                    _ => {
                        self.open = false;
                        return Action::Reset;
                    }
                },
                Key::Back => {
                    self.open = false;
                    return Action::Closed;
                }
                _ => {}
            }
            return Action::None;
        }
        let m = self.mode_index();
        let items = self.data.menus.get(m).map_or(0, |mm| mm.items.len()).max(1);
        let cur = &mut self.cursor_sub[m];
        match key {
            Key::Up => *cur = (*cur + items - 1) % items,
            Key::Down => *cur = (*cur + 1) % items,
            Key::Left | Key::Right => {
                let len = self.data.menus.get(m).and_then(|mm| mm.items.get(*cur)).map_or(0, |i| if i.ok_cancel { 0 } else { i.values.len() });
                if len > 0 {
                    let v = &mut self.values[m][*cur];
                    *v = if key == Key::Right { (*v + 1) % len } else { (*v + len - 1) % len };
                    self.changed = true;
                }
            }
            Key::Start | Key::Button(_) => {
                let last = items - 1;
                if *cur == 0 {
                    self.open = false;
                    return Action::Closed;
                } else if *cur == last {
                    self.mode_select = true;
                } else {
                    // a value item: back to OK (COMMAND LIST is not done)
                    *cur = 0;
                }
            }
            Key::Back => self.mode_select = true,
        }
        Action::None
    }

    /// Everything practice draws, in front of the fight: "PRACTICE MODE" during the fly-in, the attack data and
    /// the open menu. `project`: a point in metres to the 368x480 screen.
    pub fn draw(&self, ctx: &egui::Context, art: &Art, intro: bool, project: &dyn Fn(glam::Vec3) -> Option<(f32, f32)>) {
        let v = View::fit(ctx.content_rect());
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("practice"))).with_clip_rect(v.rect);
        if intro {
            let t = &self.data.intro;
            art.text(&painter, &v, t.font, t.colour, t.x, t.y, &t.text, Color32::WHITE);
        }
        self.draw_hud(&painter, &v, art, project);
        if self.open {
            self.draw_menu(&painter, &v, art);
        }
    }

    /// The menu box and its lines (0x800B5318 / 0x800B545C...).
    fn draw_menu(&self, painter: &egui::Painter, v: &View, art: &Art) {
        let d = &self.data;
        let (menu, k, cursor) = if self.mode_select {
            (&d.mode_select, 0, self.cursor_mode)
        } else {
            let m = self.mode_index();
            match d.menus.get(m) {
                Some(menu) => (menu, 1, self.cursor_sub[m]),
                None => return,
            }
        };
        let (border, panel) = d.boxes[k];
        let [r, g, b] = d.border_rgb;
        Art::shade(painter, v, border, [Color32::from_rgb(r, g, b); 4]);
        let [r, g, b] = d.panel_rgb;
        Art::shade(painter, v, panel, [Color32::from_rgb(r, g, b); 4]);
        let x = d.menu_x[k];
        art.text(painter, v, d.title_font, d.title_colour, x, d.title_y, &menu.title, Color32::WHITE);
        let m = self.mode_index();
        for (i, item) in menu.items.iter().enumerate() {
            let colour = if i == cursor { d.selected_colour } else { d.item_colour };
            if item.ok_cancel {
                let t = item.values.get(usize::from(!self.changed)).map_or("OK", |s| s.as_str());
                art.text(painter, v, 0, colour, x, item.y, t, Color32::WHITE);
                continue;
            }
            art.text(painter, v, 0, colour, x, item.y, &item.text, Color32::WHITE);
            if k == 1 && !item.values.is_empty() {
                let val = self.values[m][i];
                let roll = item.value_y.is_some() && (6..=7).contains(&val);
                let text = if roll { d.quick_roll[val - 6].clone() } else { item.values.get(val).cloned().unwrap_or_default() };
                let y = item.value_y.unwrap_or(item.y);
                let right = x + d.value_right;
                let x0 = right - art.text_w(0, &text);
                // the quick rolls' last letter (d / u): the printer's arrow for it (the text's colour's CLUT, as the
                // font's)
                let arrow = d.arrows.iter().find(|a| roll && text.ends_with(a.0)).map(|a| a.1);
                if let Some([au, av]) = arrow {
                    let n = text.chars().count().saturating_sub(1);
                    let head: String = text.chars().take(n).collect();
                    art.text(painter, v, 0, colour, x0, y, &head, Color32::WHITE);
                    let t = Tex { page: d.arrow_page, bpp: 4, u: au, v: av, clut: Some(art.font_clut(0, colour)) };
                    art.sprite(painter, v, &["exe", "r010"], &t, d.arrow_size[0], d.arrow_size[1], x0 + art.text_w(0, &head), y, Color32::WHITE);
                } else {
                    art.text(painter, v, 0, colour, x0, y, &text, Color32::WHITE);
                }
            }
        }
    }

    /// The attack data over the fight (0x800B6248); `project`: metres -> screen pixels (368x480).
    fn draw_hud(&self, painter: &egui::Painter, v: &View, art: &Art, project: &dyn Fn(glam::Vec3) -> Option<(f32, f32)>) {
        if self.open || self.startup > 0 || !self.attack_data() {
            return;
        }
        let d = &self.data;
        let text = |t: &Text, colour: u32, x: f32, y: f32, s: &str| art.text(painter, v, t.font, colour, x, y, s, Color32::WHITE);
        // TOTAL DMG: what the player dealt (left) and took (right)
        let total = |s: &Stats| d.total.text.replace("%d", &s.total.to_string());
        text(&d.total, d.total.colour, d.total_x[0], d.total.y, &total(&self.stats[1]));
        text(&d.total, d.total.colour, d.total_x[1], d.total.y, &total(&self.stats[0]));
        let s = &self.stats[1];
        let pct = if s.base != 0 && s.actual != 0 { s.actual * 100 / s.base } else { 0 };
        let colour = d.last_colours[match s.actual.cmp(&s.base) {
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Less => 1,
            std::cmp::Ordering::Greater => 2,
        }];
        let last = d.last_hit.text.replacen("%03d", &format!("{:03}", s.actual), 1).replacen("%03d", &format!("{pct:03}"), 1).replace("%%", "%");
        text(&d.last_hit, colour, d.last_hit.x, d.last_hit.y, &last);
        if s.counter {
            text(&d.counter, d.counter.colour, d.counter.x, d.counter.y, &d.counter.text);
        }
        if s.clean {
            text(&d.clean, d.clean.colour, d.clean.x, d.clean.y, &d.clean.text);
        }
        // the combo box: hits + COMBO, damage + DAMAGE, for 90 frames after a combo hit
        if s.timer > 0 && s.shown >= 2 && s.combo_damage > 0 {
            let hits = s.shown.to_string();
            text(&d.hits, d.hits.colour, d.hits.x, d.hits.y, &hits);
            let lx = d.combo_label_x[usize::from(s.shown >= 10)];
            text(&d.combo_label, d.combo_label.colour, lx, d.combo_label.y, &d.combo_label.text);
            let dmg = s.combo_damage;
            text(&d.damage, d.damage.colour, d.damage.x, d.damage.y, &dmg.to_string());
            let a = dmg.abs();
            let dx = d.damage_dx[if a < 10 {
                0
            } else if a < 100 {
                1
            } else {
                2
            }] + if dmg < 0 { d.negative_dx } else { 0.0 };
            text(&d.damage_label, d.damage_label.colour, d.damage_label.x + dx, d.damage_label.y, &d.damage_label.text);
        }
        // HIGH / MID / LOW / ! at the hit point (burst under the label)
        let (bu, bv, bclut, bspecial) = d.burst;
        for m in self.markers.iter().rev() {
            if m.screen.get().is_none() {
                m.screen.set(project(m.point));
            }
            let (Some((x, y)), Some(kind)) = (m.screen.get(), m.kind) else { continue };
            let special = kind == 3;
            let burst = Tex { page: d.marker_page, bpp: 4, u: bu, v: bv, clut: Some(if special { bspecial } else { bclut }) };
            let [bw, bh] = d.burst_size;
            art.sprite(painter, v, &["exe"], &burst, bw, bh, x - (bw / 2) as f32, y - (bh / 2) as f32, Color32::WHITE);
            if let Some(&(lu, lv, clut, w, h)) = d.labels.get(kind) {
                let t = Tex { page: d.marker_page, bpp: 4, u: lu, v: lv, clut: Some(clut) };
                art.sprite(painter, v, &["exe"], &t, w, h, x - w as f32 / 2.0, y - h as f32 / 2.0, Color32::WHITE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Option<PracticeData> {
        let p = crate::test_assets().join("gamedata/screens.json");
        let b = kumite_fs::read(&p).ok()?;
        Some(PracticeData::load(&serde_json::from_slice(&b).ok()?))
    }

    fn hit(victim: usize, damage: i32, base: i32, mask: u32) -> HitEvent {
        HitEvent {
            attacker: 1 - victim,
            anim: String::new(),
            zone: 0,
            damage,
            base_damage: base,
            mask,
            blocked: false,
            counter: false,
            clean: false,
            strike: glam::Vec3::ZERO,
            limb: 0,
            ki: false,
            point: glam::Vec3::ZERO,
            victim,
            health: 100,
            throw: false,
        }
    }

    /// A downed dummy holds up to get up (0x800B82E4), whatever its action, except with the quick rolls.
    #[test]
    fn a_downed_dummy_gets_up() {
        let Some(d) = data() else { return };
        let mut p = Practice::new(d);
        for (value, up) in [(STAND, true), (CROUCH_GUARD, true), (QUICK_ROLL_DOWN, false)] {
            p.values[0][2] = value;
            assert_eq!(p.dummy_pad(true, false, true).up, up, "dummy {value}");
        }
        p.values[0][2] = CROUCH;
        assert!(p.dummy_pad(true, false, false).down && !p.dummy_pad(true, false, false).up);
    }

    #[test]
    fn the_menu_opens_by_itself_and_switches_modes() {
        let Some(d) = data() else { return };
        assert!(d.mode_select.items.len() == 5 && d.mode_select.items.iter().all(|i| !i.text.is_empty()));
        let mut p = Practice::new(d);
        p.step(false);
        assert!(!p.open);
        p.step(true);
        assert!(p.open, "opens on the first fighting frame");
        p.key(Key::Down);
        assert_eq!(p.mode, PracticeMode::VsCpu, "moving the cursor switches the mode");
        p.key(Key::Start);
        assert_eq!(p.cpu(), Some((1, 2)), "MEDIUM, STAGE 3 by default");
        p.key(Key::Down);
        p.key(Key::Down);
        p.key(Key::Right);
        assert_eq!(p.cpu(), Some((2, 2)));
        assert!(p.changed, "item 0 now reads OK");
        p.key(Key::Start);
        assert_eq!(p.cursor_sub[1], 0, "confirm on a value goes back to OK");
        assert_eq!(p.key(Key::Start), Action::Closed);
        assert!(!p.open);
    }

    #[test]
    fn combos_count_and_markers_appear() {
        let Some(d) = data() else { return };
        let mut p = Practice::new(d);
        p.opened_once = true;
        p.hits(&[hit(1, 10, 10, 0x412)], [false; 2]);
        p.hits(&[hit(1, 8, 10, 0x217)], [false; 2]);
        let s = p.stats[1];
        assert_eq!((s.total, s.count, s.shown, s.combo_damage, s.actual, s.base), (18, 2, 2, 18, 8, 10));
        assert_eq!(p.markers.iter().map(|m| m.kind).collect::<Vec<_>>(), [Some(0), Some(1)], "HIGH then MID");
        p.hits(&[], [false, true]);
        p.hits(&[hit(1, 5, 5, 0x10F)], [false; 2]);
        assert_eq!((p.stats[1].count, p.stats[1].combo_damage, p.stats[1].total), (1, 5, 23), "a new combo after recovering");
        p.hits(&[hit(1, 1, 1, 0x800), hit(1, 1, 1, 0x123)], [false; 2]);
        assert_eq!(p.markers.iter().map(|m| m.kind).collect::<Vec<_>>()[2..], [None, Some(0)], "0x800: no marker; others: HIGH");
    }
}
