//! TAG BRAWL, OpenKumite's own mode (not in the original): all four fighters on the stage at once, each played by a
//! human (1P..4P: the keyboard's two layouts and up to four gamepads) or the CPU, in one of three ways: each for
//! themselves, two on two, or two together through an arcade ladder of ten pairs (the last two: the extra fighters'
//! bosses and the original's). Its setup screen picks each slot's character, costume and controller; a match is best of
//! three.

use crate::art::{Art, Screen as View};
use crate::hud::tag::{TagHud, TagInfo};
use crate::menu::{sfx, Key, Outcome, Roster};
use egui::Color32;
use kumite_game::bout::Announcement;
use kumite_game::tag::TagGame;

/// Each slot's panel centre (team 1 left, team 2 right).
const COLUMNS: [f32; 4] = [46.0, 138.0, 230.0, 322.0];
/// The face grid's box (x, y, width, height) between the VS and the hints, and a face cell (36 x 62 at full size).
const GRID_BOX: [f32; 4] = [8.0, 226.0, 352.0, 196.0];
const CELL_W: f32 = 36.0;
const CELL_H: f32 = 62.0;

/// Columns and face scale that fit `n` faces in [`GRID_BOX`] as large as they go (at most full size, 7 a row
/// when that fits).
fn grid_layout(n: usize) -> (usize, f32) {
    let [_, _, w, h] = GRID_BOX;
    (1..=n.max(1))
        .map(|cols| (cols, (w / (cols as f32 * CELL_W)).min(h / (n.div_ceil(cols).max(1) as f32 * CELL_H)).min(1.0)))
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.abs_diff(7).cmp(&a.0.abs_diff(7))))
        .unwrap_or((7, 1.0))
}

/// One slot as chosen: roster index, costume index, controller (None = CPU, Some(p) = player p).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SlotPick {
    pub character: usize,
    pub costume: usize,
    pub player: Option<usize>,
}

/// How the four fight.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TagMode {
    /// Each for themselves: the last one standing takes the round.
    Free,
    /// Two on two.
    #[default]
    Teams,
    /// Two together against ten CPU pairs in turn.
    Arcade,
    /// Ball 2 ON 2's setup (BALL's own list leads here; the ball is chosen after START).
    Ball,
    /// Force online: each player's fighter (a slot nobody plays stays out).
    Force,
}

impl TagMode {
    pub const ALL: [TagMode; 3] = [TagMode::Free, TagMode::Teams, TagMode::Arcade];

    /// Its title (the Ball's and the Force's names: screens.json `menu`).
    pub fn name(self, screens: &serde_json::Value) -> String {
        match self {
            TagMode::Free => "FREE FOR ALL".into(),
            TagMode::Teams => "TWO ON TWO".into(),
            TagMode::Arcade => "TAG ARCADE".into(),
            TagMode::Ball => format!("{} 2 ON 2", screens["menu"]["ball"].as_str().unwrap_or_default()),
            TagMode::Force => screens["menu"]["force"].as_str().unwrap_or_default().into(),
        }
    }

    /// Each slot's team.
    pub fn teams(self) -> Vec<usize> {
        match self {
            TagMode::Free => vec![0, 1, 2, 3],
            TagMode::Force => vec![0; 4],
            _ => vec![0, 0, 1, 1],
        }
    }
}

/// Pairs before the bosses' (ai.json `arcade.boss_pairs`).
const ARCADE_PAIRS: usize = 8;

/// TAG ARCADE's ladder at its longest: the drawn pairs, then the bosses'.
pub fn arcade_ladder_len(roster: &Roster) -> usize {
    ARCADE_PAIRS + roster.arcade.boss_pairs.len()
}

/// The arcade run: the CPU pairs (roster index, costume) in order and the one fought now.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TagArcade {
    pub ladder: Vec<[(usize, usize); 2]>,
    pub level: usize,
}

impl TagArcade {
    /// Eight pairs drawn from everyone but the bosses (the one the last boss becomes too) and the players' own
    /// characters, then the bosses; a CPU in the costume a player has takes another.
    pub fn new(roster: &Roster, players: [(usize, usize); 2], seed: &mut u32) -> Self {
        let bosses = roster.arcade.bosses();
        let mut pool: Vec<usize> = (0..roster.characters.len()).filter(|&i| !bosses.contains(&roster.characters[i].id) && !players.iter().any(|p| p.0 == i)).collect();
        let mut drawn = Vec::new();
        while drawn.len() < 2 * ARCADE_PAIRS && !pool.is_empty() {
            drawn.push(pool.remove(crate::menu::lcg(seed, pool.len())));
        }
        let fit = |c: usize| {
            let costumes = roster.characters[c].costumes.len();
            (c, crate::menu::free_costume(costumes, 0, |k| players.contains(&(c, k))).unwrap_or(0))
        };
        let mut ladder: Vec<[(usize, usize); 2]> = drawn.as_chunks::<2>().0.iter().map(|p| [fit(p[0]), fit(p[1])]).collect();
        for &[a, b] in &roster.arcade.boss_pairs {
            if let (Some(a), Some(b)) = (roster.index_of(a), roster.index_of(b)) {
                ladder.push([fit(a), fit(b)]);
            }
        }
        Self { ladder, level: 0 }
    }
}

