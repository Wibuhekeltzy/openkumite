//! Hit sparks, ground dust, heavy-hit sparks, the hit light and the screen shake, as the game does them
//! (effects research; data in `pictures/effects/effects.json` + `atlas.png`, written by export_effects).
//!
//! The game has two effect systems and no blood:
//! - **sprite slots** (0x80076884 / 0x80076E70): camera-facing 188.6 mm squares animating one 32-texel cell a
//!   frame. Hit sparks share a ring of 3 slots (a 4th overwrites the oldest). The red hit spark (sheet 007, CLUT
//!   (32,503)) flies along the strike at 50 mm a frame with a drag of 118/128 and stays at least 250 mm up; the
//!   block spark (steel blue) and the zero-damage spark (pink) stand still. Additive kinds dim linearly over their
//!   last 10 frames. Ground impacts (op 4, and a body landing from the air) are groups of 6 grey dust puffs (50 %
//!   blend) on a 0.5 m ring, one every 2 frames, 250 mm up; 3 groups at most;
//! - **effect objects** (pool 0x8006F4F4): on a hit of 21+ damage, 6 + 20 small additive particles fly off along
//!   the strike and fade through a palette ramp (white -> yellow -> red; blue for a ki-charged attacker), with the
//!   attacker's own character spark and its **hit light** (the fighters lit in the attacker's colour, fading over
//!   48 frames).
//!
//! Moves fire effects with timed properties: shakes (ops 1-3), a ground impact (op 4), the own / other character
//! spark (ops 6 / 7) and the red / pink / dust sprites (ops 8 / 9 / 0xA) at a bone.

use crate::{Game, UNITS};
use glam::Vec3;
use kumite_sim::detmath::DetMath;
use kumite_sim::MoveEffect;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    /// Added light (original-hardware semi-transparency mode 1).
    Add,
    /// Half of the sprite over half of the picture (mode 0): the dust.
    Half,
}

/// One animation of the atlas: its cells, one per frame.
#[derive(Clone, Debug, Default)]
pub struct Strip {
    pub cells: Arc<Vec<u32>>,
    pub life: u32,
}

/// A character's own spark and hit-light colour.
#[derive(Clone, Debug, Default)]
pub struct CharSpark {
    pub strip: Strip,
    pub light: [f32; 3],
}

/// effects.json.
#[derive(Clone, Debug, Default)]
pub struct EffectsData {
    pub cols: u32,
    pub rows: u32,
    pub spark: Strip,
    pub zero: Strip,
    pub block: Strip,
    pub dust: Strip,
    pub heavy: [Arc<Vec<u32>>; 2],
    /// By model record.
    pub characters: HashMap<usize, CharSpark>,
    /// Ground impact puff offsets (metres, world).
    pub ground: Vec<Vec3>,
}

impl EffectsData {
    pub fn load(assets: &kumite_assets::Assets) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_slice(&kumite_fs::read(&assets.root.join("pictures/effects/effects.json")).ok()?).ok()?;
        let cells = |c: &serde_json::Value| Arc::new(c.as_array().into_iter().flatten().filter_map(|x| x.as_u64()).map(|x| x as u32).collect::<Vec<_>>());
        let strip = |name: &str| Strip { cells: cells(&v["strips"][name]["cells"]), life: v["strips"][name]["life"].as_u64().unwrap_or(0) as u32 };
        let characters = v["characters"]
            .as_object()?
            .iter()
            .filter_map(|(m, c)| {
                let light = c["light"].as_array()?;
                let rgb = |k: usize| light.get(k).and_then(|x| x.as_f64()).unwrap_or(255.0) as f32 / 255.0;
                Some((m.parse().ok()?, CharSpark { strip: Strip { cells: cells(&c["cells"]), life: c["life"].as_u64().unwrap_or(30) as u32 }, light: [rgb(0), rgb(1), rgb(2)] }))
            })
            .collect();
        let ground = v["ground_offsets"]
            .as_array()?
            .iter()
            .map(|o| {
                let k = |i: usize| o.get(i).and_then(|x| x.as_f64()).unwrap_or(0.0) as f32 / UNITS;
                Vec3::new(k(0), -k(1), -k(2)) // game space (y down, z away) -> world
            })
            .collect();
        Some(Self {
            cols: v["atlas"]["cols"].as_u64()? as u32,
            rows: v["atlas"]["rows"].as_u64()? as u32,
            spark: strip("spark"),
            zero: strip("zero"),
            block: strip("block"),
            dust: strip("dust"),
            heavy: [cells(&v["strips"]["heavy_a"]["cells"]), cells(&v["strips"]["heavy_b"]["cells"])],
            characters,
            ground,
        })
    }
}

