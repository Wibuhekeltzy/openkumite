//! OPTION > KEY CONFIGURATION, the original's screen (record 10, 0x800DF014): a panel for each player (P1 at
//! x 0, P2 at 184) with the pad's picture, a label box on each of the eight buttons it sets (L2 R2 L1 R1 and the
//! four faces) led to the button, and the rows SETTING, VIBRATION, DEFAULT, EXIT. On SETTING, holding one of the
//! eight buttons opens its list of 13 commands; up / down picks, releasing closes. (With a keyboard, OpenKumite's
//! way: left / right opens the next button's list.) OPTION > KEYBOARD is OpenKumite's: each player's keys.
//! Every change applies at once and is kept in the save.

use crate::art::{int as u, num, pair, Art, Screen as View, Tex};
use crate::controls::{key_label, reserved, Command, Controls, PadButton, KEYS_PER_COMMAND, KEY_COMMANDS};
use crate::menu::{sfx, triangle, Key, Outcome};
use egui::Color32;
use winit::keyboard::KeyCode;

/// The configurable buttons in the game's order (L2 R2 L1 R1 triangle circle cross square). Their places on the
/// panel come from screens.json `key_config` (0x800EAF0C: the closed label box, the open list's column, the leader's
/// start in the box, the border colour; 0x800B9564: the buttons' tips on the DualShock's and the digital pad's
/// pictures).
const BUTTONS: [PadButton; 8] = [PadButton::L2, PadButton::R2, PadButton::L1, PadButton::R1, PadButton::North, PadButton::East, PadButton::South, PadButton::West];

/// The list opens over this many frames.
const OPEN_FRAMES: u32 = 8;

#[derive(Clone, Copy, Debug, Default)]
struct Panel {
    row: usize,
    /// The button whose list is open (index in [`BUTTONS`]), and whether a keyboard opened it.
    open: Option<(usize, bool)>,
    /// Frames since it opened (the list grows over [`OPEN_FRAMES`]).
    t: u32,
}

pub struct KeyConfig {
    pub frame: u32,
    panels: [Panel; 2],
    /// Each player's gamepad: connected (the DualShock picture; none, the digital pad's).
    pads: [bool; 2],
    pub controls: Controls,
    pub difficulty: usize,
    pub done: bool,
}

impl KeyConfig {
    pub fn new(controls: Controls, difficulty: usize) -> Self {
        Self { frame: 0, panels: [Panel::default(); 2], pads: [false; 2], controls, difficulty, done: false }
    }

    /// A frame: the buttons each player's gamepad holds (None: no pad). On SETTING, holding exactly one of the
    /// eight opens its list; letting go closes it.
    pub fn hold(&mut self, held: [Option<&[PadButton]>; 2]) {
        for (p, held) in held.into_iter().enumerate() {
            self.pads[p] = held.is_some();
            let panel = &mut self.panels[p];
            let Some(held) = held else { continue };
            let pressed: Vec<usize> = (0..8).filter(|&i| held.contains(&BUTTONS[i])).collect();
            match (panel.row, pressed.as_slice(), panel.open) {
                (0, [i], open) if open.is_none_or(|(o, _)| o != *i) => {
                    panel.open = Some((*i, false));
                    panel.t = 0;
                    sfx::play(sfx::list_open());
                }
                (_, [], Some((_, false))) => panel.open = None,
                _ => {}
            }
        }
    }

    /// A frame: the open lists grow.
    pub fn tick(&mut self) {
        self.frame += 1;
        for panel in self.panels.iter_mut() {
            panel.t = (panel.t + 1).min(OPEN_FRAMES);
        }
    }

    /// Player `p`'s panel on SETTING (its pad's face buttons hold there, they do not pick).
    pub fn on_setting(&self, p: usize) -> bool {
        self.panels[p.min(1)].row == 0
    }

    fn changed(&self) -> Option<Outcome> {
        Some(Outcome::Controls(self.controls.clone()))
    }