/// What the setup screen hands to the app.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TagSetup {
    pub slots: [SlotPick; 4],
    pub difficulty: usize,
    pub mode: TagMode,
    /// The arcade run (TAG ARCADE).
    pub arcade: Option<TagArcade>,
}

impl TagSetup {
    /// The four of this match: the arcade's CPU pair in slots 3, 4.
    pub fn match_slots(&self) -> [SlotPick; 4] {
        let mut s = self.slots;
        if let Some(pair) = self.arcade.as_ref().and_then(|a| a.ladder.get(a.level)) {
            for k in 0..2 {
                s[2 + k] = SlotPick { character: pair[k].0, costume: pair[k].1, player: None };
            }
        }
        s
    }
}

/// Where the cursor is: on a slot's panel (the top row) or a face of the grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cursor {
    Slot(usize),
    Face(usize),
}

/// The setup screen: the four slots on top, the faces (VS MODE's grid order) below. 1P's cursor moves over both
/// with the arrows: a button on a face puts that character (the button is the costume, as on the select screens)
/// into the current slot and moves on to the next; a button on a slot makes it the current one and switches who
/// plays it (CPU, 1P..4P).
///
/// The other players (2P..4P: the keyboard's second layout, gamepads 2..4) join with START: each takes a CPU slot
/// (TWO ON TWO: on the team with fewer players) and gets a cursor of its own, which picks for its slot only; a
/// button on another CPU slot moves there (the other team); BACK leaves.
pub struct TagSelect {
    pub slots: [SlotPick; 4],
    pub difficulty: usize,
    pub mode: TagMode,
    seed: u32,
    /// The slot the next pick fills.
    current: usize,
    cursor: Cursor,
    /// Roster indices in the grid's order.
    faces: Vec<usize>,
    frame: u32,
    /// The cursors of the players who joined (2P..4P; index = player).
    guests: [Option<Cursor>; 4],
    /// Ball 2 ON 2: the ball type, and whether 1P is choosing it (after START).
    pub ball_kind: usize,
    choosing_ball: bool,
}

impl TagSelect {
    pub fn new(roster: &Roster, difficulty: usize, seed: u32, mode: TagMode) -> Self {
        let n = roster.characters.len().max(1);
        let pick = |k: usize, player| SlotPick { character: (seed as usize / 7 + k * 5) % n, costume: 0, player };
        // (the arcade: 1P and a partner, the CPU's until another player joins with START)
        let mut slots = [pick(0, Some(0)), pick(1, None), pick(2, None), pick(3, None)];
        // four different models: a slot meeting an earlier one takes the next costume, or the next character (a
        // roster with fewer than four models keeps a repeat)
        for k in 1..4 {
            let mut tries = n * 4;
            while tries > 0 && (0..k).any(|i| (slots[i].character, slots[i].costume) == (slots[k].character, slots[k].costume)) {
                let costumes = roster.characters.get(slots[k].character).map_or(1, |c| c.costumes.len().max(1));
                if slots[k].costume + 1 < costumes {
                    slots[k].costume += 1;
                } else {
                    (slots[k].character, slots[k].costume) = ((slots[k].character + 1) % n, 0);
                }
                tries -= 1;
            }
        }
        let mut s = Self::with(TagSetup { slots, difficulty, mode, arcade: None }, roster);
        s.seed = seed;
        s
    }

    /// The setup again with the last match's picks.
    pub fn with(setup: TagSetup, roster: &Roster) -> Self {
        let mut faces: Vec<usize> = roster.grid.cell_ids().into_iter().filter_map(|id| roster.index_of(id)).collect();
        for i in 0..roster.characters.len() {
            if !faces.contains(&i) {
                faces.push(i);
            }
        }
        let at = faces.iter().position(|&c| c == setup.slots[0].character).unwrap_or(0);
        Self {
            slots: setup.slots,
            difficulty: setup.difficulty,
            mode: setup.mode,
            seed: setup.slots[0].character as u32 * 31 + 7,
            current: 0,
            cursor: Cursor::Face(at),
            faces,
            frame: 0,
            guests: [None; 4],
            ball_kind: 0,
            choosing_ball: false,
        }
    }