/// The sprite size: 32 texels at the game's projection = 188.6 mm, whatever the distance.
const SPRITE: f32 = 0.1886;
/// A sprite is sorted this much nearer than its centre (0x800771DC: ordering-table index (OTZ - 0x6E) >> 3, OTZ a
/// quarter of the screen depth), so one at a joint shows over the limb around it (metres).
const SORT_NEARER: f32 = 0x6E as f32 * 4.0 / UNITS;
/// Red spark speed (6400 / 128 units a frame) and drag.
const SPARK_SPEED: f32 = 0.05;
const DRAG: f32 = 118.0 / 128.0;
/// Sprites keep their centre at least this high (y −0x7D00 / 128).
const MIN_HEIGHT: f32 = 0.25;
/// A hit of this much damage adds the character spark, its light and the heavy-hit particles.
const HEAVY: i32 = 21;
const LIGHT_FRAMES: u32 = 48;

/// A sprite slot (system A).
#[derive(Clone, Debug)]
pub struct SpriteFx {
    pub pos: Vec3,
    pub vel: Vec3,
    /// 1 on the frame it appears; it is gone after `life`.
    pub age: u32,
    pub strip: Strip,
    pub blend: Blend,
    /// Dims over the last 10 frames (additive kinds).
    pub fade: bool,
    pub mirror: bool,
}

/// A ground impact: 6 puffs, puff k appearing on its 2k-th update.
#[derive(Clone, Debug)]
pub struct Ground {
    pub pos: Vec3,
    pub age: u32,
    pub puffs: Vec<SpriteFx>,
}

/// A heavy-hit particle (system B, types 1 / 2 and 12 / 13).
#[derive(Clone, Debug)]
pub struct Particle {
    pub pos: Vec3,
    /// Unit heading and the speed law: constant (type 2) or easing from v0 to v1 (type 1); game units / 16 a frame.
    pub dir: Vec3,
    pub v0: f32,
    pub v1: f32,
    pub easing: bool,
    pub age: u32,
    pub life: u32,
    pub ramp: usize,
    pub size: f32,
}

/// A quad to draw: centre, size, atlas cell, colour (multiplies the texture), blend.
#[derive(Clone, Copy, Debug)]
pub struct Sprite {
    pub pos: Vec3,
    pub size: f32,
    pub cell: u32,
    pub color: [f32; 4],
    pub blend: Blend,
    pub mirror: bool,
    /// Depth the sprite is drawn nearer the camera than it is (metres; its place and size on screen unchanged).
    pub nearer: f32,
}

pub struct Vfx {
    pub data: Option<EffectsData>,
    /// The hit-spark ring and its write index.
    pub slots: [Option<SpriteFx>; 3],
    next: usize,
    pub grounds: [Option<Ground>; 3],
    next_ground: usize,
    pub particles: Vec<Particle>,
    /// The hit light: colour and frames left (0x80039F0C), and where it is.
    pub light: Option<([f32; 3], u32, Vec3)>,
    /// Camera shake scripts (camera.json `shake`: pitch offsets per frame).
    scripts: Vec<Vec<i32>>,
    shake: Option<(usize, usize)>,
    /// PsyQ rand state (0x8007A318).
    seed: u32,
    /// Bone slot -> joint name (rules.json `bone_slots`).
    slots_joints: Vec<String>,
}

