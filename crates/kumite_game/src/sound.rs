//! Which sounds a fight step makes, from the converted data only: `gamedata/sound.json` (the EXE's move
//! sound-list code table, the SePlay code format, the character voice rows) and `audio/index.json` (which
//! WAV each VAB program/key plays). Playback is the app's job; this decides *what* plays, so it is testable.

use crate::bout::Bout;
use anyhow::{Context, Result};
use kumite_assets::Assets;
use kumite_sim::Current;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One sound to start: a WAV file and a volume (0..1).
#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub file: PathBuf,
    pub volume: f32,
}

struct Sample {
    program: u32,
    keys: (u32, u32),
    /// The tone's index in its program (what SePlay's code names, SsUtKeyOnV 0x800885D8), when the bank has them.
    tone: Option<u32>,
    file: PathBuf,
}

pub struct SoundData {
    /// Move sound-list index -> SePlay code (0x80097050).
    codes: Vec<u16>,
    /// Character voice rows: counts per category and codes.
    voices: Vec<([usize; 4], Vec<u16>)>,
    /// Per voice row, the second sound player's random attack voice (row `random`, 0x80073C50): category-0 voices to
    /// pick from and the voice lock's frames (0x80050CB8) that a voice sets and that holds off the next one.
    random_voices: Vec<Option<(Vec<usize>, u32)>>,
    /// Per voice row of the original's, the attack voice (row+4, 0x80041734): sound-list entries and the voice lock it
    /// sets.
    attack: Vec<Option<(Vec<u16>, u32)>>,
    /// Per voice row of the original's, the hurt voice (row+8, 0x80041600): the entries for a light hit and a heavy
    /// one.
    hurt: Vec<Option<(Vec<u16>, Vec<u16>)>>,
    /// Per voice row, where its own bank-2 sounds start in the sound-list table (row `system_start`: sound k is entry
    /// start + k, in the fighter's own bank): such a row plays by the second sound player (0x80073xxx).
    system_start: Vec<Option<usize>>,
    /// Such a row's own impact sounds, bank-2 sound indices (sound.json `system_impacts`): a K.O. by an arm and by a
    /// leg, a block.
    system_ko: [usize; 2],
    system_blocked: usize,
    /// The copying character's voice row (sound.json `copy_voice_remap.row`): its sounds play
    /// through the remap.
    copy_row: Option<usize>,
    /// The voice row that rings as metal when struck (sound.json `impact_rules.metal_row`, 0x80041B70).
    metal_row: Option<usize>,
    /// Per voice row, its own struck sounds (arm, leg) on its bank (0x800733B0).
    struck: Vec<Option<(u16, u16)>>,
    /// Type-9 entry 3's three sounds (0x8001AD4C).
    type9: Vec<u16>,
    /// That row's remap (0x80075548): a code it owns -> the code it plays as.
    copy_remap: Vec<(u16, u16)>,
    /// Timed sound scripts (move sound entry type 8, 0x800971F8): [cmd, frame, code].
    scripts: Vec<Vec<[u32; 3]>>,
    /// Bank key ("sys", "ann", "m071", ...) -> samples with the tones that play them.
    banks: HashMap<String, Vec<Sample>>,
    /// Music by BGM id (column 0, the arranged tracks) and the characters each belongs to.
    pub music: Vec<(PathBuf, bool, Vec<String>)>,
    /// The same BGM ids' column 1 (the arcade originals), as the Theater's music player plays them.
    pub music_arcade: Vec<(PathBuf, bool)>,
    /// Announcer calls: sound-table indices traced in the round code (sound.json `announcer`).
    announcer: serde_json::Value,
    /// The SePlay volumes (0x800278FC, sound.json `volumes`).
    volumes: Vec<u16>,
    /// The sounds the sound player's own code plays.
    impacts: Impacts,
    moves: MoveSounds,
}

/// The impacts' sound-list entries (sound.json `impact_rules`; type << 12 | table index): arm, leg where two.
#[derive(Clone, Debug, Default, Deserialize)]
struct Impacts {
    ko: [u16; 2],
    blocked: [u16; 2],
    wooden_block: [u16; 2],
    /// The entries that leave the impact to the chooser 0x80041CB4 (by limb and damage).
    auto: Vec<u16>,
    /// By the damage tier.
    legs: [u16; 4],
    arms: Arms,
    metal: Metal,
    wood: Wood,
}
/// An arm's: the lightest tier's two (by a rand bit), the others'.
#[derive(Clone, Debug, Default, Deserialize)]
struct Arms {
    light: [u16; 2],
    tiers: [u16; 3],
}
#[derive(Clone, Debug, Default, Deserialize)]
struct Metal {
    light: u16,
    ring: [u16; 2],
}
#[derive(Clone, Debug, Default, Deserialize)]
struct Wood {
    entries: Vec<u16>,
}
/// A jump's take-off, the landings, the attack's swing (sound.json `move_sounds`).
#[derive(Clone, Debug, Default, Deserialize)]
struct MoveSounds {
    take_off: u16,
    landing: u16,
    air_landing: u16,
    swing: u16,
}