    /// The slots the players fill (the arcade's CPU pairs come from its ladder).
    fn picked(&self) -> usize {
        if self.mode == TagMode::Arcade {
            2
        } else {
            4
        }
    }

    pub fn tick(&mut self) {
        self.frame += 1;
    }

    fn cols(&self) -> usize {
        grid_layout(self.faces.len()).0
    }

    fn rows(&self) -> usize {
        self.faces.len().div_ceil(self.cols())
    }

    /// Where face `f` sits (x, y) and the faces' scale.
    fn face_place(&self, f: usize) -> (f32, f32, f32) {
        let (cols, scale) = grid_layout(self.faces.len());
        let [bx, by, bw, _] = GRID_BOX;
        let (cw, ch) = (CELL_W * scale, CELL_H * scale);
        let x0 = bx + (bw - cw * cols.min(self.faces.len()) as f32) / 2.0 + 2.0 * scale;
        (x0 + cw * (f % cols) as f32, by + ch * (f / cols) as f32, scale)
    }

    /// Move the cursor (the slot row sits above the grid; its four panels spread over the grid's width).
    fn step(&mut self, key: Key) {
        self.cursor = self.moved(self.cursor, key);
    }

    fn moved(&self, cursor: Cursor, key: Key) -> Cursor {
        let n = self.faces.len();
        let cols = self.cols();
        let k = self.picked();
        let slot_of = |f: usize| ((f % cols) * 4 / cols).min(k - 1);
        let face_of = |s: usize, row: usize| (row * cols + (s * cols + cols / 2) / 4).min(n - 1);
        match (cursor, key) {
            (Cursor::Slot(s), Key::Left) => Cursor::Slot((s + k - 1) % k),
            (Cursor::Slot(s), Key::Right) => Cursor::Slot((s + 1) % k),
            (Cursor::Slot(s), Key::Down) => Cursor::Face(face_of(s, 0)),
            (Cursor::Slot(s), Key::Up) => Cursor::Face(face_of(s, self.rows() - 1)),
            (Cursor::Face(f), Key::Left) => Cursor::Face(if f % cols == 0 { (f + cols - 1).min(n - 1) } else { f - 1 }),
            (Cursor::Face(f), Key::Right) => Cursor::Face(if f % cols == cols - 1 || f + 1 >= n { f - f % cols } else { f + 1 }),
            (Cursor::Face(f), Key::Up) if f < cols => Cursor::Slot(slot_of(f)),
            (Cursor::Face(f), Key::Up) => Cursor::Face(f - cols),
            (Cursor::Face(f), Key::Down) if f + cols >= n => Cursor::Slot(slot_of(f)),
            (Cursor::Face(f), Key::Down) => Cursor::Face(f + cols),
            (c, _) => c,
        }
    }

    /// The slot player `p` plays, if any.
    fn seat_of(&self, p: usize) -> Option<usize> {
        (0..self.picked()).find(|&k| self.slots[k].player == Some(p))
    }

    /// A key of 2P..4P: join with START, then a cursor of its own for its slot.
    fn guest_key(&mut self, p: usize, key: Key, roster: &Roster) {
        let Some(seat) = self.seat_of(p) else {
            if key != Key::Start {
                return;
            }
            // a CPU slot: TWO ON TWO the team with fewer players first
            let teams = self.mode.teams();
            let humans = |t: usize| (0..self.picked()).filter(|&k| teams[k] == t && self.slots[k].player.is_some()).count();
            let mut free: Vec<usize> = (0..self.picked()).filter(|&k| self.slots[k].player.is_none()).collect();
            free.sort_by_key(|&k| (humans(teams[k]), k));
            let Some(&k) = free.first() else {
                sfx::play(sfx::back());
                return;
            };
            self.slots[k].player = Some(p);
            let at = self.faces.iter().position(|&c| c == self.slots[k].character).unwrap_or(0);
            self.guests[p] = Some(Cursor::Face(at));
            sfx::play(sfx::join());
            return;
        };
        // (seated by 1P: a cursor on its slot's face)
        let at = self.faces.iter().position(|&c| c == self.slots[seat].character).unwrap_or(0);
        let cursor = *self.guests[p].get_or_insert(Cursor::Face(at));
        match key {
            Key::Left | Key::Right | Key::Up | Key::Down => {
                self.guests[p] = Some(self.moved(cursor, key));
                sfx::play(sfx::select_cursor());
            }
            Key::Button(b) => match cursor {
                // another CPU slot: move there (TWO ON TWO: the other team)
                Cursor::Slot(s) if s != seat && self.slots[s].player.is_none() => {
                    self.slots[s].player = Some(p);
                    self.slots[seat].player = None;
                    sfx::play(sfx::pick());
                }
                Cursor::Slot(_) => {}
                Cursor::Face(f) => {
                    let character = self.faces[f];
                    let costumes = roster.characters[character].costumes.len().max(1);
                    let want = roster.characters[character].costume_for(Key::Button(b)).unwrap_or(0);
                    let others: Vec<(usize, usize)> = (0..self.picked()).filter(|&i| i != seat).map(|i| (self.slots[i].character, self.slots[i].costume)).collect();
                    let Some(costume) = crate::menu::free_costume(costumes, want, |c| others.contains(&(character, c))) else {
                        sfx::play(sfx::back());
                        return;
                    };
                    (self.slots[seat].character, self.slots[seat].costume) = (character, costume);
                    sfx::play(sfx::pick());
                }
            },
            // leaving: the CPU takes the slot
            Key::Back => {
                self.slots[seat].player = None;
                self.guests[p] = None;
                sfx::play(sfx::back());
            }
            Key::Start => {}
        }
    }