    /// A menu key of player `p` (a keyboard layout's or a gamepad's): the controls changed, or None.
    pub fn key(&mut self, p: usize, key: Key) -> Option<Outcome> {
        let p = p.min(1);
        let panel = &mut self.panels[p];
        if let Some((i, by_keyboard)) = panel.open {
            let b = BUTTONS[i];
            match key {
                Key::Up | Key::Down => {
                    let cmds = &Command::PAD;
                    let n = cmds.len();
                    let at = cmds.iter().position(|c| *c == self.controls.pads[p].command(b)).unwrap_or(n - 1);
                    let next = if key == Key::Down { (at + 1) % n } else { (at + n - 1) % n };
                    self.controls.pads[p].set(b, cmds[next]);
                    sfx::play(sfx::cursor());
                    return self.changed();
                }
                // (a keyboard's list: the next button, or closed)
                Key::Left | Key::Right if by_keyboard => {
                    panel.open = Some(((i + if key == Key::Right { 1 } else { 7 }) % 8, true));
                    panel.t = 0;
                    sfx::play(sfx::list_open());
                }
                Key::Back | Key::Start if by_keyboard => {
                    panel.open = None;
                    sfx::play(sfx::cursor());
                }
                _ => {}
            }
            return None;
        }
        match key {
            Key::Up => {
                panel.row = (panel.row + 3) % 4;
                sfx::play(sfx::cursor());
            }
            Key::Down => {
                panel.row = (panel.row + 1) % 4;
                sfx::play(sfx::cursor());
            }
            Key::Left | Key::Right if panel.row == 0 => {
                panel.open = Some((if key == Key::Right { 0 } else { 7 }, true));
                panel.t = 0;
                sfx::play(sfx::list_open());
            }
            Key::Left | Key::Right if panel.row == 1 => {
                self.controls.pads[p].vibration = !self.controls.pads[p].vibration;
                sfx::play(sfx::cursor());
                return self.changed();
            }
            // SELECT on either pad leaves
            Key::Back => {
                sfx::play(sfx::back());
                self.done = true;
            }
            Key::Start | Key::Button(_) if panel.row == 2 => {
                self.controls.pads[p] = Default::default();
                sfx::play(sfx::confirm());
                return self.changed();
            }
            Key::Start | Key::Button(_) if panel.row == 3 => {
                sfx::play(sfx::back());
                self.done = true;
            }
            _ => {}
        }
        None
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art) {
        crate::menu::title_picture(painter, v, art, true);
        let title = art.screens["key_config"]["texts"]["title"].as_str().unwrap_or_default();
        let t = crate::modes::Text::from(&art.screens["key_config"]["draw"]["title"]);
        art.text(painter, v, t.font, t.colour, t.x, t.y, title, Color32::WHITE);
        for p in 0..2 {
            self.draw_panel(painter, v, art, p, crate::art::CENTRE * p as f32);
        }
    }