impl Vfx {
    pub fn new(assets: &kumite_assets::Assets) -> Self {
        let read = |name: &str| -> serde_json::Value { kumite_fs::read(&assets.root.join("gamedata").join(name)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default() };
        let camera = read("camera.json");
        let scripts =
            camera["shake"]["scripts"].as_array().into_iter().flatten().map(|s| s["pitch_offsets"].as_array().into_iter().flatten().map(|v| v.as_i64().unwrap_or(0) as i32).collect()).collect();
        let slots_joints = read("rules.json")["bone_slots"]["joints"].as_array().into_iter().flatten().map(|v| v.as_str().unwrap_or("").to_string()).collect();
        let data = EffectsData::load(assets);
        if data.is_none() {
            log::warn!("no pictures/effects/effects.json: no hit sparks (convert with export_effects)");
        }
        Self { data, slots: [None, None, None], next: 0, grounds: [None, None, None], next_ground: 0, particles: Vec::new(), light: None, scripts, shake: None, seed: 1, slots_joints }
    }

    /// PsyQ `rand()`.
    fn rand(&mut self) -> u32 {
        self.seed = self.seed.wrapping_mul(0x41C6_4E6D).wrapping_add(12345);
        (self.seed >> 16) & 0x7FFF
    }

    /// Two rand()s as one 30-bit value (R = r1 | r2 << 16).
    fn rand2(&mut self) -> u32 {
        let a = self.rand();
        a | self.rand() << 16
    }

    /// The camera pitch offset of the running shake (4096 units per turn), added to the view.
    pub fn shake_pitch(&self) -> i32 {
        self.shake.and_then(|(s, f)| self.scripts.get(s)?.get(f).copied()).unwrap_or(0)
    }

    /// The hit light now: colour and strength (1 when lit, fading to 0 over 48 frames), and its position.
    pub fn hit_light(&self) -> Option<([f32; 3], f32, Vec3)> {
        self.light.map(|(c, t, p)| (c, t as f32 / LIGHT_FRAMES as f32, p))
    }

    pub fn clear(&mut self) {
        self.slots = [None, None, None];
        self.grounds = [None, None, None];
        self.particles.clear();
        self.light = None;
        self.shake = None;
    }

    /// A hit-spark slot (the 4th overwrites the oldest). `dir`: the strike direction (moving sparks only).
    fn sprite(&mut self, strip: &Strip, pos: Vec3, dir: Option<Vec3>, blend: Blend, eye: Vec3) {
        if strip.cells.is_empty() {
            return;
        }
        let vel = dir.map_or(Vec3::ZERO, |d| d.normalize_or_zero() * SPARK_SPEED);
        // the strike direction picks a horizontal flip so the streaks point along the hit
        let d = dir.unwrap_or(Vec3::ZERO);
        let mirror = d.x * (pos.z - eye.z) - d.z * (pos.x - eye.x) < 0.0;
        let pos = Vec3::new(pos.x, pos.y.max(MIN_HEIGHT), pos.z);
        self.slots[self.next] = Some(SpriteFx { pos, vel, age: 1, strip: strip.clone(), blend, fade: blend == Blend::Add, mirror });
        self.next = (self.next + 1) % self.slots.len();
    }

    /// A ground impact at a root (x, z): 6 dust puffs 250 mm up.
    fn ground(&mut self, root: Vec3) {
        let pos = Vec3::new(root.x, MIN_HEIGHT, root.z);
        let Some(d) = &self.data else { return };
        let first = SpriteFx { pos: pos + d.ground.first().copied().unwrap_or_default(), vel: Vec3::ZERO, age: 1, strip: d.dust.clone(), blend: Blend::Half, fade: false, mirror: false };
        self.grounds[self.next_ground] = Some(Ground { pos, age: 0, puffs: vec![first] });
        self.next_ground = (self.next_ground + 1) % self.grounds.len();
    }

    /// The character spark of model `model` at `pos`, with its hit light.
    fn char_spark(&mut self, model: usize, pos: Vec3, eye: Vec3) {
        let Some(c) = self.data.as_ref().and_then(|d| d.characters.get(&model)).cloned() else { return };
        self.sprite(&c.strip, pos, None, Blend::Add, eye);
        self.light = Some((c.light, LIGHT_FRAMES, pos));
    }

    /// A timed move effect at a position (ops 6..0xA) outside a fight (the embu): `own` / `other` are the model
    /// records whose character sparks ops 6 / 7 show.
    pub fn effect_at(&mut self, op: u32, pos: Vec3, own: usize, other: usize, eye: Vec3) {
        let Some(data) = self.data.clone() else { return };
        match op {
            6 => self.char_spark(own, pos, eye),
            7 => self.char_spark(other, pos, eye),
            8 => self.sprite(&data.spark, pos, None, Blend::Add, eye),
            9 => self.sprite(&data.zero, pos, None, Blend::Add, eye),
            _ => self.sprite(&data.dust, pos, None, Blend::Half, eye),
        }
    }

    /// A ground impact at a root (metres) outside a fight.
    pub fn ground_at(&mut self, root: Vec3) {
        self.ground(root);
    }

    /// Advance the effects one frame without a fight (the embu).
    pub fn tick(&mut self) {
        self.advance();
    }

    /// The heavy-hit particles along the strike (0x8006DF34): 6 easing ones and 20 fast straight ones.
    fn heavy(&mut self, at: Vec3, strike: Vec3, ki: bool) {
        let d = strike.normalize_or_zero();
        let (yaw, pitch) = (d.x.datan2(d.z), d.y.datan2(glam::Vec2::new(d.x, d.z).length()));
        let unit = std::f32::consts::TAU / 4096.0;
        let ramp = usize::from(ki);
        let size = if ki { 0.070 } else { 0.110 };
        let heading = |yaw: f32, pitch: f32| Vec3::new(pitch.dcos() * yaw.dsin(), pitch.dsin(), pitch.dcos() * yaw.dcos());
        for _ in 0..6 {
            let r = self.rand2();
            let dy = ((r & 0x7FF) as f32 - 0x3FF as f32) * 2.0 / 3.0 * unit;
            let r2 = self.rand2();
            let dp = ((r2 & 0x7FF) as f32 - 0x3FF as f32) * 2.0 / 3.0 * unit;
            let v0 = ((r >> 6) & 0x1FF) as f32;
            let v1 = ((r >> 8) & 0x1FF) as f32 + 0x680 as f32;
            let life = ((r >> 4) & 31) + 10;
            self.particles.push(Particle { pos: at, dir: heading(yaw + dy, pitch + dp), v0, v1, easing: true, age: 0, life, ramp, size });
        }
        for _ in 0..20 {
            let r = self.rand2();
            let off = |s: u32| ((r >> s) & 0xFF) as f32 - 127.0;
            let jitter = Vec3::new(off(0), off(8), off(16)) / UNITS;
            let r2 = self.rand2();
            let dy = ((r2 & 0x7F) as f32 - 63.0) * unit;
            let dp = (((r2 >> 7) & 0x7F) as f32 - 63.0) * unit;
            let speed = ((r2 >> 12) & 0x7FF) as f32 * 5.0 / 4.0;
            let life = ((r2 >> 10) & 31) + 20;
            let dir = heading(yaw + dy, pitch + dp);
            // the first frame jumps about 8 frames of travel
            let pos = at + jitter + dir * speed / 16.0 / UNITS * 8.0;
            self.particles.push(Particle { pos, dir, v0: speed, v1: speed, easing: false, age: 0, life, ramp, size });
        }
        if self.particles.len() > 80 {
            let extra = self.particles.len() - 80;
            self.particles.drain(..extra); // the pool holds 80
        }
    }

    /// One 60 Hz step: the effects move (each is drawn before it moves), then the last fight step's hits, move
    /// effects and landings spawn new ones.
    pub fn step(&mut self, game: &Game) {
        let (eye, _, _) = crate::view_camera(&game.camera.view);
        let models: Vec<usize> = (0..4).map(|k| game.body_of(k).map_or(0, |b| b.model_record)).collect();
        let fight = &game.fight;
        // (the co-op partner's move effects too)
        let mut actors = vec![(&fight.fighters[0], 0, fight.opponent_index(0)), (&fight.fighters[1], 1, fight.opponent_index(1))];
        if let Some(p) = fight.partner.as_deref() {
            actors.push((p, 3, fight.opponent_index(3)));
        }
        self.step_actors(
            &fight.hits,
            &models,
            &actors,
            eye,
            &|i, node| {
                let (pose, place) = game.pose_actor(i)?;
                Some(place.transform_point3(pose.get(node)?.w_axis.truncate()))
            },
            &|i, name| game.body_of(i)?.character.nodes.iter().position(|n| n.name == name),
        );
    }

    /// The same for any number of actors: `models[k]` actor k's model record; `actors` (fighter, own index,
    /// opponent index) whose move effects play; `at(k, node)` a node's world position (metres) in actor k's pose
    /// shown, `node(k, joint)` the node of a joint name.
    pub fn step_actors(
        &mut self,
        hits: &[kumite_sim::HitEvent],
        models: &[usize],
        actors: &[(&kumite_sim::Fighter, usize, usize)],
        eye: Vec3,
        at: &dyn Fn(usize, usize) -> Option<Vec3>,
        node: &dyn Fn(usize, &str) -> Option<usize>,
    ) {
        self.advance();
        let Some(data) = self.data.clone() else { return };
        for h in hits {
            let pos = h.point / UNITS;
            let strike = Vec3::new(h.strike.x, h.strike.y, h.strike.z);
            if h.blocked {
                self.sprite(&data.block, pos, None, Blend::Add, eye);
            } else if h.base_damage == 0 {
                self.sprite(&data.zero, pos, None, Blend::Add, eye);
            } else {
                // every hit: the red spark thrown along the strike (0x80076C28)
                self.sprite(&data.spark, pos, Some(strike), Blend::Add, eye);
                if h.damage.max(h.base_damage) >= HEAVY {
                    self.char_spark(models.get(h.attacker).copied().unwrap_or(0), pos, eye);
                    // fire / projectile attacks (limb bytes 0x18..0x1C) have their own effects instead
                    if !(0x18..=0x1C).contains(&h.limb) {
                        self.heavy(pos, strike, h.ki);
                    }
                }
            }
        }
        for &(f, i, o) in actors {
            for e in &f.effects {
                match *e {
                    MoveEffect::Shake(n) => self.shake = Some((n as usize, 0)),
                    MoveEffect::GroundImpact | MoveEffect::LandingImpact => self.ground(f.root() / UNITS),
                    MoveEffect::AtBone { op, slot } => {
                        let Some(p) = self.slots_joints.get(slot as usize).and_then(|name| node(i, name)).and_then(|n| at(i, n)) else { continue };
                        match op {
                            6 => self.char_spark(models[i], p, eye),
                            7 => {
                                // (an opponent past the four bodies: no spark, as its sound is skipped)
                                if let Some(&m) = models.get(o) {
                                    self.char_spark(m, p, eye);
                                }
                            }
                            8 => self.sprite(&data.spark, p, None, Blend::Add, eye),
                            9 => self.sprite(&data.zero, p, None, Blend::Add, eye),
                            _ => self.sprite(&data.dust, p, None, Blend::Half, eye),
                        }
                    }
                }
            }
        }
    }

    /// Move everything one frame.
    fn advance(&mut self) {
        for s in self.slots.iter_mut() {
            if let Some(fx) = s {
                fx.age += 1;
                fx.vel *= DRAG;
                fx.pos += fx.vel;
                fx.pos.y = fx.pos.y.max(MIN_HEIGHT);
                if fx.age > fx.strip.life.max(1) {
                    *s = None;
                }
            }
        }
        // puff k of a ground impact appears on its 2k-th update (puff 0 with the impact)
        let dust = self.data.as_ref().map(|d| (d.dust.clone(), d.ground.clone()));
        for g in self.grounds.iter_mut().flatten() {
            g.age += 1;
            for p in &mut g.puffs {
                p.age += 1;
            }
            g.puffs.retain(|p| p.age <= p.strip.life.max(1));
            if let Some((strip, offsets)) = &dust {
                let k = g.age as usize / 2;
                if g.age % 2 == 0 && k < offsets.len() {
                    g.puffs.push(SpriteFx { pos: g.pos + offsets[k], vel: Vec3::ZERO, age: 1, strip: strip.clone(), blend: Blend::Half, fade: false, mirror: false });
                }
            }
        }
        for g in self.grounds.iter_mut() {
            if g.as_ref().is_some_and(|g| g.age > 12 && g.puffs.is_empty()) {
                *g = None;
            }
        }
        for p in &mut self.particles {
            p.age += 1;
            let t = p.age as f32 / p.life.max(1) as f32;
            let s = if p.easing { p.v0 + (p.v1 - p.v0) * (0.25 + 0.5 * (std::f32::consts::FRAC_PI_2 * t).dcos()) } else { p.v0 };
            p.pos += p.dir * s / 16.0 / UNITS;
        }
        self.particles.retain(|p| p.age < p.life && p.pos.y > 0.0);
        if let Some((_, t, _)) = &mut self.light {
            *t = t.saturating_sub(1);
        }
        if self.light.is_some_and(|l| l.1 == 0) {
            self.light = None;
        }
        if let Some((s, f)) = &mut self.shake {
            *f += 1;
            if self.scripts.get(*s).is_none_or(|v| *f >= v.len()) {
                self.shake = None;
            }
        }
    }

    /// The atlas size in cells (columns, rows).
    pub fn atlas(&self) -> (u32, u32) {
        self.data.as_ref().map_or((1, 1), |d| (d.cols, d.rows))
    }

    /// What to draw this frame.
    pub fn sprites(&self) -> Vec<Sprite> {
        let mut out = Vec::new();
        let sprite = |fx: &SpriteFx| {
            let cell = fx.strip.cells[(fx.age as usize - 1).min(fx.strip.cells.len() - 1)];
            // the last 10 frames dim linearly: 128 - k * 9.6, over 128
            let k = fx.age as i32 - fx.strip.life as i32 + 10;
            let level = if fx.fade && k > 0 { (128.0 - (k as f32 * 9.6).floor()) / 128.0 } else { 1.0 };
            let alpha = if fx.blend == Blend::Half { 0.5 } else { 1.0 };
            Sprite { pos: fx.pos, size: SPRITE, cell, color: [level, level, level, alpha], blend: fx.blend, mirror: fx.mirror, nearer: SORT_NEARER }
        };
        out.extend(self.slots.iter().flatten().map(sprite));
        for g in self.grounds.iter().flatten() {
            out.extend(g.puffs.iter().map(sprite));
        }
        if let Some(d) = &self.data {
            for p in &self.particles {
                let ramp = &d.heavy[p.ramp];
                let i = (p.age as usize * 16 / p.life.max(1) as usize).min(ramp.len().saturating_sub(1));
                if let Some(&cell) = ramp.get(i) {
                    out.push(Sprite { pos: p.pos, size: p.size, cell, color: [1.0; 4], blend: Blend::Add, mirror: false, nearer: 0.0 });
                }
            }
        }
        out
    }
}