    /// Who plays slot `s` next: CPU, 1P..4P, skipping the players already in another slot.
    fn next_player(&mut self, s: usize) {
        let used: Vec<usize> = (0..4).filter(|&k| k != s).filter_map(|k| self.slots[k].player).collect();
        let order = [None, Some(0), Some(1), Some(2), Some(3)];
        let mut at = order.iter().position(|&p| p == self.slots[s].player).unwrap_or(0);
        for _ in 0..order.len() {
            at = (at + 1) % order.len();
            if order[at].is_none_or(|p| !used.contains(&p)) {
                break;
            }
        }
        self.slots[s].player = order[at];
    }

    /// A key of `player` (0 = 1P).
    pub fn key(&mut self, player: usize, key: Key, roster: &Roster) -> Outcome {
        if player > 0 {
            self.guest_key(player.min(3), key, roster);
            return Outcome::None;
        }
        // Ball 2 ON 2: the ball after START (left / right), then the match
        if self.choosing_ball {
            let n = roster.ball_types();
            match key {
                Key::Left | Key::Right => {
                    self.ball_kind = if key == Key::Right { (self.ball_kind + 1) % n } else { (self.ball_kind + n - 1) % n };
                    sfx::play(sfx::select_cursor());
                }
                Key::Start | Key::Button(_) => {
                    sfx::play(sfx::pick());
                    self.choosing_ball = false;
                    return self.ball_fight(roster);
                }
                Key::Back => {
                    self.choosing_ball = false;
                    sfx::play(sfx::back());
                }
                _ => {}
            }
            return Outcome::None;
        }
        if key == Key::Start && self.mode == TagMode::Ball {
            self.choosing_ball = true;
            sfx::play(sfx::pick());
            return Outcome::None;
        }
        match key {
            Key::Left | Key::Right | Key::Up | Key::Down => {
                self.step(key);
                sfx::play(sfx::select_cursor());
            }
            Key::Button(b) => match self.cursor {
                Cursor::Slot(s) => {
                    if self.current == s {
                        self.next_player(s);
                    }
                    self.current = s;
                    sfx::play(sfx::select_cursor());
                }
                Cursor::Face(f) => {
                    let character = self.faces[f];
                    let costumes = roster.characters[character].costumes.len().max(1);
                    let b = roster.characters[character].costume_for(Key::Button(b)).unwrap_or(0);
                    // no model twice on screen: a costume no other slot has, or none (it cannot be played)
                    let cur = self.current;
                    let others: Vec<(usize, usize)> = (0..self.picked()).filter(|&i| i != cur).map(|i| (self.slots[i].character, self.slots[i].costume)).collect();
                    let Some(costume) = crate::menu::free_costume(costumes, b, |c| others.contains(&(character, c))) else {
                        sfx::play(sfx::back());
                        return Outcome::None;
                    };
                    let slot = &mut self.slots[self.current];
                    (slot.character, slot.costume) = (character, costume);
                    self.current = (self.current + 1) % self.picked();
                    sfx::play(sfx::pick());
                }
            },
            Key::Start => {
                sfx::play(sfx::pick());
                let arcade = (self.mode == TagMode::Arcade).then(|| {
                    let p = |k: usize| (self.slots[k].character, self.slots[k].costume);
                    TagArcade::new(roster, [p(0), p(1)], &mut self.seed)
                });
                return Outcome::StartTag(TagSetup { slots: self.slots, difficulty: self.difficulty, mode: self.mode, arcade });
            }
            Key::Back => {
                sfx::play(sfx::back());
                return Outcome::Quit;
            }
        }
        Outcome::None
    }

    /// Online, the setup's time is up: the match as it stands (the ball as chosen so far).
    pub fn finish(&mut self, roster: &Roster) -> Outcome {
        self.choosing_ball = false;
        if self.mode == TagMode::Ball {
            return self.ball_fight(roster);
        }
        self.key(0, Key::Start, roster)
    }