    fn draw_panel(&self, painter: &egui::Painter, v: &View, art: &Art, p: usize, ox: f32) {
        let panel = &self.panels[p];
        let layout = &self.controls.pads[p];
        // the pad: the DualShock's picture when a pad is there, the digital one's otherwise
        let kc = &art.screens["key_config"];
        let dr = &kc["draw"];
        let pad = &dr["pad"];
        let k = usize::from(!self.pads[p]);
        let tex = Tex::at(pair(&pad["vram"][k]), u(&pad["bpp"]), Some(pair(&pad["cluts"][k])));
        let tips = if self.pads[p] { &kc["tips"]["dualshock"] } else { &kc["tips"]["digital"] };
        art.sprite(painter, v, &["r010"], &tex, u(&pad["w"]), u(&pad["h"]), ox + num(&pad["x"]), num(&pad["y"]), Color32::WHITE);
        let pulse = |a: u32| (triangle(a) / 2 + 128) as f32 / 255.0;
        let rgb = &dr["bar"]["rgb"];
        let bar = |x: f32, y: f32, w: f32, h: f32, k: f32| {
            let c = Color32::from_rgb((u(&rgb[0]) as f32 * k) as u8, (u(&rgb[1]) as f32 * k) as u8, (u(&rgb[2]) as f32 * k) as u8);
            Art::shade(painter, v, [x, y, w, h], [c; 4]);
        };
        let bx = &dr["box"];
        let (box_w, box_h, inner, row_h, rows, open_h) = (num(&bx["w"]), num(&bx["h"]), num(&bx["inner"]), num(&bx["row"]), u(&bx["rows"]) as usize, num(&bx["open"]));
        // the label boxes, led to their buttons (the open one last, over the others)
        let order = (0..8).filter(|&i| panel.open.is_none_or(|o| o.0 != i)).chain(panel.open.map(|o| o.0));
        for i in order {
            let place = &kc["places"]["entries"][i];
            let rgb = |k: usize| place["colour"][k].as_u64().unwrap_or(0) as u8;
            let (button, colour) = (BUTTONS[i], Color32::from_rgb(rgb(0), rgb(1), rgb(2)));
            let cmd = layout.command(button);
            let open = panel.open.is_some_and(|o| o.0 == i);
            let t = if open { panel.t as f32 / OPEN_FRAMES as f32 } else { 0.0 };
            let lerp = |a: f32, b: f32| a + (b - a) * t;
            // open: from the box to the list's column, the box growing to the whole list
            let x = ox + lerp(num(&place["boxed"][0]), num(&place["list"][0]));
            let y = lerp(num(&place["boxed"][1]), num(&place["list"][1]));
            let h = box_h + open_h * t;
            // the leader: a line from the box to the button's tip, and a small triangle on it
            if !open {
                let from = [x + num(&place["lead"][0]), y + num(&place["lead"][1])];
                let tip = [ox + num(&tips[i][0]), num(&tips[i][1])];
                painter.line_segment([v.pos(from[0], from[1]), v.pos(tip[0], tip[1])], egui::Stroke::new(v.rect(0.0, 0.0, 1.0, 1.0).width(), colour));
                let tri = [v.pos(tip[0] - 2.0, tip[1] - 2.0), v.pos(tip[0] + 2.0, tip[1] - 2.0), v.pos(tip[0], tip[1] + 2.0)];
                painter.add(egui::Shape::convex_polygon(tri.to_vec(), colour, egui::Stroke::NONE));
            }
            let edge = (box_w - inner) / 2.0;
            Art::shade(painter, v, [x, y, box_w, h], [colour; 4]);
            Art::shade(painter, v, [x + edge, y + 2.0, inner, h - 4.0], [Color32::BLACK; 4]);
            if open {
                // the commands, a row each: the button's red on a pulsing bar, ones other buttons have dim
                let used: Vec<Command> = BUTTONS.iter().filter(|&&q| q != button).map(|&q| layout.command(q)).collect();
                let shown = ((t * rows as f32).ceil() as usize).min(rows);
                for (k, c) in Command::PAD.iter().enumerate().take(shown) {
                    let ry = y + 2.0 + row_h * k as f32;
                    let colour = if *c == cmd {
                        bar(x + edge, ry, inner, row_h, if panel.t >= OPEN_FRAMES { pulse(self.frame << 4) } else { 1.0 });
                        5
                    } else if used.contains(c) {
                        10
                    } else {
                        6
                    };
                    self.command_label(painter, v, art, *c, x, ry + 1.0, colour);
                }
            } else {
                self.command_label(painter, v, art, cmd, x, y + 3.0, 6);
            }
        }
        // the rows: the cursor's on a pulsing blue bar (steady while a list is open)
        // (the rows: screens.json key_config.texts)
        let texts = &kc["texts"];
        let text = |key: &str| texts[key].as_str().unwrap_or_default();
        let (rw, b) = (&dr["rows"], &dr["bar"]);
        for (k, label) in ["setting", "vibration", "default", "exit"].map(text).iter().enumerate() {
            let (x, y) = (ox + num(&rw["x"]), num(&rw["y"][k]));
            let here = panel.row == k;
            if here {
                let bb = if panel.open.is_some() { 1.0 } else { pulse(self.frame << 4) };
                bar(x + num(&b["dx"]), y + num(&b["dy"]), num(&rw["w"][k]) + num(&b["dw"]), num(&b["h"]), bb);
            }
            art.text(painter, v, 0, u(&rw["colours"][usize::from(here)]), x, y, label, Color32::WHITE);
            if k == 1 {
                let (val, c) = if layout.vibration { (text("yes"), u(&rw["values"][1])) } else { (text("no"), u(&rw["values"][0])) };
                art.text(painter, v, 0, c, x + num(&rw["value_dx"]), y, val, Color32::WHITE);
            }
        }
    }