/// A sound-list entry's table index.
fn index(entry: u16) -> usize {
    (entry & 0xFFF) as usize
}

/// Announcer calls outside the fight (continue screen).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    /// "Continue?" when the continue screen opens.
    ContinueStart,
    /// The countdown's spoken number (1..8).
    Count(u32),
    /// The end of the countdown (game over).
    ContinueZero,
}

/// Per-fight sound state: each fighter's last move and frame, and its running sound script (index, counter).
#[derive(Clone, Debug, Default)]
pub struct SoundState {
    pub last: Vec<Option<(Current, u32)>>,
    pub script: Vec<Option<(usize, u32)>>,
    /// rand() for the light arm impacts' pick (0x8007A318).
    pub seed: u32,
    /// Each fighter's voice lock (the original's 0x800971E8, the extra fighters' 0x800A1AF4): frames left before a
    /// voice may play again.
    pub voice_lock: Vec<u32>,
    /// Each fighter's hurt-voice cooldown (0x800971DC): 40 frames after a hurt voice.
    pub hurt_cooldown: Vec<u32>,
    /// A wooden copying character (0x800954A8: picked with down held as the fight loads): its blocks and the hits on it
    /// sound as wood.
    pub wooden: Vec<bool>,
}

/// The attacker's damage the impacts and voices weigh (+0x5E, set as the move starts, 0x8002F648): the move's,
/// none for a throw (damage type 1).
fn attack_damage(mv: &kumite_sim::data::Move) -> i32 {
    if mv.damage_type == 1 {
        0
    } else {
        mv.damage
    }
}

/// The impact chooser's damage tier: below 13, 23, 33, and more.
fn tier(damage: i32) -> usize {
    match damage {
        ..13 => 0,
        13..23 => 1,
        23..33 => 2,
        _ => 3,
    }
}

