//! The controls, kept in the save: each player's gamepad (OPTION > KEY CONFIGURATION, the original's screen:
//! the eight buttons L2 R2 L1 R1 triangle circle cross square each give one of 13 commands) and keyboard
//! (OPTION > KEYBOARD, OpenKumite's). A command is what the game reads: a direction, a set of the four buttons
//! (LP 1, RP 2, LK 3, RK 4; two at once as LP+RP and so on), a sidestep (STEP up / down), START or BACK. The
//! gamepads (native and in a browser) are read by their buttons' places, named as on the original pad.

use crate::menu::Key;
use kumite_sim::input::Pad;
use serde::{Deserialize, Serialize};
use winit::keyboard::KeyCode;

/// What an input does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Command {
    None,
    Up,
    Down,
    Left,
    Right,
    /// Buttons 1..4 as bits 0..3 (several: pressed together).
    Buttons(u8),
    /// A sidestep: a new press taps up (down); held, the directions rest (0x8002C154 / 0x8002C278).
    StepUp,
    StepDown,
    Start,
    Back,
}

impl Command {
    /// The commands a pad button takes, in KEY CONFIGURATION's order (labels 0x800EAED0, bits 0x800B94D0).
    pub const PAD: [Command; 13] = [
        Command::Buttons(1),
        Command::Buttons(2),
        Command::Buttons(4),
        Command::Buttons(8),
        Command::Buttons(3),
        Command::Buttons(12),
        Command::Buttons(5),
        Command::Buttons(10),
        Command::Buttons(9),
        Command::Buttons(6),
        Command::StepUp,
        Command::StepDown,
        Command::None,
    ];

    /// The game's name (`names`: screens.json `key_config.texts.commands`, in `PAD`'s order: the sidesteps with a
    /// gap for their arrow); another button set: its buttons' names joined by +; the directions and START / BACK
    /// (the keyboard's): OpenKumite's.
    pub fn label(self, names: &serde_json::Value) -> String {
        let name = |k: usize| names[k].as_str().unwrap_or_default().to_string();
        if let Some(k) = Command::PAD.iter().position(|&c| c == self) {
            return name(k);
        }
        match self {
            Command::Up => "UP".into(),
            Command::Down => "DOWN".into(),
            Command::Left => "LEFT".into(),
            Command::Right => "RIGHT".into(),
            Command::Buttons(b) => (0..4).filter(|i| b & (1 << i) != 0).map(name).collect::<Vec<_>>().join("+"),
            Command::Start => "START".into(),
            Command::Back => "BACK".into(),
            // (in PAD)
            Command::None | Command::StepUp | Command::StepDown => String::new(),
        }
    }

    /// Into a pad's state (directions, buttons).
    fn press(self, pad: &mut Pad) {
        match self {
            Command::Up => pad.up = true,
            Command::Down => pad.down = true,
            Command::Left => pad.left = true,
            Command::Right => pad.right = true,
            Command::Buttons(b) => pad.buttons |= b,
            _ => {}
        }
    }

    /// The menu key it is (a single button picks; combinations do nothing in menus).
    pub fn menu_key(self) -> Option<Key> {
        Some(match self {
            Command::Up => Key::Up,
            Command::Down => Key::Down,
            Command::Left => Key::Left,
            Command::Right => Key::Right,
            Command::Buttons(b) if b.count_ones() == 1 => Key::Button(b.trailing_zeros() as usize),
            Command::Start => Key::Start,
            Command::Back => Key::Back,
            _ => return None,
        })
    }
}

/// A gamepad's button, by its place (the original pad's names; the standard layout's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PadButton {
    /// Cross (A).
    South,
    /// Circle (B).
    East,
    /// Square (X).
    West,
    /// Triangle (Y).
    North,
    L1,
    R1,
    L2,
    R2,
    Select,
    Start,
    L3,
    R3,
    Up,
    Down,
    Left,
    Right,
    /// The PS / Home / Guide button.
    Home,
}

impl PadButton {
    /// The buttons KEY CONFIGURATION sets, in the game's order (raw pad bits 0..7); the others are fixed:
    /// SELECT back, START, the d-pad (0x800B94EC).
    pub const CONFIGURABLE: [PadButton; 8] = [PadButton::L2, PadButton::R2, PadButton::L1, PadButton::R1, PadButton::North, PadButton::East, PadButton::South, PadButton::West];
}