    /// A command's label centred in its box at x (font 0); the sidesteps with their arrow (record 10's glyphs) in the
    /// gap before STEP.
    fn command_label(&self, painter: &egui::Painter, v: &View, art: &Art, c: Command, x: f32, y: f32, colour: u32) {
        let kc = &art.screens["key_config"];
        let text = c.label(&kc["texts"]["commands"]);
        let half = (kc["draw"]["box"]["w"].as_u64().unwrap_or(0) / 2) as f32;
        art.text(painter, v, 0, colour, x + half - (art.text_w(0, &text) as u32 / 2) as f32, y, &text, Color32::WHITE);
        let k = match c {
            Command::StepUp => 0,
            Command::StepDown => 1,
            _ => return,
        };
        let a = &kc["draw"]["arrows"];
        let [tu, tv] = pair(&a["uv"][k]);
        let t = Tex { page: pair(&a["page"]), bpp: 4, u: tu, v: tv, clut: Some(art.font_clut(0, colour)) };
        art.sprite(painter, v, &["r010"], &t, u(&a["w"]), u(&a["h"]), x + num(&a["dx"]), y, Color32::WHITE);
    }
}

/// OPTION > KEYBOARD (OpenKumite's): each player's keys, two for each command; START on one and press the new key.
pub struct KeyboardConfig {
    pub frame: u32,
    player: usize,
    cursor: usize,
    /// The column (the first key or the second).
    slot: usize,
    /// Waiting for a key to bind to the row under the cursor.
    pub waiting: bool,
    pub controls: Controls,
    pub difficulty: usize,
    pub done: bool,
}

/// A row of the keyboard page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Player,
    Command(Command),
    Default,
    Exit,
}

impl KeyboardConfig {
    pub fn new(controls: Controls, difficulty: usize) -> Self {
        Self { frame: 0, player: 0, cursor: 0, slot: 0, waiting: false, controls, difficulty, done: false }
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = vec![Row::Player];
        rows.extend(KEY_COMMANDS.iter().map(|&c| Row::Command(c)));
        rows.extend([Row::Default, Row::Exit]);
        rows
    }

    fn changed(&self) -> Option<Outcome> {
        Some(Outcome::Controls(self.controls.clone()))
    }

    pub fn key(&mut self, key: Key) -> Option<Outcome> {
        if self.waiting {
            if key == Key::Back {
                self.waiting = false;
                sfx::play(sfx::back());
            }
            return None;
        }
        let rows = self.rows();
        let n = rows.len();
        let step = |i: usize, n: usize, right: bool| if right { (i + 1) % n } else { (i + n - 1) % n };
        match (key, rows[self.cursor.min(n - 1)]) {
            (Key::Up, _) => {
                sfx::play(sfx::cursor());
                self.cursor = step(self.cursor, n, false);
            }
            (Key::Down, _) => {
                sfx::play(sfx::cursor());
                self.cursor = step(self.cursor, n, true);
            }
            (Key::Back, _) | (Key::Start | Key::Button(_), Row::Exit) => {
                sfx::play(sfx::back());
                self.done = true;
            }
            (Key::Left | Key::Right | Key::Start | Key::Button(_), Row::Player) => {
                self.player = 1 - self.player;
                sfx::play(sfx::cursor());
            }
            (Key::Left | Key::Right, Row::Command(_)) => {
                self.slot = step(self.slot, KEYS_PER_COMMAND, key == Key::Right);
                sfx::play(sfx::cursor());
            }
            (Key::Start | Key::Button(_), Row::Command(_)) => {
                self.waiting = true;
                sfx::play(sfx::confirm());
            }
            (Key::Start | Key::Button(_), Row::Default) => {
                self.controls.keyboard[self.player] = Controls::default().keyboard[self.player].clone();
                sfx::play(sfx::confirm());
                return self.changed();
            }
            _ => {}
        }
        None
    }

