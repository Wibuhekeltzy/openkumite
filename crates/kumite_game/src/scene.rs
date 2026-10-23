//! In-engine cut-scenes (`gamedata/scenes.json`). The boss-change scene (arcade overlay, init 0x800B3CA0, per
//! frame 0x800B4060): after the human wins a round against the last arcade stage's boss, the human's fighter is shown
//! as the second-to-last stage's boss lying at the last boss's feet, both at the world origin facing +x, playing the
//! last boss's moves (the body borrows its animation), on its scripted camera, with the scene's music and a white
//! fade; then the opponent becomes the one scenes.json names and the match goes on.
//!
//! Timeline (scene frame c): one black frame, then c = 0..=256 runs the script (its entries fire at their frame),
//! the fighters show move frame c − 9 from their scripted start, the camera shows key c; the fade adds grey
//! 4 · (c − 192) and the stage stops drawing at level 0xD0; then 4 frames of full white.

use crate::{Game, UNITS};
use anyhow::{Context, Result};
use glam::Vec3;
use kumite_assets::Assets;
use kumite_sim::data::{Bank, MoveRef};
use kumite_sim::detmath::DetMath;
use kumite_sim::Current;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
struct Scripts {
    p1_human: Vec<[i32; 5]>,
    p2_human: Vec<[i32; 5]>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BodyChoice {
    /// The character who gets the other body (the second-to-last stage's boss itself), and the model indexes by the
    /// boss costume.
    pub alt_character: usize,
    pub model_index: [usize; 2],
    pub model_index_alt: [usize; 2],
}

/// scenes.json `boss_change`.
#[derive(Clone, Debug, Deserialize)]
pub struct BossChangeData {
    scripts: Scripts,
    pub bgm: usize,
    pub camera_moveset: i64,
    pub camera_selector: usize,
    pub camera_halve: i32,
    pub camera_dolly: [i32; 2],
    pub body: BodyChoice,
    pub facing: i32,
    pub stage_off_level: i32,
    pub white_frames: u32,
    /// The character the opponent becomes.
    pub becomes: usize,
    pub trigger: Trigger,
    /// Model index (0x800958C4) -> model record, from roster.json.
    #[serde(skip)]
    pub models: std::collections::HashMap<usize, usize>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Trigger {
    pub stage_index: usize,
}

impl BossChangeData {
    pub fn load(assets: &Assets) -> Result<Self> {
        let gd = assets.root.join("gamedata");
        let v: serde_json::Value = kumite_sim::data::load(&gd.join("scenes.json"))?;
        let mut data: Self = serde_json::from_value(v["boss_change"].clone()).context("scenes.json boss_change")?;
        let roster: serde_json::Value = kumite_sim::data::load(&gd.join("roster.json"))?;
        for e in roster["selections"]["entries"].as_array().into_iter().flatten() {
            if let (Some(i), Some(m)) = (e["model_index"].as_u64(), e["model_record"].as_u64()) {
                data.models.insert(i as usize, m as usize);
            }
        }
        Ok(data)
    }

    /// The model record the human's slot shows: the second-to-last stage's boss (its stand-in for a player who is that
    /// boss), its costume.
    pub fn body_model(&self, human_character: usize, boss_costume: usize) -> Option<usize> {
        let set = if human_character == self.body.alt_character { self.body.model_index_alt } else { self.body.model_index };
        self.models.get(&set[usize::from(boss_costume != 0)]).copied()
    }
}

/// Something the app plays for the scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneEvent {
    Music(usize),
}

/// The boss-change scene running.
pub struct BossChangeScene {
    /// The two fighters of the scene: the human's slot with the boss's body, and the last boss.
    pub game: Game,
    data: BossChangeData,
    script: Vec<[i32; 5]>,
    next: usize,
    /// −1: the black first frame (state 0); then the scene frame.
    c: i32,
    /// Per slot: the scripted move, its next frame (0-based) and its last.
    slots: [Option<(Current, u32, u32)>; 2],
    /// Fade level and its step per frame (cmd 5); the stage is no longer drawn once it reached its level.
    fade: Option<(i32, i32)>,
    stage_off: bool,
    /// Full-white frames left once the script ended.
    white_left: Option<u32>,
    /// Camera keys (game space, Y down): eye, target.
    keys: Vec<(Vec3, Vec3)>,
    /// What to draw this frame.
    pub stage_visible: bool,
    pub white: u8,
    pub done: bool,
    pub events: Vec<SceneEvent>,
}

impl BossChangeScene {
    /// `game`: the scene's fighters (slot `human` = the boss's body, the other the last boss).
    pub fn new(mut game: Game, data: BossChangeData, human: usize) -> Self {
        let script = if human == 0 { data.scripts.p1_human.clone() } else { data.scripts.p2_human.clone() };
        // the camera motion comes from the fighter with the last boss's move set (0x800670FC)
        let motion = game.fight.fighters.iter().find(|f| f.moveset.moveset_id == Some(data.camera_moveset)).and_then(|f| f.moveset.camera_motions.get(&data.camera_selector.to_string()).cloned());
        let (h, [dn, dd]) = (data.camera_halve.max(1), data.camera_dolly);
        let keys = motion
            .map(|m| {
                m.values
                    .iter()
                    .map(|v| {
                        let ch = |k: usize| v.get(k).copied().unwrap_or(0);
                        let tgt = [ch(3) / h, ch(4) / h, ch(5) / h];
                        let eye = [0, 1, 2].map(|k| tgt[k] + (ch(k) / h - tgt[k]) * dn / dd.max(1));
                        (Vec3::new(eye[0] as f32, eye[1] as f32, eye[2] as f32), Vec3::new(tgt[0] as f32, tgt[1] as f32, tgt[2] as f32))
                    })
                    .collect()
            })
            .unwrap_or_default();
        // both at the origin facing +x (0x4000)
        let yaw = data.facing as f32 * std::f32::consts::TAU / 65536.0;
        for f in &mut game.fight.fighters {
            f.position = Vec3::ZERO;
            f.offset = Vec3::ZERO;
            f.yaw = yaw;
            f.root_yaw = yaw;
            f.locked = true;
        }
        game.visible = [false; 2];
        Self { game, data, script, next: 0, c: -1, slots: [None, None], fade: None, stage_off: false, white_left: None, keys, stage_visible: false, white: 0, done: false, events: Vec::new() }
    }