/// A pad button in the menus and the select screens: they read the raw pad, not KEY CONFIGURATION (0x800ADF08):
/// square, triangle, cross, circle as the keys 1..4 (the costumes), START, SELECT back, the d-pad.
pub fn raw_menu_key(b: PadButton) -> Option<Key> {
    Some(match b {
        PadButton::West => Key::Button(0),
        PadButton::North => Key::Button(1),
        PadButton::South => Key::Button(2),
        PadButton::East => Key::Button(3),
        PadButton::Start => Key::Start,
        PadButton::Select | PadButton::Home => Key::Back,
        PadButton::Up => Key::Up,
        PadButton::Down => Key::Down,
        PadButton::Left => Key::Left,
        PadButton::Right => Key::Right,
        _ => return None,
    })
}

/// Stick deflection that counts as a direction.
pub const DEAD_ZONE: f32 = 0.5;

/// One player's gamepad: the eight configurable buttons' commands (the d-pad and the left stick move, SELECT
/// goes back, START starts), and the vibration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PadLayout {
    pub buttons: Vec<(PadButton, Command)>,
    pub vibration: bool,
}

impl Default for PadLayout {
    /// The original's (0x800B94FC): square LP, triangle RP, cross LK, circle RK, the shoulders NO USE; no
    /// vibration.
    fn default() -> Self {
        use PadButton as B;
        let buttons = vec![
            (B::L2, Command::None),
            (B::R2, Command::None),
            (B::L1, Command::None),
            (B::R1, Command::None),
            (B::North, Command::Buttons(2)),
            (B::East, Command::Buttons(8)),
            (B::South, Command::Buttons(4)),
            (B::West, Command::Buttons(1)),
        ];
        Self { buttons, vibration: false }
    }
}

impl PadLayout {
    /// A button's command (the fixed ones: the d-pad's directions, SELECT / Home back, START).
    pub fn command(&self, b: PadButton) -> Command {
        match b {
            PadButton::Up => Command::Up,
            PadButton::Down => Command::Down,
            PadButton::Left => Command::Left,
            PadButton::Right => Command::Right,
            PadButton::Select | PadButton::Home => Command::Back,
            PadButton::Start => Command::Start,
            PadButton::L3 | PadButton::R3 => Command::None,
            b => self.buttons.iter().find(|(x, _)| *x == b).map_or(Command::None, |(_, c)| *c),
        }
    }

    pub fn set(&mut self, b: PadButton, c: Command) {
        match self.buttons.iter_mut().find(|(x, _)| *x == b) {
            Some(slot) => slot.1 = c,
            None => self.buttons.push((b, c)),
        }
    }

    /// The pad's state from the buttons held and the left stick (y up positive), and the sidesteps held (bit 0
    /// up, bit 1 down; [`Step`] makes them taps).
    pub fn pad(&self, held: impl Iterator<Item = PadButton>, stick: (f32, f32)) -> (Pad, u8) {
        let mut pad = Pad::default();
        let mut steps = 0;
        for b in held {
            match self.command(b) {
                Command::StepUp => steps |= 1,
                Command::StepDown => steps |= 2,
                c => c.press(&mut pad),
            }
        }
        pad.up |= stick.1 > DEAD_ZONE;
        pad.down |= stick.1 < -DEAD_ZONE;
        pad.left |= stick.0 < -DEAD_ZONE;
        pad.right |= stick.0 > DEAD_ZONE;
        (pad, steps)
    }
}

/// The sidestep buttons (STEP up / down) as the game reads them: a new press is a tap of up (down); while held
/// with no attack button the directions rest (0x8002C154 / 0x8002C278).
#[derive(Clone, Copy, Debug, Default)]
pub struct Step {
    held: u8,
}

impl Step {
    pub fn apply(&mut self, mut pad: Pad, steps: u8) -> Pad {
        let new = steps & !self.held;
        self.held = steps;
        if steps != 0 && pad.buttons == 0 {
            pad.up = false;
            pad.down = false;
            pad.left = false;
            pad.right = false;
        }
        if new & 1 != 0 {
            pad.up = true;
        }
        if new & 2 != 0 {
            pad.down = true;
        }
        pad
    }
}

/// The keyboard's commands, in KEY CONFIG's order.
pub const KEY_COMMANDS: [Command; 15] = [
    Command::Up,
    Command::Down,
    Command::Left,
    Command::Right,
    Command::Buttons(1),
    Command::Buttons(2),
    Command::Buttons(4),
    Command::Buttons(8),
    Command::Buttons(3),
    Command::Buttons(12),
    Command::Buttons(5),
    Command::Buttons(10),
    Command::Buttons(9),
    Command::Buttons(6),
    Command::Start,
];