impl SoundData {
    pub fn load(assets: &Assets) -> Result<Self> {
        let read = |p: PathBuf| -> Result<serde_json::Value> { Ok(serde_json::from_slice(&kumite_fs::read(&p).with_context(|| format!("reading {}", p.display()))?)?) };
        let sound = read(assets.root.join("gamedata/sound.json"))?;
        let audio = read(assets.root.join("audio/index.json"))?;
        let music_json = read(assets.root.join("gamedata/music.json"))?;
        let codes: Vec<u16> = sound["sfx_table"]["codes"].as_array().context("sound.json: sfx_table")?.iter().map(|v| v.as_u64().unwrap_or(0) as u16).collect();
        let voices = sound["voice_rows"]["rows"]
            .as_array()
            .context("sound.json: voice rows")?
            .iter()
            .map(|r| {
                let c = |k: usize| r["counts"][k].as_u64().unwrap_or(0) as usize;
                ([c(0), c(1), c(2), c(3)], r["codes"].as_array().into_iter().flatten().map(|v| v.as_u64().unwrap_or(0) as u16).collect())
            })
            .collect();
        let random_voices = sound["voice_rows"]["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| {
                let v = &r["random"];
                let voices = v["voices"].as_array()?.iter().filter_map(|i| i.as_u64()).map(|i| i as usize).collect();
                Some((voices, v["lock"].as_u64().unwrap_or(0) as u32))
            })
            .collect();
        let entries = |v: &serde_json::Value| -> Vec<u16> { v.as_array().into_iter().flatten().filter_map(|c| c.as_u64()).map(|c| c as u16).collect() };
        let attack = sound["voice_rows"]["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| r["attack"].is_object().then(|| (entries(&r["attack"]["entries"]), r["attack"]["lock"].as_u64().unwrap_or(0) as u32)))
            .collect();
        let hurt = sound["voice_rows"]["rows"].as_array().into_iter().flatten().map(|r| r["hurt"].is_object().then(|| (entries(&r["hurt"]["light"]), entries(&r["hurt"]["heavy"])))).collect();
        let sfx_dir = assets.root.join("audio/sfx");
        let mut banks: HashMap<String, Vec<Sample>> = HashMap::new();
        for bank in audio["sfx"].as_array().context("audio/index.json: sfx")? {
            for s in bank["samples"].as_array().into_iter().flatten() {
                let Some(file) = s["file"].as_str() else { continue };
                let key = file.split('/').next().unwrap_or_default().to_string();
                for t in s["tones"].as_array().into_iter().flatten() {
                    let v = |k: usize| t[k].as_u64().unwrap_or(0) as u32;
                    let tone = t.get(3).and_then(|x| x.as_u64()).map(|x| x as u32);
                    banks.entry(key.clone()).or_default().push(Sample { program: v(0), keys: (v(1), v(2)), tone, file: sfx_dir.join(file) });
                }
            }
        }
        let music_dir = assets.root.join("audio/music");
        let music = music_json["bgm"]
            .as_array()
            .context("music.json: bgm")?
            .iter()
            .map(|b| {
                let col = &b["columns"][0];
                let names = b["characters"].as_array().into_iter().flatten().filter_map(|n| n.as_str()).map(str::to_string).collect();
                (music_dir.join(col["file"].as_str().unwrap_or_default()), col["loop"].as_bool().unwrap_or(false), names)
            })
            .collect();
        let music_arcade = music_json["bgm"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|b| {
                let col = &b["columns"][1];
                (music_dir.join(col["file"].as_str().unwrap_or_default()), col["loop"].as_bool().unwrap_or(false))
            })
            .collect();
        let scripts =
            sound["scripts"]["entries"].as_array().into_iter().flatten().map(|s| s.as_array().into_iter().flatten().map(|e| [0, 1, 2].map(|k| e[k].as_u64().unwrap_or(0) as u32)).collect()).collect();
        let system_start = sound["voice_rows"]["rows"].as_array().into_iter().flatten().map(|r| r["system_start"].as_u64().map(|s| s as usize)).collect();
        let row = |v: &serde_json::Value| v.as_u64().map(|r| r as usize);
        let si = &sound["system_impacts"];
        let (system_ko, system_blocked) = ([row(&si["ko"][0]).unwrap_or(0), row(&si["ko"][1]).unwrap_or(0)], row(&si["blocked"]).unwrap_or(0));
        let (copy_row, metal_row) = (row(&sound["copy_voice_remap"]["row"]), row(&sound["impact_rules"]["metal_row"]));
        let struck = sound["voice_rows"]["rows"].as_array().into_iter().flatten().map(|r| Some((r["struck"]["arm"].as_u64()? as u16, r["struck"]["leg"].as_u64()? as u16))).collect();
        let type9 = entries(&sound["type9_sounds"]["codes"]);
        let codes_ref: &Vec<u16> = &codes;
        let copy_remap = sound["copy_voice_remap"]["pairs"].as_array().into_iter().flatten().filter_map(|p| Some((*codes_ref.get(p[0].as_u64()? as usize)?, p[1].as_u64()? as u16))).collect();
        let volumes = entries(&sound["volumes"]["table"]);
        let impacts = serde_json::from_value(sound["impact_rules"].clone()).context("sound.json: impact_rules")?;
        let moves = serde_json::from_value(sound["move_sounds"].clone()).context("sound.json: move_sounds")?;
        Ok(Self {
            codes,
            voices,
            random_voices,
            attack,
            hurt,
            system_start,
            system_ko,
            system_blocked,
            copy_row,
            metal_row,
            struck,
            type9,
            copy_remap,
            scripts,
            banks,
            music,
            music_arcade,
            announcer: sound["announcer"].clone(),
            volumes,
            impacts,
            moves,
        })
    }

    /// The copying character's voice row (sound.json `copy_voice_remap.row`).
    pub fn copy_row(&self) -> Option<usize> {
        self.copy_row
    }

    /// The WAV for a SePlay code (kumite_converter's reference engine.md §9.1): bank 1 system, 2 announcer, 3 the
    /// character's own bank (`own` = "m071" etc.); the program's tone of that index (SsUtKeyOnV), or in a bank without
    /// the indices (the extra fighters' ones) the tone whose key range holds tone + 60.
    pub fn file(&self, code: u16, own: &str) -> Option<&Path> {
        let bank = match code >> 14 {
            1 => "sys",
            2 => "ann",
            3 => own,
            _ => return None,
        };
        let (program, tone) = (((code >> 4) & 3) as u32, (code & 0xF) as u32);
        let samples = self.banks.get(bank)?;
        if let Some(s) = samples.iter().find(|s| s.program == program && s.tone == Some(tone)) {
            return Some(s.file.as_path());
        }
        let note = tone + 60;
        samples.iter().find(|s| s.program == program && s.tone.is_none() && s.keys.0 <= note && note <= s.keys.1).map(|s| s.file.as_path())
    }

