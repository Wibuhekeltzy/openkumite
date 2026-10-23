//! Demos: a whole session recorded as its inputs, like the demo files of PC action games. The game is
//! deterministic given its seed, so the seed, the launch arguments, every fight step's pads and every menu key
//! (with the logic step it arrived before) replay the session exactly.
//!
//! Debug cheats (K.O., stage jumps) are recorded as keys too. Rewinding or loading a saved state cannot be
//! replayed, so it ends the recording.

use crate::menu::Key;
use anyhow::{Context, Result};
use kumite_sim::input::Pad;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What a demo records besides the pads: a menu key for a player, or a debug cheat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Key(usize, Key),
    Cheat(Cheat),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cheat {
    /// K.O. this player (the other wins the round).
    Ko(usize),
    /// P1 wins the match: its wins are set one short of the target, then P2 is K.O.'d.
    WinMatch,
    /// Arcade from this ladder stage (0..9).
    ArcadeStage(usize),
    /// Health cheat: 0 off, 1 god mode (P1), 2 both fighters.
    Health(u8),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DemoFile {
    pub version: u32,
    pub seed: u32,
    /// The launch arguments (without the program name and the demo options).
    pub args: Vec<String>,
    /// Fight-step pads, run-length coded: [first logic step, count, P1 bits, P2 bits].
    pub pads: Vec<[u64; 4]>,
    /// Events as [logic step, "p:key" / "cheat:..."].
    pub events: Vec<(u64, String)>,
}

pub fn pad_bits(p: Pad) -> u64 {
    u64::from(p.up) | u64::from(p.down) << 1 | u64::from(p.left) << 2 | u64::from(p.right) << 3 | (p.buttons as u64) << 4
}

pub fn bits_pad(b: u64) -> Pad {
    Pad { up: b & 1 != 0, down: b & 2 != 0, left: b & 4 != 0, right: b & 8 != 0, buttons: (b >> 4) as _, start: false }
}

fn key_name(k: Key) -> String {
    match k {
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Left => "left".into(),
        Key::Right => "right".into(),
        Key::Start => "start".into(),
        Key::Back => "back".into(),
        Key::Button(b) => format!("b{}", b + 1),
    }
}

fn name_key(s: &str) -> Option<Key> {
    Some(match s {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "start" => Key::Start,
        "back" => Key::Back,
        b if b.starts_with('b') => Key::Button(b[1..].parse::<usize>().ok()?.checked_sub(1)?),
        _ => return None,
    })
}

fn encode(e: &Event) -> String {
    match e {
        Event::Key(p, k) => format!("{p}:{}", key_name(*k)),
        Event::Cheat(Cheat::Ko(p)) => format!("cheat:ko{p}"),
        Event::Cheat(Cheat::WinMatch) => "cheat:win".into(),
        Event::Cheat(Cheat::ArcadeStage(n)) => format!("cheat:stage{n}"),
        Event::Cheat(Cheat::Health(m)) => format!("cheat:health{m}"),
    }
}

fn decode(s: &str) -> Option<Event> {
    let (a, b) = s.split_once(':')?;
    if a == "cheat" {
        return Some(Event::Cheat(if b == "win" {
            Cheat::WinMatch
        } else if let Some(p) = b.strip_prefix("ko") {
            Cheat::Ko(p.parse().ok()?)
        } else if let Some(m) = b.strip_prefix("health") {
            Cheat::Health(m.parse().ok()?)
        } else {
            Cheat::ArcadeStage(b.strip_prefix("stage")?.parse().ok()?)
        }));
    }
    Some(Event::Key(a.parse().ok()?, name_key(b)?))
}

impl DemoFile {
    pub fn new(seed: u32, args: Vec<String>) -> Self {
        Self { version: 1, seed, args, pads: Vec::new(), events: Vec::new() }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = kumite_fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string(self)?).with_context(|| format!("writing {}", path.display()))
    }

    pub fn push_pads(&mut self, step: u64, pads: [Pad; 2]) {
        let (a, b) = (pad_bits(pads[0]), pad_bits(pads[1]));
        if let Some(last) = self.pads.last_mut() {
            if last[0] + last[1] == step && last[2] == a && last[3] == b {
                last[1] += 1;
                return;
            }
        }
        self.pads.push([step, 1, a, b]);
    }

    pub fn push_event(&mut self, step: u64, e: &Event) {
        self.events.push((step, encode(e)));
    }

    /// Logic steps covered.
    pub fn len(&self) -> u64 {
        let p = self.pads.last().map_or(0, |r| r[0] + r[1]);
        let e = self.events.last().map_or(0, |e| e.0 + 1);
        p.max(e)
    }
}

/// A demo being played back.
pub struct Playback {
    pub file: DemoFile,
    pad_run: usize,
    event: usize,
}

impl Playback {
    pub fn new(file: DemoFile) -> Self {
        Self { file, pad_run: 0, event: 0 }
    }

    /// The events recorded before logic step `step`.
    pub fn events(&mut self, step: u64) -> Vec<Event> {
        let mut out = Vec::new();
        while let Some((s, e)) = self.file.events.get(self.event) {
            if *s > step {
                break;
            }
            if let Some(e) = decode(e) {
                out.push(e);
            } else {
                log::warn!("demo: unknown event {e:?}");
            }
            self.event += 1;
        }
        out
    }

    /// The fight pads of logic step `step` (none recorded: neutral).
    pub fn pads(&mut self, step: u64) -> [Pad; 2] {
        while let Some(r) = self.file.pads.get(self.pad_run) {
            if r[0] + r[1] <= step {
                self.pad_run += 1;
                continue;
            }
            if r[0] <= step {
                return [bits_pad(r[2]), bits_pad(r[3])];
            }
            break;
        }
        [Pad::default(); 2]
    }

    pub fn finished(&self, step: u64) -> bool {
        step >= self.file.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_and_events_round_trip() {
        let mut d = DemoFile::new(7, vec!["--debug".into()]);
        let p = Pad { up: true, buttons: 0b1010, ..Pad::default() };
        for s in 10..20 {
            d.push_pads(s, [p, Pad::default()]);
        }
        d.push_pads(20, [Pad::default(), p]);
        d.push_event(3, &Event::Key(1, Key::Button(2)));
        d.push_event(25, &Event::Cheat(Cheat::ArcadeStage(9)));
        assert_eq!(d.pads.len(), 2);
        let mut pb = Playback::new(serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap());
        assert_eq!(pb.events(5), vec![Event::Key(1, Key::Button(2))]);
        assert_eq!(pb.pads(9), [Pad::default(); 2]);
        assert_eq!(pb.pads(15)[0], p);
        assert_eq!(pb.pads(20)[1], p);
        assert_eq!(pb.events(25), vec![Event::Cheat(Cheat::ArcadeStage(9))]);
        assert!(pb.finished(26) && !pb.finished(25));
    }
}