    /// Ball 2 ON 2's match: slots 1, 2 P1's side (its first, its partner), 3, 4 the other's.
    fn ball_fight(&self, roster: &Roster) -> Outcome {
        let p = |k: usize| (self.slots[k].character, self.slots[k].costume);
        let seats = [self.slots[0].player, self.slots[2].player, self.slots[3].player, self.slots[1].player];
        let level = (self.difficulty * 4 + 1).min(9);
        let setup = crate::menu::FightSetup {
            mode: crate::menu::Mode::Ball,
            players: [p(0), p(2)],
            stage: roster.ball_stage,
            difficulty: self.difficulty,
            level,
            handicap: [100; 2],
            ball: Some((self.ball_kind, 0)),
            partners: Some(crate::menu::Doubles { partners: [p(1), p(3)], seats }),
        };
        Outcome::Fight(setup, Vec::new())
    }

    /// The setup again after a Ball 2 ON 2 match (its four, its seats, its ball).
    pub fn ball_again(roster: &Roster, setup: &crate::menu::FightSetup) -> Self {
        let d = setup.partners.unwrap_or(crate::menu::Doubles { partners: setup.players, seats: [Some(0), None, None, None] });
        let slot = |(character, costume): (usize, usize), player| SlotPick { character, costume, player };
        let slots = [slot(setup.players[0], d.seats[0]), slot(d.partners[0], d.seats[3]), slot(setup.players[1], d.seats[1]), slot(d.partners[1], d.seats[2])];
        let mut s = Self::with(TagSetup { slots, difficulty: setup.difficulty, mode: TagMode::Ball, arcade: None }, roster);
        s.ball_kind = setup.ball.map_or(0, |b| b.0);
        s
    }