    /// The SePlay volume (bits 9-6) through the game's table 0x800278FC (out of 127; full without it).
    fn volume(&self, code: u16) -> f32 {
        self.volumes.get(((code >> 6) & 0xF) as usize).map_or(1.0, |&v| v as f32 / 127.0)
    }

    /// A system sound (menus: bank 1) by its SePlay code.
    pub fn system(&self, code: u16) -> Option<Sound> {
        self.sound(code, "")
    }

    /// A move sound-list table entry (0x80097050) as a sound, character banks resolving to `own`, owned by voice
    /// row `row`.
    fn table_for(&self, index: usize, own: &str, row: usize) -> Option<Sound> {
        self.codes.get(index).copied().filter(|&c| c != 0).and_then(|c| self.sound_of(c, own, row))
    }

    /// A sound owned by voice row `row`: the copying character's play through its remap (0x80075548).
    fn sound_of(&self, code: u16, own: &str, row: usize) -> Option<Sound> {
        let code = if Some(row) == self.copy_row { self.copy_remap.iter().find(|m| m.0 == code).map_or(code, |m| m.1) } else { code };
        self.sound(code, own)
    }

    /// Voice `index` of category `cat` (0..3) of row `row`.
    fn voice(&self, row: usize, own: &str, cat: usize, index: usize) -> Option<Sound> {
        let (counts, codes) = self.voices.get(row)?;
        if index >= *counts.get(cat)? {
            return None;
        }
        let at = counts[..cat].iter().sum::<usize>() + index;
        codes.get(at).and_then(|&c| self.sound_of(c, own, row))
    }

    /// A sound-list entry `type << 12 | index`: types 2..5 are the character's voice categories 0..3, the others
    /// index the sound-list table.
    fn entry(&self, code: u16, character: usize, own: &str) -> Option<Sound> {
        let (kind, index) = ((code >> 12) as usize, (code & 0xFFF) as usize);
        if (2..=5).contains(&kind) {
            return self.voice(character, own, kind - 2, index);
        }
        self.table_for(index, own, character)
    }

    fn sound(&self, code: u16, own: &str) -> Option<Sound> {
        self.file(code, own).map(|f| Sound { file: f.to_path_buf(), volume: self.volume(code) })
    }

    fn index(&self, path: &[&str]) -> Option<u16> {
        let mut v = &self.announcer;
        for p in path {
            v = &v[*p];
        }
        v.as_u64().map(|i| i as u16)
    }

    fn table(&self, index: u16) -> Option<Sound> {
        self.codes.get(index as usize).copied().filter(|&c| c != 0).and_then(|c| self.sound(c, ""))
    }

    /// The announcer's voice for a bout announcement; `humans[i]`: player i is a human (YOU WIN / YOU LOSE).
    pub fn announce(&self, a: crate::bout::Announcement, humans: [bool; 2]) -> Option<Sound> {
        use crate::bout::Announcement as A;
        let index = match a {
            A::Round(_) => self.index(&["round"]),
            A::RoundNumber(n) => self.announcer["round_numbers"]["by_round"].get(n as usize).and_then(|v| v.as_u64()).map(|i| i as u16),
            A::FinalRound => self.index(&["final_round"]),
            A::Fight => self.index(&["fight"]),
            A::TimeUp => self.index(&["result", "time_up"]),
            A::Perfect => self.index(&["result", "perfect"]),
            A::DoubleKo => self.index(&["result", "double_ko"]),
            A::Great => self.index(&["result", "great"]),
            A::Ko => self.index(&["result", "ko"]),
            A::Wins(i) => self.index(&["match", if humans[i] { "you_win" } else { "you_lose" }]),
            A::Draw => self.index(&["match", "draw"]),
        }?;
        self.table(index)
    }

    /// The announcer outside the fight.
    pub fn call(&self, c: Call) -> Option<Sound> {
        let index = match c {
            Call::ContinueStart => self.index(&["continue", "start"])?,
            Call::Count(n) => self.index(&["continue", "count"])?.checked_add(u16::try_from(n).ok()?)?,
            Call::ContinueZero => self.index(&["continue", "zero"])?,
        };
        self.table(index)
    }