    /// A key pressed while waiting: bound to the row's command (Esc cancels, Backspace clears the slot).
    pub fn capture(&mut self, code: KeyCode) -> Option<Outcome> {
        self.waiting = false;
        let Some(&Row::Command(c)) = self.rows().get(self.cursor) else { return None };
        let p = self.player;
        match code {
            KeyCode::Escape => {
                sfx::play(sfx::back());
                None
            }
            KeyCode::Backspace => {
                let layout = &mut self.controls.keyboard[p];
                if let Some(k) = layout.keys_of(c).get(self.slot).copied() {
                    layout.keys.iter_mut().filter(|(x, _)| *x == c).for_each(|(_, keys)| keys.retain(|&x| x != k));
                }
                sfx::play(sfx::back());
                self.changed()
            }
            k if reserved(k) => {
                sfx::play(sfx::back());
                None
            }
            k => {
                // (a key plays one player: the other's layout loses it)
                for (_, keys) in self.controls.keyboard[1 - p].keys.iter_mut() {
                    keys.retain(|&x| x != k);
                }
                let slot = self.slot.min(self.controls.keyboard[p].keys_of(c).len());
                self.controls.keyboard[p].bind(c, slot, k);
                sfx::play(sfx::confirm());
                self.changed()
            }
        }
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art) {
        crate::menu::title_picture(painter, v, art, true);
        art.text(painter, v, 1, 6, crate::art::CENTRE - art.text_w(1, "KEYBOARD") / 2.0, 28.0, "KEYBOARD", Color32::WHITE);
        let rows = self.rows();
        let bob = triangle((self.frame & 31) << 4) as f32 / 32.0;
        let ([left, right], [aw, ah]) = crate::menu::arrows(&art.screens["select"]);
        let colour = |k: usize| if k == self.cursor { 5 } else { 6 };
        let label = if self.player == 0 { "1P" } else { "2P" };
        let x = crate::art::CENTRE - art.text_w(1, label) / 2.0;
        art.text(painter, v, 1, colour(0), x, 60.0, label, Color32::WHITE);
        if self.cursor == 0 {
            art.sprite(painter, v, &["exe", "r009"], &left, aw, ah, x - 14.0 - bob, 64.0, Color32::WHITE);
            art.sprite(painter, v, &["exe", "r009"], &right, aw, ah, x + art.text_w(1, label) + 4.0 + bob, 64.0, Color32::WHITE);
        }
        let layout = &self.controls.keyboard[self.player];
        for (k, row) in rows.iter().enumerate().skip(1) {
            let y = 96.0 + 20.0 * (k - 1) as f32 + if matches!(row, Row::Default | Row::Exit) { 8.0 } else { 0.0 };
            match row {
                Row::Command(c) => {
                    art.text(painter, v, 0, colour(k), 24.0, y, &c.label(&art.screens["key_config"]["texts"]["commands"]), Color32::WHITE);
                    let keys = layout.keys_of(*c);
                    for slot in 0..KEYS_PER_COMMAND {
                        let here = k == self.cursor && slot == self.slot;
                        let text = if here && self.waiting {
                            if self.frame & 16 != 0 {
                                "?".to_string()
                            } else {
                                String::new()
                            }
                        } else {
                            keys.get(slot).map_or("---".to_string(), |&key| key_label(key))
                        };
                        art.text(painter, v, 0, if here { 2 } else { colour(k) }, 128.0 + 120.0 * slot as f32, y, &text, Color32::WHITE);
                    }
                }
                Row::Default => art.text(painter, v, 0, colour(k), crate::art::CENTRE - art.text_w(0, "DEFAULT") / 2.0, y, "DEFAULT", Color32::WHITE),
                _ => art.text(painter, v, 0, colour(k), crate::art::CENTRE - art.text_w(0, "EXIT") / 2.0, y, "EXIT", Color32::WHITE),
            }
        }
        let hint = if self.waiting { "PRESS A KEY  ESC:CANCEL  BS:CLEAR" } else { "START: SET A KEY" };
        art.text(painter, v, 0, 6, crate::art::CENTRE - art.text_w(0, hint) / 2.0, 456.0, hint, Color32::WHITE);
    }
}
