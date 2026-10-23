//! ONLINE (OpenKumite's own main menu entry): how to meet (create a room and give its code, join a room by its code,
//! host by IP, join an IP; no IP in a browser), the code or address, a nickname, how many play at this computer (1
//! or 2), then START: into the room (`room.rs`), up to four players from up to four computers; its host chooses
//! the mode.
//!
//! The code, address and nickname are typed on the keyboard, or edited with a pad: LEFT / RIGHT pick the place,
//! buttons 1 / 2 turn the character there, button 3 deletes it.

use crate::art::{Art, Screen as View};
use crate::menu::{sfx, Key, Outcome};
use egui::Color32;

/// How to meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    CreateRoom,
    JoinRoom,
    HostIp,
    JoinIp,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Method::CreateRoom => "CREATE ROOM",
            Method::JoinRoom => "JOIN ROOM",
            Method::HostIp => "HOST BY IP",
            Method::JoinIp => "JOIN BY IP",
        }
    }

    /// The ways this build has (a browser has no raw UDP).
    fn all() -> Vec<Method> {
        if cfg!(target_arch = "wasm32") {
            vec![Method::CreateRoom, Method::JoinRoom]
        } else {
            vec![Method::CreateRoom, Method::JoinRoom, Method::HostIp, Method::JoinIp]
        }
    }
}

/// What START hands to the app.
#[derive(Clone, Debug)]
pub struct OnlineSetup {
    pub method: Method,
    /// The room code, or the address to join.
    pub code: String,
    pub name: String,
    /// Players at this computer (1, or 2: the keyboard's two layouts / two gamepads).
    pub locals: u8,
}

/// The nickname last used (the app sets it from the save, and keeps what START used).
static NAME: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn set_default_name(n: &str) {
    if let Ok(mut g) = NAME.lock() {
        *g = n.to_string();
    }
}

/// (only the letters the nickname row types, as many as it holds: the editing cursor counts bytes, and a save or
/// `--name` may say anything)
pub fn default_name() -> String {
    let name = NAME.lock().ok().map(|g| g.chars().filter(|c| NAME_CHARS.contains(*c)).take(NAME_LEN).collect::<String>());
    name.filter(|n| !n.is_empty()).unwrap_or_else(|| "PLAYER".to_string())
}

/// The room code's letters (no 0 / O, 1 / I: read aloud or off a screen without mistakes).
const CODE_CHARS: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const IP_CHARS: &str = "0123456789.:";
const CODE_LEN: usize = 5;
const NAME_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const NAME_LEN: usize = 10;
/// An address's longest (an IPv4 address and a port).
const ADDRESS_LEN: usize = 21;

#[derive(Clone)]
pub struct OnlineMenu {
    methods: Vec<Method>,
    method: usize,
    room: String,
    address: String,
    name: String,
    /// This computer's address when hosting (shown, not edited).
    pub host_address: String,
    row: usize,
    edit: usize,
    frame: u32,
    locals: u8,
    pub difficulty: usize,
    /// Meeting the other player: what the app reports.
    pub waiting: Option<String>,
}

const ROWS: usize = 5;