/// How many keys a command can have.
pub const KEYS_PER_COMMAND: usize = 2;

/// One player's keyboard: each command's keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyLayout {
    pub keys: Vec<(Command, Vec<KeyCode>)>,
}

impl KeyLayout {
    fn new(keys: &[(Command, &[KeyCode])]) -> Self {
        Self { keys: keys.iter().map(|(c, k)| (*c, k.to_vec())).collect() }
    }

    /// P1: WASD, F G V B (and U I J K), Space start.
    pub fn p1() -> Self {
        use KeyCode as K;
        Self::new(&[
            (Command::Up, &[K::KeyW]),
            (Command::Down, &[K::KeyS]),
            (Command::Left, &[K::KeyA]),
            (Command::Right, &[K::KeyD]),
            (Command::Buttons(1), &[K::KeyF, K::KeyU]),
            (Command::Buttons(2), &[K::KeyG, K::KeyI]),
            (Command::Buttons(4), &[K::KeyV, K::KeyJ]),
            (Command::Buttons(8), &[K::KeyB, K::KeyK]),
            (Command::Buttons(3), &[]),
            (Command::Buttons(12), &[]),
            (Command::Buttons(5), &[]),
            (Command::Buttons(10), &[]),
            (Command::Buttons(9), &[]),
            (Command::Buttons(6), &[]),
            (Command::Start, &[K::Space]),
        ])
    }

    /// P2: the arrows, ; ' . / (numpad 4 5 1 2), numpad Enter / right Shift start (Enter is always P1's).
    pub fn p2() -> Self {
        use KeyCode as K;
        Self::new(&[
            (Command::Up, &[K::ArrowUp]),
            (Command::Down, &[K::ArrowDown]),
            (Command::Left, &[K::ArrowLeft]),
            (Command::Right, &[K::ArrowRight]),
            (Command::Buttons(1), &[K::Semicolon, K::Numpad4]),
            (Command::Buttons(2), &[K::Quote, K::Numpad5]),
            (Command::Buttons(4), &[K::Period, K::Numpad1]),
            (Command::Buttons(8), &[K::Slash, K::Numpad2]),
            (Command::Buttons(3), &[]),
            (Command::Buttons(12), &[]),
            (Command::Buttons(5), &[]),
            (Command::Buttons(10), &[]),
            (Command::Buttons(9), &[]),
            (Command::Buttons(6), &[]),
            (Command::Start, &[K::NumpadEnter, K::ShiftRight]),
        ])
    }

    pub fn keys_of(&self, c: Command) -> &[KeyCode] {
        self.keys.iter().find(|(x, _)| *x == c).map_or(&[], |(_, k)| k.as_slice())
    }

    /// The commands a key gives.
    pub fn commands_of(&self, key: KeyCode) -> impl Iterator<Item = Command> + '_ {
        self.keys.iter().filter(move |(_, k)| k.contains(&key)).map(|(c, _)| *c)
    }

    /// The pad's state with the keys `down`.
    pub fn pad(&self, down: impl Fn(KeyCode) -> bool) -> Pad {
        let mut pad = Pad::default();
        for (c, keys) in &self.keys {
            if keys.iter().any(|&k| down(k)) {
                c.press(&mut pad);
            }
        }
        pad
    }

    /// `key` for command `c` (slot 0 or 1): taken from wherever else it was in this layout.
    pub fn bind(&mut self, c: Command, slot: usize, key: KeyCode) {
        for (_, keys) in self.keys.iter_mut() {
            keys.retain(|&k| k != key);
        }
        if let Some((_, keys)) = self.keys.iter_mut().find(|(x, _)| *x == c) {
            if slot < keys.len() {
                keys[slot] = key;
            } else {
                keys.push(key);
            }
            keys.truncate(KEYS_PER_COMMAND);
        } else {
            self.keys.push((c, vec![key]));
        }
    }

    /// Command `c` without keys.
    #[cfg(test)]
    pub fn clear(&mut self, c: Command) {
        if let Some((_, keys)) = self.keys.iter_mut().find(|(x, _)| *x == c) {
            keys.clear();
        }
    }

    /// Every command of KEY CONFIG present (a save from before a command was added).
    fn complete(&mut self, defaults: &KeyLayout) {
        for c in KEY_COMMANDS {
            if !self.keys.iter().any(|(x, _)| *x == c) {
                self.keys.push((c, defaults.keys_of(c).to_vec()));
            }
        }
    }
}