    /// The camera this frame: eye and target in metres (y up).
    pub fn camera(&self) -> (Vec3, Vec3) {
        // the key of the frame just run (c has moved on to the next one)
        let k = (self.c - 1).clamp(0, self.keys.len() as i32 - 1).max(0) as usize;
        let to_m = |p: Vec3| Vec3::new(p.x, -p.y, -p.z) / UNITS;
        self.keys.get(k).map_or((Vec3::new(0.0, 1.5, 4.0), Vec3::new(0.0, 1.0, 0.0)), |&(e, t)| (to_m(e), to_m(t)))
    }

    /// Start move `alias` on slot `p`: its own, or (an empty alias) the other fighter's, borrowed like a throw
    /// victim's (the round-init borrow merge 0x8002D07C).
    fn start(&mut self, p: usize, alias: usize, from: u32, end: u32) {
        let own = self.game.fight.fighters[p].moveset.aliases.as_ref().and_then(|a| a.get(alias)).copied();
        let other = self.game.fight.fighters[1 - p].moveset.clone();
        let current = match own {
            Some(MoveRef { bank: Bank::Own, index: Some(i) }) => Current { bank: Bank::Own, index: i },
            Some(MoveRef { bank: Bank::Common, index: Some(i) }) => Current { bank: Bank::Common, index: i },
            _ => match other.aliases.as_ref().and_then(|a| a.get(alias)).copied() {
                Some(MoveRef { bank: Bank::Own, index: Some(i) }) => {
                    self.game.fight.fighters[p].partner = Some(Arc::clone(&other));
                    Current { bank: Bank::Partner, index: i }
                }
                Some(MoveRef { bank: Bank::Common, index: Some(i) }) => Current { bank: Bank::Common, index: i },
                _ => {
                    log::warn!("scene: alias {alias} is empty for both fighters");
                    return;
                }
            },
        };
        self.slots[p] = Some((current, from, end));
    }

    pub fn step(&mut self) {
        self.events.clear();
        if self.done {
            return;
        }
        if let Some(left) = self.white_left {
            // state 2: full white, no stage, no fighters
            self.white = 255;
            self.stage_visible = false;
            self.game.visible = [false; 2];
            self.white_left = Some(left.saturating_sub(1));
            self.done = left <= 1;
            return;
        }
        if self.c < 0 {
            // state 0: the camera starts; a black frame
            self.c = 0;
            return;
        }
        let c = self.c;
        self.stage_visible = !self.stage_off;
        while let Some(&[frame, cmd, a, b, e]) = self.script.get(self.next) {
            if c < frame {
                break;
            }
            self.next += 1;
            match cmd {
                6 => self.events.push(SceneEvent::Music(self.data.bgm)),
                2 | 3 => self.start(cmd as usize - 2, a.max(0) as usize, b.max(0) as u32, e.max(0) as u32),
                5 => self.fade = Some((0, a)),
                -1 => self.white_left = Some(self.data.white_frames),
                _ => log::warn!("scene: unknown command {cmd}"),
            }
        }
        // the scripted moves: move frame = their counter + 1, placed by their absolute root (one origin, as a
        // throw pair)
        for p in 0..2 {
            let Some((current, cur, end)) = self.slots[p] else { continue };
            let frame = cur + 1;
            let root = {
                let f = &mut self.game.fight.fighters[p];
                f.current = current;
                f.frame = frame;
                self.game.anim_root(p)
            };
            let f = &mut self.game.fight.fighters[p];
            if let Some(r) = root {
                let (s, co) = f.yaw.dsin_cos();
                f.offset = Vec3::new(r.x * co + r.z * s, 0.0, -r.x * s + r.z * co);
            }
            self.game.visible[p] = true;
            self.slots[p] = Some((current, (cur + 1).min(end), end));
        }
        if let Some((level, speed)) = self.fade {
            self.stage_off |= level >= self.data.stage_off_level;
            let level = level.min(0x100);
            self.white = level.clamp(0, 255) as u8;
            self.fade = Some((level + speed, speed));
        }
        self.c += 1;
    }
}