    /// The fight music of a character (BGM ids 7..27 belong to the characters), and whether it loops.
    pub fn music_for(&self, name: &str) -> Option<(&Path, bool)> {
        self.music.iter().find(|m| m.2.iter().any(|n| n == name)).map(|m| (m.0.as_path(), m.1))
    }

    /// Music by BGM id from column 0 (arranged) or 1 (arcade), and whether it loops.
    pub fn music_column(&self, id: usize, column: usize) -> Option<(&Path, bool)> {
        if column == 1 {
            return self.music_arcade.get(id).map(|m| (m.0.as_path(), m.1));
        }
        self.music(id)
    }

    /// Music by BGM id, and whether it loops (the jingles play once).
    pub fn music(&self, id: usize) -> Option<(&Path, bool)> {
        self.music.get(id).map(|m| (m.0.as_path(), m.1))
    }

    /// Sounds started by the last bout step. `last` holds each fighter's move before the step (updated
    /// here); `banks` the characters' sound banks ("m071"), `characters` their character ids (voice rows).
    pub fn step(&self, bout: &Bout, state: &mut SoundState, banks: [&str; 2], characters: [usize; 2]) -> Vec<Sound> {
        self.step_with_partner(bout, state, banks, characters, None, None)
    }

    /// [`SoundData::step`] with a co-op partner (actor 3: Force's, Ball 2 ON 2's) and actor 2's own
    /// voice (Ball 2 ON 2): their banks and character ids.
    pub fn step_with_partner(&self, bout: &Bout, state: &mut SoundState, banks: [&str; 2], characters: [usize; 2], partner: Option<(&str, usize)>, extra: Option<(&str, usize)>) -> Vec<Sound> {
        let f = &bout.game.fight;
        // (Force's other soldier is actor 2: it plays the target's sound bank, unvoiced)
        let mut fighters: Vec<&kumite_sim::Fighter> = f.fighters.iter().chain(f.extra.as_deref()).collect();
        let (eb, ec) = extra.unwrap_or((banks[1], characters[1]));
        let mut banks = vec![banks[0], banks[1], eb];
        let mut characters = vec![characters[0], characters[1], ec];
        let mut voiced = vec![0, 1];
        if extra.is_some() && fighters.len() == 3 {
            voiced.push(2);
        }
        if let (Some(p), 3) = (f.partner.as_deref(), fighters.len()) {
            // (without its own: the player's bank)
            let (bank, ch) = partner.unwrap_or((banks[0], characters[0]));
            fighters.push(p);
            banks.push(bank);
            characters.push(ch);
            if partner.is_some() {
                voiced.push(3);
            }
        }
        let n = fighters.len();
        let opponents: Vec<usize> = (0..n).map(|i| f.opponent_index(i)).collect();
        self.step_actors(&fighters, &opponents, &voiced, &f.hits, &f.data, f.frame, state, &banks[..n], &characters[..n])
    }