/// Keys that always work in menus whatever the layouts: Enter starts, Esc / Backspace go back, the arrows move
/// (when no layout has them).
pub fn system_key(code: KeyCode) -> Option<Key> {
    match code {
        KeyCode::Enter => Some(Key::Start),
        KeyCode::Escape | KeyCode::Backspace => Some(Key::Back),
        KeyCode::ArrowUp => Some(Key::Up),
        KeyCode::ArrowDown => Some(Key::Down),
        KeyCode::ArrowLeft => Some(Key::Left),
        KeyCode::ArrowRight => Some(Key::Right),
        _ => None,
    }
}

/// Keys KEY CONFIG does not take (the system's and OpenKumite's tools: function keys, M mute, the debug keys).
pub fn reserved(code: KeyCode) -> bool {
    use KeyCode as K;
    matches!(code, K::Escape | K::Backspace | K::Enter | K::F1 | K::F2 | K::F3 | K::F4 | K::F5 | K::F6 | K::F7 | K::F8 | K::F9 | K::F10 | K::F11 | K::F12 | K::KeyM)
}

/// A key's name as KEY CONFIG shows it.
pub fn key_label(code: KeyCode) -> String {
    use KeyCode as K;
    let s = match code {
        K::ArrowUp => "UP",
        K::ArrowDown => "DOWN",
        K::ArrowLeft => "LEFT",
        K::ArrowRight => "RIGHT",
        K::Space => "SPACE",
        K::Enter => "ENTER",
        K::NumpadEnter => "NUM ENTER",
        K::Semicolon => ";",
        K::Quote => "'",
        K::Comma => ",",
        K::Period => ".",
        K::Slash => "/",
        K::Backslash => "\\",
        K::BracketLeft => "[",
        K::BracketRight => "]",
        K::Minus => "-",
        K::Equal => "=",
        K::Backquote => "`",
        K::ShiftLeft => "L SHIFT",
        K::ShiftRight => "R SHIFT",
        K::ControlLeft => "L CTRL",
        K::ControlRight => "R CTRL",
        K::AltLeft => "L ALT",
        K::AltRight => "R ALT",
        K::Tab => "TAB",
        _ => {
            let name = format!("{code:?}");
            let name = name.strip_prefix("Key").or_else(|| name.strip_prefix("Digit")).map(str::to_string).unwrap_or_else(|| name.replace("Numpad", "NUM "));
            return name.to_uppercase();
        }
    };
    s.to_string()
}

/// The controls kept in the save.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Controls {
    /// P1's and P2's keyboard (one player alone has both).
    pub keyboard: [KeyLayout; 2],
    /// P1's and P2's gamepads (the tag brawl's P3 and P4 as P1 and P2).
    pub pads: [PadLayout; 2],
}

impl Default for Controls {
    fn default() -> Self {
        Self { keyboard: [KeyLayout::p1(), KeyLayout::p2()], pads: Default::default() }
    }
}

impl Controls {
    /// After loading: every command present.
    pub fn complete(mut self) -> Self {
        let defaults = Controls::default();
        for (k, d) in self.keyboard.iter_mut().zip(&defaults.keyboard) {
            k.complete(d);
        }
        for p in self.pads.iter_mut() {
            for b in PadButton::CONFIGURABLE {
                if !p.buttons.iter().any(|(x, _)| *x == b) {
                    p.buttons.push((b, PadLayout::default().command(b)));
                }
            }
        }
        self
    }

    /// Player `p`'s gamepad layout (P3 / P4 as P1 / P2).
    pub fn pad(&self, p: usize) -> &PadLayout {
        &self.pads[p % 2]
    }

    /// The keyboard's pads: one player alone plays with both layouts, two with one each.
    pub fn keyboard_pads(&self, two_players: bool, down: impl Fn(KeyCode) -> bool) -> [Pad; 2] {
        let [a, b] = [self.keyboard[0].pad(&down), self.keyboard[1].pad(&down)];
        if two_players {
            [a, b]
        } else {
            [merge(a, b), Pad::default()]
        }
    }

    /// A key in menus: the player whose layout has it (P2's only when it is not also P1's), and the menu key.
    pub fn menu_key(&self, code: KeyCode) -> Option<(usize, Key)> {
        for (player, layout) in self.keyboard.iter().enumerate() {
            if let Some(k) = layout.commands_of(code).find_map(Command::menu_key) {
                return Some((player, k));
            }
        }
        system_key(code).map(|k| (0, k))
    }
}