    pub fn draw(&self, painter: &egui::Painter, v: &View, art: &Art, roster: &Roster) {
        // the two team halves, shaded in their colours
        let (blue, red) = (Color32::from_rgb(0x10, 0x20, 0x60), Color32::from_rgb(0x60, 0x10, 0x18));
        let centred = |font: usize, colour: u32, cx: f32, y: f32, t: &str| art.text(painter, v, font, colour, cx - art.text_w(font, t) / 2.0, y, t, Color32::WHITE);
        if self.mode == TagMode::Free {
            // each for themselves: a colour per fighter
            let colours = [blue, red, Color32::from_rgb(0x10, 0x48, 0x18), Color32::from_rgb(0x58, 0x48, 0x08)];
            for (k, c) in colours.into_iter().enumerate() {
                Art::shade(painter, v, [92.0 * k as f32, 0.0, 92.0, crate::art::SCREEN_H], [c, Color32::BLACK, c, Color32::BLACK]);
            }
            for (k, cx) in COLUMNS.iter().enumerate() {
                centred(0, 6, *cx, 32.0, &format!("SLOT {}", k + 1));
            }
        } else if self.mode == TagMode::Force {
            // Force online: all on one side
            Art::shade(painter, v, crate::art::FULL, [blue, Color32::BLACK, blue, Color32::BLACK]);
            centred(0, 6, crate::art::CENTRE, 32.0, "PLAYERS");
        } else {
            Art::shade(painter, v, [0.0, 0.0, crate::art::CENTRE, crate::art::SCREEN_H], [blue, Color32::BLACK, blue, Color32::BLACK]);
            Art::shade(painter, v, [crate::art::CENTRE, 0.0, crate::art::CENTRE, crate::art::SCREEN_H], [Color32::BLACK, red, Color32::BLACK, red]);
            let (a, b) = if self.mode == TagMode::Arcade { ("PLAYERS", "10 PAIRS") } else { ("TEAM 1", "TEAM 2") };
            centred(0, 6, 92.0, 32.0, a);
            centred(0, 6, 276.0, 32.0, b);
            if self.mode != TagMode::Ball {
                centred(1, 7, crate::art::CENTRE, 206.0, "VS");
            }
        }
        centred(1, 6, crate::art::CENTRE, 10.0, &self.mode.name(&art.screens));
        let blink = self.frame & 0x10 != 0;
        let white = |a: u8| Color32::from_rgba_unmultiplied(255, 255, 255, a);
        let frame = |x: f32, y: f32, w: f32, h: f32, c: Color32| {
            for r in [[x, y, w, 2.0], [x, y + h - 2.0, w, 2.0], [x, y, 2.0, h], [x + w - 2.0, y, 2.0, h]] {
                Art::shade(painter, v, r, [c; 4]);
            }
        };
        // the slots: face, name, style, who plays; the current one framed, the cursor's blinking
        for (k, s) in self.slots.iter().enumerate() {
            let cx = COLUMNS[k];
            // the arcade's CPU pairs: unknown until fought
            if k >= self.picked() {
                centred(1, 6, cx, 100.0, "?");
                centred(1, 6, cx, 170.0, "CPU");
                continue;
            }
            let c = &roster.characters[s.character];
            crate::modes::face_sized(painter, v, art, roster, c.id, cx - 19.0, 56.0, 0, 1.2);
            let model = c.costumes.get(s.costume).copied().unwrap_or(0);
            let cos = roster.costume_of.get(&model).copied().unwrap_or(0);
            let name = roster.ui.selection_names.get(c.id * 4 + cos).cloned().unwrap_or_else(|| c.name.to_uppercase());
            centred(0, 6, cx, 132.0, &name);
            centred(0, 6, cx, 150.0, &format!("STYLE {}", s.costume + 1));
            let nobody = if self.mode == TagMode::Force { "-" } else { "CPU" };
            let player = s.player.map_or(nobody.to_string(), |p| format!("{}P", p + 1));
            centred(1, if s.player.is_some() { 5 } else { 6 }, cx, 170.0, &player);
            if k == self.current {
                frame(cx - 45.0, 50.0, 90.0, 144.0, Color32::from_rgb(0xF0, 0xC0, 0x20));
            }
            if self.cursor == Cursor::Slot(k) && blink {
                frame(cx - 42.0, 53.0, 84.0, 138.0, white(255));
            }
        }
        // the faces
        for (f, &ch) in self.faces.iter().enumerate() {
            let (x, y, scale) = self.face_place(f);
            crate::modes::face_sized(painter, v, art, roster, roster.characters[ch].id, x, y, 0, scale);
            if self.cursor == Cursor::Face(f) {
                frame(x - 2.0 * scale, y - 2.0 * scale, CELL_W * scale, CELL_H * scale, if blink { white(255) } else { Color32::from_rgb(0xF0, 0xC0, 0x20) });
            }
        }
        // the other players' cursors, in their colours, with their number
        let colours = [Color32::from_rgb(0xF0, 0x40, 0x40), Color32::from_rgb(0x40, 0xE0, 0x60), Color32::from_rgb(0xF0, 0xE0, 0x40)];
        for (p, cursor) in self.guests.iter().enumerate().skip(1) {
            let (Some(cursor), Some(seat)) = (cursor, self.seat_of(p)) else { continue };
            let c = colours[(p - 1).min(2)];
            let c = if blink { c } else { c.gamma_multiply(0.6) };
            match *cursor {
                Cursor::Face(f) => {
                    let (x, y, scale) = self.face_place(f);
                    frame(x - 2.0 * scale + p as f32, y - 2.0 * scale + p as f32, CELL_W * scale - 2.0 * p as f32, CELL_H * scale - 2.0 * p as f32, c);
                    art.text(painter, v, 0, 6, x + 1.0, y + 1.0 + 9.0 * (p as f32 - 1.0), &format!("{}P", p + 1), Color32::WHITE);
                }
                Cursor::Slot(s) => {
                    let cx = COLUMNS[s];
                    frame(cx - 40.0 + p as f32, 55.0 + p as f32, 80.0 - 2.0 * p as f32, 134.0 - 2.0 * p as f32, c);
                }
            }
            let _ = seat;
        }
        // what the cursor does
        let hint = match self.cursor {
            Cursor::Face(f) => format!("{} -> SLOT {}   BUTTON 1-4: STYLE", roster.characters[self.faces[f]].name.to_uppercase(), self.current + 1),
            Cursor::Slot(s) if s == self.current => "BUTTON: CPU / 1P / 2P / 3P / 4P".to_string(),
            Cursor::Slot(_) => "BUTTON: PICK FOR THIS SLOT".to_string(),
        };
        centred(0, 7, crate::art::CENTRE, 430.0, &hint);
        // Ball 2 ON 2: the ball being chosen
        if self.mode == TagMode::Ball {
            let kind = roster.ball.as_ref().and_then(|b| b.types.get(self.ball_kind));
            let name = kind.map_or("BALL".to_string(), |k| format!("{} {}", k.level, k.name));
            let line = if self.choosing_ball { format!("< {name} >   START: PLAY") } else { name };
            centred(0, if self.choosing_ball && blink { 5 } else { 6 }, crate::art::CENTRE, 206.0, &line);
        }
        centred(0, 6, crate::art::CENTRE, 452.0, &format!("START FIGHT  2P-4P JOIN  CPU {}", crate::menu::DIFFICULTIES[self.difficulty.min(2)]));
        if self.frame < 16 {
            Art::shade(painter, v, crate::art::FULL, [Color32::from_black_alpha(255 - (self.frame * 16).min(255) as u8); 4]);
        }
    }
}