impl OnlineMenu {
    pub fn new(seed: u32, difficulty: usize) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let host_address = crate::direct::local_ip().map_or("THIS COMPUTER".to_string(), |ip| format!("{ip}:{}", crate::direct::DEFAULT_PORT));
        #[cfg(target_arch = "wasm32")]
        let host_address = String::new();
        // the room code from the system's randomness (anyone guessing it walks into the room); the seed if there is
        // none
        let mut bytes = [0; 4];
        let mut s = getrandom::getrandom(&mut bytes).map_or(seed, |()| u32::from_le_bytes(bytes)) | 1;
        let mut room = String::new();
        for _ in 0..CODE_LEN {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let k = (s >> 16) as usize % CODE_CHARS.len();
            room.push(CODE_CHARS.as_bytes()[k] as char);
        }
        Self { methods: Method::all(), method: 0, room, address: String::new(), name: default_name(), host_address, row: 0, edit: 0, frame: 0, locals: 1, difficulty, waiting: None }
    }

    /// Testing: the way (`Method::all` index) and the code or address filled in.
    pub fn preset(&mut self, method: usize, code: Option<String>) {
        self.method = method.min(self.methods.len() - 1);
        if let Some(c) = code {
            match self.method_now() {
                Method::CreateRoom | Method::JoinRoom => self.room = c.to_uppercase().chars().filter(|c| CODE_CHARS.contains(*c)).take(CODE_LEN).collect(),
                Method::JoinIp => self.address = c.chars().filter(|c| IP_CHARS.contains(*c)).take(ADDRESS_LEN).collect(),
                Method::HostIp => {}
            }
        }
        self.row = 0;
        self.edit = self.text().0.len();
    }

    pub fn tick(&mut self) {
        self.frame += 1;
    }

    fn method_now(&self) -> Method {
        self.methods[self.method.min(self.methods.len() - 1)]
    }

    /// The text of the code / address row, and whether it is typed (else it is shown).
    fn text(&self) -> (String, bool) {
        match self.method_now() {
            Method::CreateRoom => (self.room.clone(), false),
            Method::JoinRoom => (self.room.clone(), true),
            Method::HostIp => (self.host_address.clone(), false),
            Method::JoinIp => (self.address.clone(), true),
        }
    }

    /// The text the row being edited holds (the code / address row when it is typed, the nickname), and its
    /// characters and length.
    fn text_mut(&mut self) -> Option<(&mut String, &'static str, usize)> {
        // (the editing cursor counts bytes: every text here is of its row's ASCII letters, filtered where it comes in)
        debug_assert!(self.room.is_ascii() && self.address.is_ascii() && self.name.is_ascii());
        match (self.row, self.method_now()) {
            (1, Method::JoinRoom) => Some((&mut self.room, CODE_CHARS, CODE_LEN)),
            (1, Method::JoinIp) => Some((&mut self.address, IP_CHARS, ADDRESS_LEN)),
            (2, _) => Some((&mut self.name, NAME_CHARS, NAME_LEN)),
            _ => None,
        }
    }

    /// The edited row's text as it is.
    fn edited(&self) -> String {
        if self.row == 2 {
            self.name.clone()
        } else {
            self.text().0
        }
    }

    /// A row is being typed in (the code / address when it is typed, the nickname): the keyboard's letters go
    /// there, not to the menu.
    pub fn typing(&self) -> bool {
        self.waiting.is_none() && ((self.row == 1 && self.text().1) || self.row == 2)
    }

    /// A character typed on the keyboard (the code or address row, when it is typed).
    pub fn type_char(&mut self, c: char) {
        if self.waiting.is_some() {
            return;
        }
        let c = c.to_ascii_uppercase();
        let edit = self.edit;
        if let Some((t, set, max)) = self.text_mut() {
            if c == '\u{8}' {
                if edit > 0 && edit <= t.len() {
                    t.remove(edit - 1);
                    self.edit -= 1;
                }
            } else if set.contains(c) && t.len() < max {
                let at = edit.min(t.len());
                t.insert(at, c);
                self.edit = at + 1;
            }
        }
    }

    pub fn key(&mut self, key: Key) -> Outcome {
        if self.waiting.is_some() {
            return match key {
                Key::Back => {
                    sfx::play(sfx::back());
                    self.waiting = None;
                    Outcome::CancelOnline
                }
                _ => Outcome::None,
            };
        }
        match (self.row, key) {
            (_, Key::Back) => {
                sfx::play(sfx::back());
                return Outcome::Quit;
            }
            (_, Key::Up | Key::Down) => {
                sfx::play(sfx::cursor());
                self.row = if key == Key::Down { (self.row + 1) % ROWS } else { (self.row + ROWS - 1) % ROWS };
                self.edit = self.edited().len();
            }
            (0, Key::Left | Key::Right) => {
                sfx::play(sfx::cursor());
                let m = self.methods.len();
                self.method = if key == Key::Right { (self.method + 1) % m } else { (self.method + m - 1) % m };
                self.edit = self.text().0.len();
            }
            (3, Key::Left | Key::Right | Key::Button(_)) => {
                sfx::play(sfx::cursor());
                self.locals = 3 - self.locals;
            }
            (1 | 2, Key::Left) => self.edit = self.edit.saturating_sub(1),
            (1 | 2, Key::Right) => self.edit = (self.edit + 1).min(self.edited().len()),
            // a pad: buttons 1 / 2 turn the character at the place, 3 deletes it
            (1 | 2, Key::Button(b)) => {
                let edit = self.edit;
                if let Some((t, set, max)) = self.text_mut() {
                    let set: Vec<char> = set.chars().collect();
                    sfx::play(sfx::cursor());
                    if b == 2 {
                        if edit < t.len() {
                            t.remove(edit);
                        }
                    } else {
                        let step = if b == 1 { set.len() - 1 } else { 1 };
                        if edit >= t.len() {
                            if t.len() < max {
                                t.push(set[0]);
                            }
                        } else {
                            let k = set.iter().position(|&c| c == t.as_bytes()[edit] as char).unwrap_or(0);
                            t.replace_range(edit..edit + 1, &set[(k + step) % set.len()].to_string());
                        }
                    }
                }
            }
            (4, Key::Button(_)) | (_, Key::Start) => {
                let (code, _) = self.text();
                if code.is_empty() || self.name.is_empty() {
                    sfx::play(sfx::back());
                    self.row = if code.is_empty() { 1 } else { 2 };
                    return Outcome::None;
                }
                sfx::play(sfx::confirm());
                self.waiting = Some("CONNECTING".to_string());
                set_default_name(&self.name);
                return Outcome::StartOnline(Box::new(OnlineSetup { method: self.method_now(), code, name: self.name.clone(), locals: self.locals }));
            }
            _ => {}
        }
        Outcome::None
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art) {
        Art::shade(painter, v, crate::art::FULL, [Color32::from_rgb(0x06, 0x10, 0x28), Color32::from_rgb(0x06, 0x10, 0x28), Color32::BLACK, Color32::BLACK]);
        let centred = |font: usize, colour: u32, y: f32, t: &str| art.text(painter, v, font, colour, crate::art::CENTRE - art.text_w(font, t) / 2.0, y, t, Color32::WHITE);
        centred(1, 6, 24.0, "ONLINE");
        let (text, typed) = self.text();
        let label = match self.method_now() {
            Method::CreateRoom | Method::JoinRoom => "ROOM CODE",
            Method::HostIp => "YOUR ADDRESS",
            Method::JoinIp => "ADDRESS",
        };
        let rows: [(&str, String); ROWS] = [
            ("HOW", self.method_now().name().to_string()),
            (label, if text.is_empty() { "_".to_string() } else { text.clone() }),
            ("NICKNAME", if self.name.is_empty() { "_".to_string() } else { self.name.clone() }),
            ("PLAYERS HERE", self.locals.to_string()),
            ("", "START".to_string()),
        ];
        let blink = self.frame & 0x10 != 0;
        for (k, (name, value)) in rows.iter().enumerate() {
            let y = 84.0 + 44.0 * k as f32;
            let on = k == self.row && self.waiting.is_none();
            if on {
                Art::shade(painter, v, [16.0, y - 6.0, 336.0, 30.0], [Color32::from_rgba_unmultiplied(255, 255, 255, 40); 4]);
            }
            art.text(painter, v, 0, 6, 28.0, y + 2.0, name, Color32::WHITE);
            // (a value too wide for the row, a long address: the small font)
            let font = if 140.0 + art.text_w(1, value) > 352.0 { 0 } else { 1 };
            art.text(painter, v, font, if on { 9 } else { 0 }, 140.0, if font == 0 { y + 2.0 } else { y }, value, Color32::WHITE);
            // the rows LEFT / RIGHT turn: arrows either side of the value
            if on && (k == 0 || k == 3) {
                let (x0, x1, cy) = (130.0, 146.0 + art.text_w(font, value), y + 9.0);
                let tri =
                    |tip: f32, base: f32| egui::Shape::convex_polygon(vec![v.pos(tip, cy), v.pos(base, cy - 6.0), v.pos(base, cy + 6.0)], Color32::from_rgb(0xF0, 0xC0, 0x20), egui::Stroke::NONE);
                painter.add(tri(x0 - 6.0, x0));
                painter.add(tri(x1 + 6.0, x1));
            }
            // the place being edited
            if ((k == 1 && typed) || k == 2) && on && blink {
                let at = value.chars().take(self.edit).collect::<String>();
                let x = 140.0 + art.text_w(font, &at);
                Art::shade(painter, v, [x, y + 18.0, 10.0, 2.0], [Color32::WHITE; 4]);
            }
        }
        let hint = match self.row {
            1 if typed => "TYPE IT, OR BUTTONS 1/2 CHANGE, 3 DELETES",
            1 if self.method_now() == Method::CreateRoom => "GIVE THIS CODE TO THE OTHERS",
            1 => "THE OTHERS JOIN THIS ADDRESS (UDP 7000)",
            2 => "YOUR NAME FOR THE OTHERS",
            3 => "2: THE KEYBOARD'S TWO SIDES / TWO PADS",
            _ => "START: CONNECT   BACK: MAIN MENU",
        };
        centred(0, 7, 400.0, hint);
        if let Some(w) = &self.waiting {
            Art::shade(painter, v, [0.0, 196.0, crate::art::SCREEN_W, 80.0], [Color32::from_black_alpha(220); 4]);
            centred(0, if blink { 5 } else { 6 }, 216.0, w);
            centred(0, 6, 246.0, "BACK: CANCEL");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nickname from the save or `--name` with letters the row cannot type (the cursor counts bytes: an É was a
    /// panic on the first edit) keeps only those it can, as many as it holds.
    #[test]
    fn a_saved_nickname_keeps_only_what_the_row_types() {
        set_default_name("JOSÉ-MARÍA DE LA CRUZ");
        assert_eq!(default_name(), "JOSMARADEL");
        set_default_name("ÉÉÉ");
        assert_eq!(default_name(), "PLAYER");
    }
}