/// Both pads' bits together.
pub fn merge(a: Pad, b: Pad) -> Pad {
    Pad { up: a.up || b.up, down: a.down || b.down, left: a.left || b.left, right: a.right || b.right, buttons: a.buttons | b.buttons, start: a.start || b.start }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyboard_layouts() {
        let c = Controls::default();
        let down = |k: KeyCode| matches!(k, KeyCode::KeyW | KeyCode::KeyU | KeyCode::ArrowLeft | KeyCode::Slash);
        // alone: both layouts play P1
        let [p, q] = c.keyboard_pads(false, down);
        assert!(p.up && p.left && p.buttons == 0b1001 && q == Pad::default());
        // two players: one each
        let [p1, p2] = c.keyboard_pads(true, down);
        assert!(p1.up && !p1.left && p1.buttons == 1);
        assert!(p2.left && p2.buttons == 8);
        assert_eq!(c.menu_key(KeyCode::ArrowDown), Some((1, Key::Down)));
        assert_eq!(c.menu_key(KeyCode::KeyF), Some((0, Key::Button(0))));
        assert_eq!(c.menu_key(KeyCode::Escape), Some((0, Key::Back)));
    }

    #[test]
    fn binding_moves_a_key() {
        let mut k = KeyLayout::p1();
        // U to 1+2: no longer button 1's
        k.bind(Command::Buttons(3), 0, KeyCode::KeyU);
        assert_eq!(k.keys_of(Command::Buttons(1)), &[KeyCode::KeyF]);
        assert_eq!(k.pad(|x| x == KeyCode::KeyU).buttons, 3);
        k.bind(Command::Buttons(1), 1, KeyCode::KeyH);
        assert_eq!(k.keys_of(Command::Buttons(1)), &[KeyCode::KeyF, KeyCode::KeyH]);
        k.clear(Command::Up);
        assert!(!k.pad(|x| x == KeyCode::KeyW).up);
    }

    #[test]
    fn pad_layout() {
        let mut l = PadLayout::default();
        let (pad, _) = l.pad([PadButton::West, PadButton::East, PadButton::Down].into_iter(), (0.0, 0.0));
        assert_eq!((pad.buttons, pad.down), (0b1001, true), "square + circle = LP+RK, d-pad down");
        let (stick, _) = l.pad(std::iter::empty(), (0.9, -0.8));
        assert!(stick.right && stick.down && !stick.left && !stick.up, "stick down-right");
        assert_eq!(l.command(PadButton::L1), Command::None, "the shoulders: NO USE");
        l.set(PadButton::R1, Command::Buttons(3));
        assert_eq!(l.pad([PadButton::R1].into_iter(), (0.0, 0.0)).0.buttons, 3, "R1 = LP+RP");
        assert_eq!(l.command(PadButton::South).menu_key(), Some(Key::Button(2)), "cross = LK picks costume 3");
        assert_eq!(l.command(PadButton::Select).menu_key(), Some(Key::Back));
        assert_eq!(Command::Buttons(3).menu_key(), None);
        let names = serde_json::json!(["A", "B", "C", "D", "AB", "CD", "AC", "BD", "AD", "CB", " S", " S", "N"]);
        assert_eq!(Command::Buttons(6).label(&names), "CB");
        assert_eq!(Command::Buttons(7).label(&names), "A+B+C");
        assert_eq!(Command::None.label(&names), "N");
        assert_eq!(Command::PAD.len(), 13);
        // the save keeps the layout
        let json = serde_json::to_string(&Controls::default()).unwrap();
        assert_eq!(serde_json::from_str::<Controls>(&json).unwrap(), Controls::default());
    }

    #[test]
    fn sidestep_buttons_tap() {
        let mut l = PadLayout::default();
        l.set(PadButton::L1, Command::StepUp);
        let mut step = Step::default();
        let held = |b: &[PadButton]| l.pad(b.iter().copied(), (0.0, 0.0));
        let (p, s) = held(&[PadButton::L1, PadButton::Left]);
        let first = step.apply(p, s);
        assert!(first.up && !first.left, "a new press taps up, the directions rest");
        let (p, s) = held(&[PadButton::L1, PadButton::Left]);
        assert_eq!(step.apply(p, s), Pad::default(), "held: nothing more");
        let (p, s) = held(&[PadButton::L1, PadButton::West, PadButton::Left]);
        let attack = step.apply(p, s);
        assert!(attack.left && attack.buttons == 1, "with an attack button the directions count");
    }
}