/// A tag match in progress.
pub struct TagRun {
    pub game: TagGame,
    pub setup: TagSetup,
    pub info: TagInfo,
    pub hud: TagHud,
    /// Sound banks ("m071") and voice rows (+0x1A) per actor.
    pub banks: [String; 4],
    pub characters: [usize; 4],
    pub sound: kumite_game::sound::SoundState,
    /// The pause menu: (cursor, player who paused).
    pub pause: Option<(usize, usize)>,
    /// What each actor's texture shows of its face animation.
    pub faces_shown: [(usize, kumite_game::face::Face); 4],
    /// TAG ARCADE lost: the continue count (frames left), then GAME OVER (frames shown).
    pub cont: Option<u32>,
    pub game_over: Option<u32>,
    /// START on the continue: the same pair again.
    pub retry: bool,
}

/// What the app does when a match has ended.
#[derive(Clone, Debug)]
pub enum TagNext {
    /// Back to the setup.
    Setup,
    /// TAG ARCADE: the next pair.
    Next(Box<TagSetup>),
    /// TAG ARCADE: all ten pairs beaten.
    Cleared,
    /// TAG ARCADE lost: the continue count.
    Continue,
}

impl TagRun {
    /// The CPU level of a difficulty (EASY / MEDIUM / HARD).
    pub fn cpu_level(difficulty: usize) -> (usize, usize) {
        (difficulty.min(2), [3, 6, 9][difficulty.min(2)])
    }

    pub fn new(game: TagGame, setup: TagSetup, roster: &Roster) -> Self {
        let models: [usize; 4] = std::array::from_fn(|i| game.bodies[i].model_record);
        let slots = setup.match_slots();
        let humans = (0..game.wins.len()).map(|t| (0..4).any(|i| game.teams[i] == t && slots[i].player.is_some())).collect();
        let info = TagInfo {
            models,
            costumes: models.map(|m| roster.costume_of.get(&m).copied().unwrap_or(0)),
            labels: slots.map(|s| s.player.map_or("CPU".to_string(), |p| format!("{}P", p + 1))),
            names: slots.map(|s| roster.characters[s.character].name.to_uppercase()),
            humans,
            stage: setup.arcade.as_ref().map(|a| (a.level + 1, a.ladder.len())),
        };
        Self {
            banks: models.map(|m| format!("m{m:03}")),
            characters: models.map(|m| roster.voice_of.get(&m).copied().unwrap_or(0)),
            game,
            setup,
            info,
            hud: TagHud::default(),
            sound: Default::default(),
            pause: None,
            faces_shown: Default::default(),
            cont: None,
            game_over: None,
            retry: false,
        }
    }

    /// An announcement and the humans it speaks to (the winners' team first: "you win" when it has a human).
    pub fn announcement(&self, e: Announcement) -> (Announcement, [bool; 2]) {
        match e {
            Announcement::Wins(w) => (Announcement::Wins(0), [self.info.humans.get(w).copied().unwrap_or(false), false]),
            e => (e, [false; 2]),
        }
    }

    /// The match is over: what comes next.
    pub fn next(&self) -> TagNext {
        let Some(a) = &self.setup.arcade else { return TagNext::Setup };
        let won = matches!(self.game.phase, kumite_game::tag::TagPhase::MatchOver { winner: Some(0) });
        if !won {
            return TagNext::Continue;
        }
        if a.level + 1 >= a.ladder.len() {
            return TagNext::Cleared;
        }
        let mut s = self.setup.clone();
        if let Some(a) = s.arcade.as_mut() {
            a.level += 1;
        }
        TagNext::Next(Box::new(s))
    }

    /// The round is on (a script's pads start then).
    pub fn phase_is_fight(&self) -> bool {
        self.game.phase == kumite_game::tag::TagPhase::Fight
    }