    /// The same for any number of actors: `opponents[i]` actor i's opponent (cmd 2 entries), the `voiced` actors
    /// play their move sounds; hits name actors by index; `characters` are the actors' voice rows (+0x1A).
    ///
    /// the original's per-frame sound player (0x80040EA0) in its order: the K.O.'s scream; the move's sound list (up to
    /// three entries, each with its own gate, every frame); while a timed script waits, nothing of the rest; the
    /// hurt voice, the attack voice; the jump and landings, the swing; then the impacts.
    #[allow(clippy::too_many_arguments)]
    pub fn step_actors(
        &self,
        fighters: &[&kumite_sim::Fighter],
        opponents: &[usize],
        voiced: &[usize],
        hits: &[kumite_sim::HitEvent],
        data: &kumite_sim::GameData,
        frame: u64,
        state: &mut SoundState,
        banks: &[&str],
        characters: &[usize],
    ) -> Vec<Sound> {
        let mut out = Vec::new();
        let n = fighters.len();
        state.last.resize(n.max(state.last.len()), None);
        state.script.resize(n.max(state.script.len()), None);
        state.voice_lock.resize(n.max(state.voice_lock.len()), 0);
        state.hurt_cooldown.resize(n.max(state.hurt_cooldown.len()), 0);
        let rand = |seed: &mut u32| {
            *seed = seed.wrapping_mul(0x41C6_4E6D).wrapping_add(12345);
            (*seed >> 16) & 0x7FFF
        };
        let pick = |list: &[u16], r: u32| list.get(((r & 0xFF) as usize * list.len()) >> 8).copied();
        // the opponents' scripts a type-9 entry 1 stops (applied after the loop)
        let mut stop_script = Vec::new();
        for &i in voiced.iter().filter(|&&i| i < n) {
            let fighter = fighters[i];
            let row = characters[i];
            let second = self.system_start.get(row).copied().flatten().is_some();
            let now = (fighter.current, fighter.frame);
            let started = fighter.frame <= 1 && state.last[i].is_none_or(|(c, fr)| c != fighter.current || fr > 1);
            let advanced = state.last[i] != Some(now);
            let last_move = state.last[i].map(|(c, _)| c);
            state.last[i] = Some(now);
            // the voice lock and the hurt cooldown run down a frame per step (0x80041920, 0x80073F10)
            if advanced {
                state.voice_lock[i] = state.voice_lock[i].saturating_sub(1);
                state.hurt_cooldown[i] = state.hurt_cooldown[i].saturating_sub(1);
            }
            let mv = fighter.current_move(data);
            let entries: Vec<&kumite_sim::data::SoundEntry> = mv.sounds.iter().flatten().take(3).collect();
            let alive = !(fighter.health <= 0 && !hits.is_empty());
            let mut voiced_now = false;
            // the K.O. (+0xDE): a category-2 voice (0x80040F0C)
            if !second && hits.iter().any(|h| h.victim == i && h.health <= 0) {
                let k = self.voices.get(row).map_or(0, |v| v.0[2]);
                if k > 0 {
                    let c = ((rand(&mut state.seed) & 0xFF) as usize * k) >> 8;
                    out.extend(self.entry(0x4000 | c as u16, row, banks[i]));
                    voiced_now = true;
                }
            }
            // timed sound scripts (type 8, 0x800413E0): a move's script starts with it and keeps running after the
            // move changes; its counter is the move's frame; each entry fires on its frame. cmd 0 plays a table
            // sound, cmd 1 an entry for this fighter, cmd 2 for the opponent
            let new_move = started || last_move.is_none_or(|c| c != fighter.current);
            if new_move {
                if let Some(e) = entries.iter().find(|e| e.kind == 8 && e.index != 255) {
                    state.script[i] = Some((e.index as usize, fighter.frame.saturating_sub(1)));
                }
            }
            // the owner being hit ends it (+0xCE)
            if hits.iter().any(|h| h.victim == i && !h.blocked) {
                state.script[i] = None;
            }
            // while a script waits for a later frame (or ends) the frame has no hurt or attack voice, no jump or
            // landing, no swing (0x8004191C)
            let script_busy = state.script[i].is_some();
            if advanced {
                if let Some((index, counter)) = &mut state.script[i] {
                    *counter += 1;
                    let list = self.scripts.get(*index).map(Vec::as_slice).unwrap_or_default();
                    for &[cmd, _, code] in list.iter().filter(|e| e[1] == *counter) {
                        // (an opponent past the voiced actors, the widened melee's soldiers: no sound)
                        let who = if cmd == 2 { opponents.get(i).copied().unwrap_or(n) } else { i };
                        let (Some(&ch), Some(&bank)) = (characters.get(who), banks.get(who)) else { continue };
                        let snd = if cmd == 0 { self.table_for(code as usize, banks[i], row) } else { self.entry(code as u16, ch, bank) };
                        out.extend(snd);
                    }
                    if list.last().is_none_or(|e| e[1] <= *counter) {
                        state.script[i] = None;
                    }
                }
            }
            let mut no_jump_sounds = false;
            if second {
                // the extra fighters': a move's own voice on its first frame waits for the lock and sets it
                // (0x80073570); OpenKumite's type 10, the random attack voice (0x80073C50): one time in two, unlocked
                let random = self.random_voices.get(row).and_then(Option::as_ref);
                if started && alive {
                    for e in entries.iter().filter(|e| e.kind == 2) {
                        if state.voice_lock[i] > 0 {
                            continue;
                        }
                        if let Some((_, lock)) = random {
                            state.voice_lock[i] = *lock;
                        }
                        out.extend(self.voice(row, banks[i], 0, e.index as usize));
                    }
                    if let Some((list, lock)) = random.filter(|r| !r.0.is_empty() && state.voice_lock[i] == 0) {
                        if entries.iter().any(|e| e.kind == 10) && rand(&mut state.seed) & 1 != 0 {
                            let k = (list.len() as u32 * (rand(&mut state.seed) & 0xFF)) >> 8;
                            out.extend(self.voice(row, banks[i], 0, list[k as usize]));
                            state.voice_lock[i] = *lock;
                        }
                    }
                }
            } else {
                // the original's list entries (0x8001A6B4): 2 a category-0 voice on frame 1, the lock clear and nothing
                // voiced, then the lock (row+0xC); 3 a category-1 voice likewise, no lock but nothing more voiced;
                // 9: 0 no jump or landing sounds, 1 with the opponent's script running a category-0 voice (row+0x12,
                // 0x2000) and its script stopped, 2 the lock held (1); 3 one of three sounds (0x8001AD4C)
                for e in &entries {
                    match (e.kind, e.index) {
                        (2 | 3, idx) if started && alive && state.voice_lock[i] == 0 && !voiced_now => {
                            out.extend(self.voice(row, banks[i], (e.kind - 2) as usize, idx as usize));
                            if e.kind == 2 {
                                state.voice_lock[i] = self.attack.get(row).and_then(Option::as_ref).map_or(10, |a| a.1);
                            } else {
                                voiced_now = true;
                            }
                        }
                        (9, 0) => no_jump_sounds = true,
                        (9, 1) if advanced && opponents.get(i).is_some_and(|&o| state.script.get(o).is_some_and(Option::is_some)) => {
                            out.extend(self.entry(0x2000, row, banks[i]));
                            stop_script.extend(opponents.get(i).copied());
                        }
                        (9, 2) => state.voice_lock[i] = 1,
                        (9, 3) if advanced => {
                            let k = ((rand(&mut state.seed) & 0xFF) as usize * self.type9.len()) >> 8;
                            if let Some(&c) = self.type9.get(k) {
                                out.extend(self.sound_of(c, banks[i], row));
                            }
                        }
                        _ => {}
                    }
                }
                if !script_busy {
                    // a clean hit (+0xD0) on the living fighter, its cooldown clear, on odd frames: the light or the
                    // heavy list by the attacker's damage (+0x5E, 20), then 40 frames' cooldown (0x80041600)
                    if !voiced_now && fighter.health > 0 && state.hurt_cooldown[i] == 0 && frame & 1 != 0 {
                        if let Some(h) = hits.iter().find(|h| h.victim == i && !h.blocked && !h.throw) {
                            let heavy = fighters.get(h.attacker).is_some_and(|a| attack_damage(a.current_move(data)) >= 20);
                            if let Some((light, strong)) = self.hurt.get(row).and_then(Option::as_ref) {
                                if let Some(code) = pick(if heavy { strong } else { light }, rand(&mut state.seed)) {
                                    out.extend(self.entry(code, row, banks[i]));
                                    state.hurt_cooldown[i] = 40;
                                    voiced_now = true;
                                }
                            }
                        }
                    }
                    // a damaging move starting (+0xDA), the lock clear and nothing voiced: one time in four an
                    // attack voice, then the lock (0x80041734)
                    if started && !voiced_now && state.voice_lock[i] == 0 && attack_damage(mv) != 0 && rand(&mut state.seed) & 3 == 0 {
                        if let Some((list, lock)) = self.attack.get(row).and_then(Option::as_ref) {
                            if let Some(code) = pick(list, rand(&mut state.seed)) {
                                out.extend(self.entry(code, row, banks[i]));
                                state.voice_lock[i] = *lock;
                            }
                        }
                    }
                }
            }
            if script_busy && !second {
                continue;
            }
            // a jump's take-off (the first frame of its jump window, not frame 1) and the landings from a flight (a
            // juggle's, the air setup's) (0x80041830, 0x80040DF4)
            let m = &self.moves;
            if !no_jump_sounds {
                let js = mv.bytes_18.jump_start;
                if advanced && js != 0 && fighter.frame == js && fighter.frame != 1 {
                    out.extend(self.table(m.take_off & 0xFFF));
                }
                match fighter.landing {
                    1 => out.extend(self.table(m.landing & 0xFFF)),
                    2 => out.extend(self.table(m.air_landing & 0xFFF)),
                    _ => {}
                }
            }
            // the attack's swing, hit or not: the original's whoosh (`move_sounds.swing`) the frame before
            // the first active one (0x800418F8 -> 0x80041B30); an extra fighter's move its own (its type-2 entry,
            // OpenKumite's type 6: a bank-2 sound, 0x80073390) on the first active frame
            if advanced {
                let a0 = mv.active[0];
                if second {
                    if fighter.frame == a0 {
                        for e in entries.iter().filter(|e| e.kind == 6) {
                            out.extend(self.table_for(e.index as usize, banks[i], row));
                        }
                    }
                } else if mv.attack.is_some() && a0 > 1 && fighter.frame == a0 - 1 {
                    out.extend(self.table(m.swing & 0xFFF));
                }
            }
        }
        for o in stop_script {
            if let Some(s) = state.script.get_mut(o) {
                *s = None;
            }
        }
        // impacts (0x80040EA0, sound.json `impact_rules`): a K.O. once (an arm: descriptor byte 0 below 0xB), with or
        // without impact entries; else per type 1 / 7 entry of the attacker's list: a block (an arm below 0xC); the
        // metal row struck, its metal ring (0x80041B70); a row with its own struck sounds, those (0x800733B0); else the
        // entry's own sound, or for the auto codes the chooser 0x80041CB4: legs by the attacker's damage (+0x5E: the
        // move's, 0 for a throw; below 13, 23, 33, more), arms likewise, the lightest's two by rand bit 7 (rand drawn
        // in every tier). A row with its own bank-2 sounds attacking: those of `system_impacts` for a block and a K.O.,
        // else its entries' own sounds
        for h in hits {
            let (a, v) = (h.attacker, h.victim);
            let (Some(fa), Some(fv)) = (fighters.get(a), fighters.get(v)) else { continue };
            let mv = fa.current_move(data);
            let limb = mv.desc0.unwrap_or(0);
            let damage = attack_damage(mv);
            let (row_a, row_v) = (characters.get(a).copied().unwrap_or(0), characters.get(v).copied().unwrap_or(0));
            let impacts: Vec<u16> = mv.sounds.iter().flatten().take(3).filter(|e| e.kind == 1 || e.kind == 7).map(|e| ((e.kind << 12) | e.index) as u16).collect();
            let system = self.system_start.get(row_a).copied().flatten();
            let play = |out: &mut Vec<Sound>, index: usize, bank: &str, row: usize| {
                if let Some(code) = self.codes.get(index).copied().filter(|&c| c != 0) {
                    out.extend(self.sound_of(code, bank, row));
                }
            };
            if h.health <= 0 && !h.blocked {
                if let Some(start) = system {
                    play(&mut out, start + self.system_ko[usize::from(limb >= 0xB)], banks[a], row_a);
                } else {
                    play(&mut out, index(self.impacts.ko[usize::from(limb >= 0xB)]), banks[a], row_a);
                }
                continue;
            }
            let wooden = state.wooden.get(v).copied().unwrap_or(false);
            for &code in &impacts {
                if h.blocked && wooden {
                    // a wooden character's block (on him)
                    play(&mut out, index(self.impacts.wooden_block[usize::from(limb >= 0xC)]), banks[v], row_v);
                } else if h.blocked {
                    if let Some(start) = system {
                        play(&mut out, start + self.system_blocked, banks[a], row_a);
                    } else {
                        play(&mut out, index(self.impacts.blocked[usize::from(limb >= 0xC)]), banks[a], row_a);
                    }
                } else if let Some((arm, leg)) = self.struck.get(row_v).and_then(|x| *x) {
                    out.extend(self.sound_of(if limb < 0xB { arm } else { leg }, banks[v], row_v));
                } else if system.is_none() && Some(row_v) == self.metal_row {
                    let (vd, metal) = (attack_damage(fv.current_move(data)), &self.impacts.metal);
                    play(&mut out, index(if damage < 7 || vd < 0x10 { metal.light } else { metal.ring[usize::from(frame & 3 == 0)] }), banks[v], row_v);
                } else if wooden {
                    // a clean hit on a wooden character: one of the wood sounds (0x80041C40)
                    let wood = &self.impacts.wood.entries;
                    let k = ((rand(&mut state.seed) & 0xFF) as usize * wood.len()) >> 8;
                    if let Some(&w) = wood.get(k) {
                        play(&mut out, index(w), banks[v], row_v);
                    }
                } else if system.is_some() || !self.impacts.auto.contains(&code) {
                    play(&mut out, index(code), banks[a], row_a);
                } else if limb >= 0xC {
                    play(&mut out, index(self.impacts.legs[tier(damage)]), banks[a], row_a);
                } else {
                    let (r, arms) = (rand(&mut state.seed), &self.impacts.arms);
                    let entry = match tier(damage) {
                        0 => arms.light[usize::from((r >> 6) & 2 != 0)],
                        t => arms.tiers[t - 1],
                    };
                    play(&mut out, index(entry), banks[a], row_a);
                }
            }
        }
        out
    }
}
