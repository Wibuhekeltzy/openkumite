//! The attract mode's embu (演武, record 0 0x800D3748; attract research §2): a scripted two-actor show on its stage,
//! not a fight. Both actor slots play the embu's own move set (aliases 3438..3459, `embu_NNN` clips baked into
//! every character) at the world origin facing +x, and swap bodies between shots; a chain of 22 scripted camera
//! motions films it; BGM 0 plays; it ends in an additive white fade. Data: `gamedata/attract.json` `embu` and
//! `movesets/moveset_embu.json`.
//!
//! Per script frame c: the script's entries up to c run (model swap: the slot is hidden until its next move;
//! play: a move over a frame range; fade; end), each playing slot shows move frame `cur + 1` and fires that frame's
//! properties, the camera shows key c + 1 of the chained shots, and the fade adds its grey.

use crate::{placement, UNITS};
use anyhow::{Context, Result};
use glam::{Mat4, Vec3};
use kumite_assets::{Assets, Character};
use kumite_sim::data::Moveset;
use kumite_sim::detmath::DetMath;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize)]
pub struct Variant {
    pub initial_models: [usize; 2],
    pub script: Vec<[i32; 5]>,
    /// Model index -> character glb.
    pub models: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
struct Camera {
    first_selector: usize,
    selector_step: usize,
    shots: usize,
    key_offset: i32,
    halve: i32,
    dolly: [i32; 2],
}

#[derive(Clone, Debug, Deserialize)]
struct Actors {
    facing: i32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Unlock {
    pub variant_2_character: usize,
    pub variant_1_unlocked_at_least: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmbuData {
    pub stage: usize,
    pub bgm: usize,
    pub alias_base: i32,
    pub end_frame: i32,
    pub wait_frames: i32,
    camera: Camera,
    actors: Actors,
    pub unlock: Unlock,
    pub variants: Vec<Variant>,
    #[serde(skip)]
    pub moveset: Option<Arc<Moveset>>,
    /// Camera keys of the chained shots (game space, Y down): eye, target.
    #[serde(skip)]
    keys: Vec<(Vec3, Vec3)>,
    /// Bone slot -> joint name (rules.json `bone_slots`, for the effects at a bone).
    #[serde(skip)]
    bone_slots: Vec<String>,
}

impl EmbuData {
    pub fn load(assets: &Assets) -> Result<Self> {
        let gd = assets.root.join("gamedata");
        let v: serde_json::Value = kumite_sim::data::load(&gd.join("attract.json"))?;
        let mut data: Self = serde_json::from_value(v["embu"].clone()).context("attract.json embu")?;
        let ms: Moveset = kumite_sim::data::load(&gd.join("movesets/moveset_embu.json"))?;
        let common = kumite_sim::data::move_count(&gd.join("movesets/common.json"))?;
        ms.check(common).context("moveset_embu.json")?;
        let c = &data.camera;
        let (h, [dn, dd]) = (c.halve.max(1), c.dolly);
        for k in 0..c.shots {
            let Some(m) = ms.camera_motions.get(&(c.first_selector + c.selector_step * k).to_string()) else { continue };
            data.keys.extend(m.values.iter().map(|v| {
                let ch = |k: usize| v.get(k).copied().unwrap_or(0);
                let tgt = [ch(3) / h, ch(4) / h, ch(5) / h];
                let eye = [0, 1, 2].map(|k| tgt[k] + (ch(k) / h - tgt[k]) * dn / dd.max(1));
                (Vec3::new(eye[0] as f32, eye[1] as f32, eye[2] as f32), Vec3::new(tgt[0] as f32, tgt[1] as f32, tgt[2] as f32))
            }));
        }
        data.moveset = Some(Arc::new(ms));
        let rules: serde_json::Value = kumite_sim::data::load(&gd.join("rules.json")).unwrap_or_default();
        data.bone_slots = rules["bone_slots"]["joints"].as_array().into_iter().flatten().map(|j| j.as_str().unwrap_or("").to_string()).collect();
        Ok(data)
    }

    /// The highest variant the unlocks allow (0x800563BC): 2 with the character `variant_2_character`, 1 with at
    /// least `variant_1_unlocked_at_least` characters, else 0.
    pub fn max_variant(&self, unlocked: &[usize]) -> usize {
        if unlocked.contains(&self.unlock.variant_2_character) {
            2
        } else if unlocked.len() >= self.unlock.variant_1_unlocked_at_least {
            1
        } else {
            0
        }
    }
}

/// Something the app does for the scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EmbuEvent {
    Music(usize),
    /// A ground impact (op 4) at a root (metres).
    GroundImpact(Vec3),
    /// A timed effect at a bone (ops 6..0xA) of the actor showing `model` (the game's model index).
    Effect {
        op: u32,
        pos: Vec3,
        model: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    /// Not drawn (after a move ran out, or a body swap until the next move).
    Hidden,
    /// Embu move `mv`, its frame index `cur` shown next, up to `end`.
    Playing { mv: usize, cur: u32, end: u32 },
}

pub struct EmbuScene {
    pub data: Arc<EmbuData>,
    pub variant: usize,
    /// The characters by model index.
    pub characters: HashMap<usize, Arc<Character>>,
    /// Per slot: the model index it shows, and its state.
    pub models: [usize; 2],
    slots: [Slot; 2],
    /// What each slot shows this frame: (embu move, frame index).
    shown: [Option<(usize, u32)>; 2],
    next: usize,
    /// The script frame: negative while the scene waits to start.
    c: i32,
    fade: Option<(i32, i32)>,
    pub white: u8,
    pub done: bool,
    pub events: Vec<EmbuEvent>,
    yaw: f32,
}

impl EmbuScene {
    /// Load the characters of variant `variant` and set the scene up (0x800D36B0, state 0).
    pub fn new(assets: &Assets, data: Arc<EmbuData>, variant: usize) -> Result<Self> {
        let v = data.variants.get(variant).context("embu variant")?;
        let mut characters = HashMap::new();
        for (m, file) in &v.models {
            let m: usize = m.parse()?;
            characters.insert(m, Arc::new(assets.character_file(file)?));
        }
        let yaw = data.actors.facing as f32 * std::f32::consts::TAU / 65536.0;
        let models = v.initial_models;
        let c = -data.wait_frames;
        Ok(Self { data, variant, characters, models, slots: [Slot::Hidden; 2], shown: [None; 2], next: 0, c, fade: None, white: 0, done: false, events: Vec::new(), yaw })
    }

    /// One 60 Hz frame.
    pub fn step(&mut self) {
        self.events.clear();
        if self.done {
            return;
        }
        self.shown = [None; 2];
        if self.c < 0 {
            // waiting for the loads (a 4-frame countdown), the stage shown; BGM 0 (re)starts
            if self.c == -self.data.wait_frames {
                self.events.push(EmbuEvent::Music(self.data.bgm));
            }
            self.c += 1;
            return;
        }
        let c = self.c;
        let script = &self.data.variants[self.variant].script;
        let mut end = false;
        while let Some(&[frame, cmd, a, b, e]) = script.get(self.next) {
            if c < frame {
                break;
            }
            self.next += 1;
            match cmd {
                0 | 1 => {
                    let s = cmd as usize;
                    if self.models[s] != a as usize {
                        self.models[s] = a as usize;
                        self.slots[s] = Slot::Hidden; // the new body is built over the next 7 frames
                    }
                }
                2 | 3 => self.slots[cmd as usize - 2] = Slot::Playing { mv: (a - self.data.alias_base).max(0) as usize, cur: b.max(0) as u32, end: e.max(0) as u32 },
                5 => self.fade = Some((0, a)),
                -1 => end = true,
                _ => {}
            }
        }
        for s in 0..2 {
            if let Slot::Playing { mv, cur, end } = self.slots[s] {
                self.shown[s] = Some((mv, cur));
                self.fire(s, mv, cur + 1);
                self.slots[s] = if cur + 1 > end { Slot::Hidden } else { Slot::Playing { mv, cur: cur + 1, end } };
            }
        }
        if let Some((level, speed)) = self.fade {
            self.white = level.clamp(0, 255) as u8;
            self.fade = Some(((level + speed).min(256), speed));
        }
        self.c += 1;
        self.done = end || self.c > self.data.end_frame;
    }

    /// The move's timed properties at `frame` (ground impacts and effects at a bone; hand poses are not drawn).
    fn fire(&mut self, s: usize, mv: usize, frame: u32) {
        let Some(m) = self.data.moveset.as_ref().and_then(|ms| ms.moves.get(mv)) else { return };
        for p in m.properties.iter().flatten().filter(|p| p.frame == frame) {
            match p.op {
                4 => {
                    if let Some((_, place)) = self.pose(s) {
                        self.events.push(EmbuEvent::GroundImpact(place.w_axis.truncate()));
                    }
                }
                6..=0x0A => {
                    if let Some(pos) = self.bone(s, p.arg) {
                        self.events.push(EmbuEvent::Effect { op: p.op, pos, model: self.models[s] });
                    }
                }
                _ => {}
            }
        }
    }

    /// World position (metres) of slot `s`'s bone slot.
    fn bone(&self, s: usize, slot: u32) -> Option<Vec3> {
        let name = self.data.bone_slots.get(slot as usize)?;
        let ch = self.characters.get(&self.models[s])?;
        let node = ch.nodes.iter().position(|n| &n.name == name)?;
        let (pose, place) = self.pose(s)?;
        Some(place.transform_point3(pose[node].w_axis.truncate()))
    }

    /// Slot `s` this frame, when drawn: its node matrices and placement. The root is drawn at the origin plus
    /// the clip's root, turned by the facing (absolute, as in the game).
    pub fn pose(&self, s: usize) -> Option<(Vec<Mat4>, Mat4)> {
        let (mv, idx) = self.shown[s]?;
        let ch = self.characters.get(&self.models[s])?;
        let clip = ch.clip(&format!("embu_{mv:03}"))?;
        let root = ch.nodes.iter().position(|n| n.parent.is_none())?;
        let r = ch.clips[clip].translations[root].as_ref().map_or(Vec3::ZERO, |t| t[(idx as usize).min(t.len().saturating_sub(1))] * UNITS);
        let (sn, cs) = self.yaw.dsin_cos();
        let at = Vec3::new(r.x * cs + r.z * sn, 0.0, -r.x * sn + r.z * cs);
        Some((ch.pose(clip, idx as usize, true), placement(at, self.yaw)))
    }

    /// The camera: eye and target in metres (y up), key c + 1 of the chained shots.
    pub fn camera(&self) -> (Vec3, Vec3) {
        let k = (self.c - 1 + self.data.camera.key_offset).clamp(0, self.data.keys.len() as i32 - 1).max(0) as usize;
        let to_m = |p: Vec3| Vec3::new(p.x, -p.y, -p.z) / UNITS;
        self.data.keys.get(k).map_or((Vec3::new(0.0, 1.5, 4.0), Vec3::new(0.0, 1.0, 0.0)), |&(e, t)| (to_m(e), to_m(t)))
    }

    /// The script frame shown last.
    pub fn frame(&self) -> i32 {
        self.c - 1
    }
}