    /// A menu key (START / SELECT pause; the pause menu's CANCEL / RESET). Returns true to leave the match.
    pub fn key(&mut self, player: usize, key: Key) -> bool {
        // the continue count: START fights the pair again, a button hurries the count, SELECT gives up
        if let Some(c) = self.cont.as_mut() {
            match key {
                Key::Start => {
                    sfx::play(sfx::start());
                    self.retry = true;
                }
                Key::Button(_) => *c = c.saturating_sub(crate::menu::CONTINUE_STEP).max(1),
                Key::Back => {
                    self.cont = None;
                    self.game_over = Some(0);
                }
                _ => {}
            }
            return false;
        }
        if self.game_over.is_some() {
            return false;
        }
        match (&mut self.pause, key) {
            (None, Key::Back | Key::Start) => {
                sfx::play(sfx::pick());
                self.pause = Some((0, player));
            }
            (Some(_), Key::Back) => self.pause = None,
            (Some((c, _)), Key::Up | Key::Down) => {
                sfx::play(sfx::cursor());
                *c ^= 1;
            }
            (Some((c, _)), Key::Start | Key::Button(_)) => {
                sfx::play(sfx::pick());
                let leave = *c == 1;
                self.pause = None;
                return leave;
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Option<Roster> {
        let gd = crate::test_assets().join("gamedata");
        gd.exists().then(|| Roster::load(&gd))
    }

    #[test]
    fn faces_fill_the_slots_in_turn_and_slots_switch_controllers() {
        let Some(roster) = roster() else { return };
        let mut s = TagSelect::new(&roster, 1, 3, TagMode::Teams);
        assert_eq!(s.slots.map(|x| x.player), [Some(0), None, None, None]);
        assert_eq!(s.faces.len(), roster.characters.len(), "every character has a face");
        // four picks on the grid: right, pick (cross: costume 2), right, pick ... fill slots 1..4
        let mut picked = Vec::new();
        for k in 0..4 {
            s.key(0, Key::Right, &roster);
            let Cursor::Face(f) = s.cursor else { panic!("on the grid") };
            picked.push(s.faces[f]);
            s.key(0, Key::Button(if k == 0 { 2 } else { 0 }), &roster);
        }
        assert_eq!(s.slots.map(|x| x.character).to_vec(), picked);
        assert_eq!(s.slots[0].costume, 1);
        assert_eq!(s.current, 0, "back to the first slot");
        // up from the top row: slot 2's panel; a button makes it current, a second one makes it 2P (1P is taken)
        s.cursor = Cursor::Face(3);
        s.key(0, Key::Up, &roster);
        assert_eq!(s.cursor, Cursor::Slot(1));
        s.key(0, Key::Button(0), &roster);
        assert_eq!((s.current, s.slots[1].player), (1, None));
        s.key(0, Key::Button(0), &roster);
        assert_eq!(s.slots[1].player, Some(1));
        s.key(0, Key::Down, &roster);
        assert!(matches!(s.cursor, Cursor::Face(_)));
        assert!(matches!(s.key(0, Key::Start, &roster), Outcome::StartTag(t) if t.slots == s.slots && crate::room::MatchSetup::Tag(t.clone()).valid(&roster)));
        assert!(matches!(s.key(0, Key::Back, &roster), Outcome::Quit));
    }

    #[test]
    fn the_tag_arcade_is_ten_pairs_ending_with_the_bosses() {
        let Some(roster) = roster() else { return };
        if roster.arcade.boss_pairs.len() != 2 || roster.arcade.boss_pairs.iter().flatten().any(|&id| roster.index_of(id).is_none()) {
            return;
        }
        let players = [(0, 0), (1, 0)];
        let mut seed = 5;
        let a = TagArcade::new(&roster, players, &mut seed);
        assert_eq!(a.ladder.len(), 10);
        let slot = |c| SlotPick { character: c, costume: 0, player: None };
        let setup = TagSetup { slots: [slot(0), slot(1), slot(2), slot(3)], difficulty: 1, mode: TagMode::Arcade, arcade: Some(a.clone()) };
        assert!(crate::room::MatchSetup::Tag(setup.clone()).valid(&roster));
        let mut bad = setup;
        bad.arcade.as_mut().unwrap().level = 10;
        assert!(!crate::room::MatchSetup::Tag(bad).valid(&roster), "a level past the ladder");
        let ids = |k: usize| a.ladder[k].map(|(c, _)| roster.characters[c].id);
        assert_eq!(ids(8), roster.arcade.boss_pairs[0], "the extra fighters' bosses");
        assert_eq!(ids(9), roster.arcade.boss_pairs[1], "the original's");
        let first: Vec<usize> = a.ladder[..8].iter().flatten().map(|&(c, _)| c).collect();
        let mut unique = first.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 16, "no one twice");
        for c in first {
            assert!(![0, 1].contains(&c), "not the players' own characters");
            assert!(!roster.arcade.bosses().contains(&roster.characters[c].id), "no boss before the end");
        }
        // the setup: slots 3, 4 are the pair fought now, CPUs
        let mut setup = TagSetup { slots: TagSelect::new(&roster, 1, 1, TagMode::Arcade).slots, difficulty: 1, mode: TagMode::Arcade, arcade: Some(a) };
        setup.arcade.as_mut().unwrap().level = 9;
        let m = setup.match_slots();
        assert_eq!([m[2].character, m[3].character].map(|c| roster.characters[c].id), roster.arcade.boss_pairs[1]);
        assert!(m[2].player.is_none() && m[3].player.is_none());
        assert_eq!(setup.slots[1].player, None, "the partner is the CPU's until 2P joins");
    }
}
