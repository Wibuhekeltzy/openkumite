//! Ball (battle type 7; ball research): the ball (record 8's struct and state machine 0x800B11DC), its
//! flight, the fighters' attacks on it (EXE 0x80047D98), its hits on the fighters' bodies (0x80047F14), the
//! points, and the Ball rules for the fighters (no damage between them, each in its own half of the court, the
//! back-step push table). Worked in the game's own units and axes (x, y down, z) with its integer arithmetic; the
//! simulation's axes are (x, −y, −z).
//!
//! 2 ON 2 (OpenKumite's own): the four of [`Fight`]'s co-op melee on the court, the players' side actors 0 and 3,
//! the other 1 and 2 ([`side_of`]). Each side keeps its one gauge; whoever hits the ball aims it at the foe it
//! faces; a point costs the side's nearest standing player. With two (actors 0, 1) everything is as the original.

use crate::detmath::DetMath;
use crate::{posture, segment_hits, BallPush, Fight, GameData, Sphere, ONE};
use glam::Vec3;
use serde::Deserialize;

/// One ball type (record 8 0x800B5F9C).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct BallType {
    pub speed_unit: i32,
    pub lob_unit: i32,
    pub gravity: i32,
    pub ambient: [u8; 3],
    pub damage_percent: i32,
    pub zone: i32,
    pub level: String,
    pub name: String,
    /// Face colours: triangles [face & 7], quads [8 + (face & 7)].
    pub colours: Vec<[u8; 3]>,
}

/// The ball mesh: a UV sphere of radius 384 (game units).
#[derive(Clone, Debug, Deserialize)]
pub struct BallMesh {
    pub vertices: Vec<[i32; 3]>,
    pub normals: Vec<[i32; 3]>,
    pub triangles: Vec<[usize; 3]>,
    pub quads: Vec<[usize; 4]>,
}

/// gamedata/ball.json.
#[derive(Clone, Debug, Deserialize)]
pub struct BallData {
    pub types: Vec<BallType>,
    pub mesh: BallMesh,
    pub body_offsets: Vec<[i32; 3]>,
}

const RADIUS: f32 = 608.0;
const REST_Y: i32 = -0x180;
/// The fighters' start x (∓5120) and court (0x800430FC).
pub const FIGHTER_X: f32 = 5120.0;
const COURT_Z: f32 = 256.0;
/// 2 ON 2: the court is deeper, so partners can stand side by side (OpenKumite's own).
pub const COURT_Z_DOUBLES: f32 = 1536.0;

/// The court side of actor `i` ([`Fight::actor`] indices): the players' 0 (actors 0, 3), the other 1 (1, 2).
pub fn side_of(i: usize) -> usize {
    usize::from(i == 1 || i == 2)
}

/// sin of a 4096-per-turn angle, 4096 = 1.0 (table 0x8001E8C4).
pub fn sin12(a: i32) -> i32 {
    ((a & 0xFFF) as f64 * std::f64::consts::TAU / 4096.0).dsin().mul_add(4096.0, 0.0).round() as i32
}
pub fn cos12(a: i32) -> i32 {
    sin12(a + 0x400)
}
/// The engine's ratan2: the angle with dx = cos·r, dz = sin·r, −2048..2048.
pub fn atan2_12(dx: i32, dz: i32) -> i32 {
    if dx == 0 && dz == 0 {
        return 0;
    }
    ((dz as f64).datan2(dx as f64) * 4096.0 / std::f64::consts::TAU).round() as i32
}
fn isqrt(v: i64) -> i64 {
    (v.max(0) as f64).sqrt() as i64
}

/// What the ball did this frame, for sounds and pictures.
#[derive(Clone, Debug, PartialEq)]
pub enum BallEvent {
    /// A system sound (SePlay code).
    Sound(u16),
    /// A floating text at the ball (text, colour).
    Text(String, u32),
    /// Screen shake (0 small, 1 big).
    Shake(u8),
    /// The big Attack Ball burned a fighter (side): its spark.
    Burn(usize),
    /// A landing in a scoring area: the impact sparks.
    Impact,
}

#[derive(Clone, Debug)]
pub struct Ball {
    pub kind: BallType,
    offsets: Vec<[i32; 3]>,
    pub state: u8,
    pub pos: [i32; 3],
    pub vel: [i32; 3],
    pub g: i32,
    pub spin: [i32; 3],
    spin_rate: [i32; 3],
    pub speed: i32,
    pub heading: i32,
    pub homing: bool,
    pub cooldown: u8,
    just_hit: bool,
    /// Body-test segments: (previous, current) of position + offset.
    segs: [[[i32; 3]; 2]; 4],
    pub owner: usize,
    pub receiver: usize,
    smash_target: usize,
    /// Each side's accumulated charge (the gauges), the ball's power (0 = harmless).
    pub charge: [i32; 2],
    pub power: i32,
    smash_speed: i32,
    rally: i32,
    hit_charged: bool,
    /// Visual mode 0..9 (0x800B2A4C) and the state timer.
    pub visual: u8,
    pub timer: i32,
    pub squash: i32,
    pub squash_axis: [i32; 3],
    resting: i32,
    bump_cooldown: i32,
    /// The last hitter's move (the third player struct's frozen copy): who, and its damage.
    p3: Option<usize>,
    /// The KO replay passes (2; 6 after an IRON power K.O.).
    pub replay_passes: u32,
    pub events: Vec<BallEvent>,
}

impl Ball {
    /// A new round's ball (0x800B07C8 / reset 0x800B0B14): it waits over the server's side.
    pub fn new(data: &BallData, kind: usize, server: usize, first_round: bool) -> Self {
        // (no types in the data: a placeholder ball; the app offers no Ball then)
        let kind = data.types.get(kind).or(data.types.first()).cloned().unwrap_or_default();
        let offsets = data.body_offsets.clone();
        let pos = [if server == 1 { 0xD00 } else { -0xD00 }, -0x500, 0];
        let g = kind.gravity;
        let mut b = Self {
            kind,
            offsets,
            state: 0,
            pos,
            vel: [0; 3],
            g,
            spin: [0; 3],
            spin_rate: [0x20, 0x10, 4],
            speed: 0,
            heading: 0,
            homing: false,
            cooldown: 0,
            just_hit: false,
            segs: [[[0; 3]; 2]; 4],
            owner: server,
            receiver: 1 - server,
            smash_target: 1 - server,
            charge: [0; 2],
            power: 0,
            smash_speed: 0,
            rally: 0,
            hit_charged: false,
            visual: 0,
            timer: if first_round { 100 } else { 40 },
            squash: 0,
            squash_axis: [0; 3],
            resting: 0,
            bump_cooldown: 0,
            p3: None,
            replay_passes: 2,
            events: Vec::new(),
        };
        b.rebuild_segments();
        b.rebuild_segments();
        b
    }

    fn rebuild_segments(&mut self) {
        for (i, o) in self.offsets.iter().enumerate().take(4) {
            self.segs[i][0] = self.segs[i][1];
            self.segs[i][1] = [self.pos[0] + o[0], self.pos[1] + o[1], self.pos[2] + o[2]];
        }
    }

    /// The ball's centre in the simulation's axes.
    pub fn position(&self) -> Vec3 {
        to_sim(self.pos)
    }

    /// Drawn this frame (states 1..9).
    pub fn visible(&self) -> bool {
        self.state != 0
    }
}

pub fn to_sim(p: [i32; 3]) -> Vec3 {
    Vec3::new(p[0] as f32, -p[1] as f32, -p[2] as f32)
}

/// A fighter's position in the game's axes (P+0xF68: the root bone).
fn game_pos(f: &crate::Fighter) -> [i32; 3] {
    let r = f.root();
    [r.x.round() as i32, (-r.y).round() as i32, (-r.z).round() as i32]
}

/// The ball as the CPU sees it (record 8 0x800B4494): the fighters' and the ball's positions along the line
/// from P1 to P2 (u), and the ball's offset across it (v); game units.
#[derive(Clone, Copy, Debug)]
pub struct BallSight {
    pub u: [i32; 2],
    pub ub: i32,
    pub vb: i32,
}

impl BallSight {
    /// The distance from side `me` to the ball (0x800B4644).
    pub fn distance(&self, me: usize) -> i32 {
        let du = (self.u[me] - self.ub) as i64;
        isqrt(du * du + self.vb as i64 * self.vb as i64) as i32
    }
    /// The ball is behind side `me` (0x800B46A0).
    pub fn behind(&self, me: usize) -> bool {
        if me == 0 {
            self.ub < self.u[0]
        } else {
            self.u[1] < self.ub
        }
    }
    /// The ball is just in front of side `me`, within 2000 (0x800B4720).
    pub fn in_front(&self, me: usize) -> bool {
        if me == 0 {
            self.u[0] < self.ub && self.ub < self.u[0] + 2000
        } else {
            self.u[1] - 2000 < self.ub && self.ub < self.u[1]
        }
    }
}

impl Fight {
    /// The CPU's view of the ball (0x800B4494), when there is one in play.
    pub fn ball_sight(&self) -> Option<BallSight> {
        self.ball_sight_pair(0, 1)
    }

    /// The view of actor `i` (2 ON 2): along the line between it and the foe it faces, ordered as their sides
    /// on the screen.
    pub fn ball_sight_for(&self, i: usize) -> Option<BallSight> {
        if !self.ball_doubles() {
            return self.ball_sight();
        }
        let o = self.opponent_index(i);
        let (me, op) = (self.actor(i)?, self.actor(o)?);
        if me.side <= op.side {
            self.ball_sight_pair(i, o)
        } else {
            self.ball_sight_pair(o, i)
        }
    }

    /// The ball projected on the line from actor `a` to actor `b`.
    fn ball_sight_pair(&self, a: usize, b: usize) -> Option<BallSight> {
        let ball = self.ball.as_ref().filter(|b| b.visible())?;
        let (p1, p2) = (game_pos(self.actor(a)?), game_pos(self.actor(b)?));
        let (dx, dz) = (p2[0] - p1[0], p2[2] - p1[2]);
        if dx == 0 {
            return None;
        }
        let a = -atan2_12(dx, dz) & 0xFFF;
        let (c, s) = (cos12(a) as i64, sin12(a) as i64);
        let z0 = p1[2] as i64 - dz as i64 * p1[0] as i64 / dx as i64;
        let along = |p: [i32; 3]| ((p[0] as i64 * c - (p[2] as i64 - z0) * s) / 4096) as i32;
        let across = |p: [i32; 3]| ((p[0] as i64 * s + (p[2] as i64 - z0) * c) / 4096) as i32;
        Some(BallSight { u: [along(p1), along(p2)], ub: along(ball.pos), vb: across(ball.pos) })
    }

    /// The actors on the court: the two fighters, or (2 ON 2) the four, the players' side first.
    pub fn ball_actors(&self) -> Vec<usize> {
        if self.ball_doubles() {
            vec![0, 3, 1, 2]
        } else {
            vec![0, 1]
        }
    }

    /// Whom actor `i` sends the ball to: the other fighter, or (2 ON 2) the foe it faces.
    fn ball_target(&self, i: usize) -> usize {
        if self.ball_doubles() {
            self.opponent_index(i)
        } else {
            1 - i.min(1)
        }
    }

    /// Actor `i`'s place (game axes).
    fn pos_of(&self, i: usize) -> [i32; 3] {
        self.actor(i).map_or([0; 3], game_pos)
    }

    /// A side's place: its fighter's, or (2 ON 2) the middle of its two.
    fn side_pos(&self, side: usize) -> [i32; 3] {
        let members: Vec<[i32; 3]> = self.ball_actors().into_iter().filter(|&i| side_of(i) == side).map(|i| self.pos_of(i)).collect();
        let n = members.len().max(1) as i32;
        let sum = members.iter().fold([0; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
        [sum[0] / n, sum[1] / n, sum[2] / n]
    }

    /// What the ball's motion needs of the court: the z band, the receiver's place and height.
    fn court_view(&self, receiver: usize) -> CourtView {
        let band = if self.ball_doubles() {
            let z = COURT_Z_DOUBLES as i32;
            (-z - 0x80, z + 0x80)
        } else {
            let zc = (self.pos_of(0)[2] + self.pos_of(1)[2]) / 2;
            (zc - 0x80, zc + 0x80)
        };
        let ry = self.actor(receiver).map_or(0, |f| -(f.root().y as i32));
        CourtView { band, receiver: self.pos_of(receiver), receiver_y: ry }
    }

    /// The Ball rules for the fighters after their logic (0x800430FC, 0x8003F554): the back-step pushes
    /// (ball.json `constants.moves`), then each fighter kept in its own half of the court.
    pub(crate) fn ball_fighters(&mut self) {
        let court_z = if self.ball_doubles() { COURT_Z_DOUBLES } else { COURT_Z };
        let gd = self.data.clone();
        for i in self.ball_actors() {
            let Some(f) = self.actor_mut(i) else { continue };
            let Some(&BallPush { push, frames: [from, to], .. }) = gd.ball.pushes.iter().find(|p| p.aliases.contains(&f.alias)) else { continue };
            if push > 0.0 && (from..=to).contains(&f.frame) {
                // backwards: the facing turned half a turn
                f.position -= Vec3::new(f.yaw.dsin(), 0.0, f.yaw.dcos()) * push;
            }
        }
        for i in self.ball_actors() {
            let Some(f) = self.actor_mut(i) else { continue };
            let (lo, hi) = if side_of(i) == 0 { (-FIGHTER_X, 0.0) } else { (0.0, FIGHTER_X) };
            f.position.x = f.position.x.clamp(lo, hi);
            f.position.z = f.position.z.clamp(-court_z, court_z);
        }
    }

    /// The ball hook (0x800B094C), after the fighters' hit tests.
    pub(crate) fn ball_step(&mut self, gd: &GameData, bodies: &[&crate::BodyVolumes]) {
        let Some(mut b) = self.ball.take() else { return };
        // (the Ball rules hold while the ball is out of its slot)
        let rules = std::mem::replace(&mut self.ball_rules, true);
        b.events.clear();
        if b.bump_cooldown > 0 {
            b.bump_cooldown -= 1;
        }
        if !b.cooldown_active() {
            b.hit_charged = false;
        }
        self.ball_machine(gd, bodies, &mut b);
        b.visual_state();
        self.ball = Some(b);
        self.ball_rules = rules;
    }

    fn ball_machine(&mut self, gd: &GameData, bodies: &[&crate::BodyVolumes], b: &mut Ball) {
        match b.state {
            0 => {
                b.timer -= 1;
                if b.timer <= 0 {
                    (b.state, b.timer) = (8, 32);
                    b.events.push(BallEvent::Sound(gd.ball.sounds.serve));
                }
                return;
            }
            1 | 8 => {
                b.timer -= 1;
                if b.timer <= 0 {
                    b.state = if b.state == 8 { 9 } else { 2 };
                    (b.speed, b.cooldown, b.heading, b.homing) = (0, 0, 0, false);
                    self.place_on_axis(b);
                }
                return;
            }
            7 => {
                (b.power, b.rally) = (0, 0);
                b.timer -= 1;
                if b.timer <= 0 {
                    (b.state, b.timer) = (6, 16);
                }
                return;
            }
            6 => {
                (b.power, b.rally) = (0, 0);
                b.timer -= 1;
                if b.timer <= 0 {
                    (b.state, b.timer) = (1, 32);
                    let zc = (self.side_pos(0)[2] + self.side_pos(1)[2]) / 2;
                    b.pos = [0, -0xE40, zc];
                    (b.vel, b.spin_rate) = ([0; 3], [0; 3]);
                    b.g = b.kind.gravity / 2;
                    b.rebuild_segments();
                    b.rebuild_segments();
                    if self.state == crate::RoundState::Fighting {
                        b.events.push(BallEvent::Sound(gd.ball.sounds.drop));
                    }
                }
                return;
            }
            _ => {}
        }
        self.ball_flight(gd, bodies, b);
    }

    /// States 1 / 8 end: the ball on the line between the fighters (0x800B11DC).
    fn place_on_axis(&self, b: &mut Ball) {
        let (p1, p2) = (self.side_pos(0), self.side_pos(1));
        let (x1, x2) = (p1[0].min(0), p2[0].max(0));
        if x2 != x1 {
            b.pos[2] = p1[2] + ((b.pos[0] - x1) as i64 * (p2[2] - p1[2]) as i64 / (x2 - x1) as i64) as i32;
        }
    }

    /// Flight and ground (states 2..5 and 9, 0x800B127C).
    fn ball_flight(&mut self, gd: &GameData, bodies: &[&crate::BodyVolumes], b: &mut Ball) {
        let g_type = b.kind.gravity;
        let mut skip_tests = false;
        if b.state == 4 {
            if b.pos[1] < -0x1400 {
                // the smash's apex: down fast at the target
                b.pos[1] = -0x1400;
                (b.state, b.g, b.receiver, b.speed) = (2, g_type, b.smash_target, b.smash_speed);
                b.vel[1] = (2 * b.smash_speed) / 3 + 0x1000;
                skip_tests = true;
            }
        } else if b.pos[1] < -0x2000 {
            b.pos[1] = -0x2000;
            (b.vel[1], b.g) = (0, g_type);
            skip_tests = true;
        }
        if !skip_tests {
            let airborne = b.pos[1] < REST_Y && b.pos[0].abs() <= 0x2800;
            if !airborne {
                b.pos[1] = REST_Y;
                b.g = g_type;
                if b.vel[1] > 0xD00 {
                    b.events.push(BallEvent::Sound(gd.ball.sounds.bounce));
                }
                if b.kind.gravity > 0x200 {
                    if b.vel[1] > 0x3000 {
                        b.events.push(BallEvent::Shake(1));
                    } else if b.vel[1] > 0x1800 {
                        b.events.push(BallEvent::Shake(0));
                    }
                }
                if b.pos[0].abs() > b.kind.zone {
                    // a point: the ball fell on a side
                    (b.vel[1], b.speed) = (0, 0);
                    if self.state != crate::RoundState::Fighting || b.state == 5 {
                        (b.state, b.timer) = (6, 16);
                    } else {
                        let side = usize::from(b.pos[0] >= 0);
                        b.charge[side] = 0;
                        let v = self.point_loser(side, b.pos);
                        if let Some(f) = self.actor_mut(v) {
                            f.health = (f.health - 30 * ONE).max(0);
                        }
                        (b.state, b.timer) = (7, 5);
                        b.events.push(BallEvent::Impact);
                        if self.link.is_none() {
                            // the forced hit reaction, from the ball's spot, with the last hitter's move
                            let attacker = b.p3.unwrap_or(self.ball_target(v));
                            self.ball_strike(gd, v, attacker, 0, to_sim(b.pos), false);
                        }
                    }
                    b.move_on(&self.court_view(b.receiver), self.frame);
                    return;
                }
                // the net zone: it bounces
                b.state = 2;
                b.vel[1] = -b.vel[1] >> 1;
                for r in &mut b.spin_rate {
                    *r >>= 1;
                }
            }
            if b.pos[1] >= -0x189 {
                b.resting += 1;
                if b.resting >= 600 {
                    (b.state, b.timer) = (6, 16);
                    b.move_on(&self.court_view(b.receiver), self.frame);
                    return;
                }
            } else {
                b.resting = 0;
            }
            if !b.homing {
                b.speed = (b.speed - 0x20).max(0);
            }
            if b.cooldown > 0 {
                b.cooldown -= 1;
            } else if !self.ball_attacked(gd, b) && !self.ball_actors().into_iter().any(|i| self.actor(i).is_some_and(|f| f.locked)) {
                self.ball_body(gd, bodies, b);
            }
        }
        b.move_on(&self.court_view(b.receiver), self.frame);
    }

    /// Who a point on `side` costs: its fighter, or (2 ON 2) the one of its two standing nearest the ball.
    fn point_loser(&self, side: usize, at: [i32; 3]) -> usize {
        let members: Vec<usize> = self.ball_actors().into_iter().filter(|&i| side_of(i) == side).collect();
        let standing: Vec<usize> = members.iter().copied().filter(|&i| self.actor(i).is_some_and(|f| f.health > 0)).collect();
        let pool = if standing.is_empty() { &members } else { &standing };
        let d = |i: usize| {
            let p = self.pos_of(i);
            let (dx, dz) = ((p[0] - at[0]) as i64, (p[2] - at[2]) as i64);
            dx * dx + dz * dz
        };
        pool.iter().copied().min_by_key(|&i| d(i)).unwrap_or(side)
    }

    /// The fighters' attack segments against the ball (0x80047D98): returns whether one hit it.
    fn ball_attacked(&mut self, gd: &GameData, b: &mut Ball) -> bool {
        let centre = to_sim(b.pos);
        let sphere = Sphere { centre, radius: RADIUS, zone: 0 };
        // who hit it this frame, the players' side first (with two: P1 before P2)
        let hitters: Vec<(usize, Vec3)> =
            self.ball_actors().into_iter().filter_map(|i| self.actor(i)?.attack_segments(gd).into_iter().find(|(p, q)| segment_hits(*p, *q, &sphere)).map(|(p, q)| (i, q - p))).collect();
        let Some(&(h, d)) = hitters.first() else { return false };
        let both = hitters.iter().any(|&(i, _)| side_of(i) != side_of(h));
        let o = self.ball_target(h);
        let Some(f) = self.actor(h) else { return false };
        if f.connected {
            return true;
        }
        let mv = f.current_move(gd);
        let (dmg, mask) = (if mv.damage_type == 1 { 0 } else { mv.damage }, mv.attack_mask.mask);
        let airborne = f.posture(gd) & posture::AIRBORNE != 0 || f.air.is_some() || f.airborne_flag(gd);
        let (moveset, alias) = (f.moveset.moveset_id, f.alias);
        let (spd, lob, g) = (b.kind.speed_unit, b.kind.lob_unit, b.kind.gravity);
        (b.owner, b.receiver, b.homing) = (h, o, false);
        b.p3 = Some(h);
        if let Some(fh) = self.actor_mut(h) {
            fh.connected = true;
            fh.landed = Some(true);
        }
        b.cooldown = 5;
        let (p, q) = (self.pos_of(o), b.pos);
        b.heading = atan2_12(p[0] - q[0], p[2] - q[2]);
        b.squash_axis = [0, atan2_12(d.x as i32, -d.z as i32), atan2_12(d.x as i32, -d.y as i32)];
        b.just_hit = true;
        b.squash = dmg * 8 + 0x140;
        let dist = self.ball_distance(h, o);
        let ox = q[0].abs();
        let target_z = p[2];
        if both {
            // both at once: straight up
            (b.power, b.speed, b.state, b.vel[1], b.g) = (0, 0, 2, -37 * g, g);
        } else if gd.ball.self_charge.is_some_and(|(m, a)| moveset == Some(m) && alias != a) && matches!(mask, 0x607 | 0x706) {
            // the self-stab: he takes the other side's charge
            let c = b.charge[side_of(o)];
            if let Some(fh) = self.actor_mut(h) {
                fh.health = (fh.health - c * ONE).max(0);
            }
            b.charge = [0; 2];
            (b.state, b.timer) = (7, 5);
        } else if dmg == 0 && b.power > 0 {
            // "!": the charged ball pops up, ready to be smashed
            b.state = 3;
            (b.vel[1], b.g) = (-37 * g, g);
            b.smash_speed = (b.speed & 0xFFFF) + 5 * spd;
            b.speed = (8 * lob + ox - b.speed / 4).max(0);
            b.events.push(BallEvent::Text("!".into(), 1));
            b.power = 0;
            b.cooldown += 2;
            b.events.push(BallEvent::Sound(gd.ball.sounds.pop));
        } else if dmg == 0 {
            (b.vel[1], b.g) = (-37 * g, g);
            b.speed = (10 * lob + ox - b.speed / 4).max(0);
            (b.state, b.power) = (2, 0);
        } else if matches!(mask, 0x607 | 0x706) {
            // the spike
            b.homing = true;
            b.speed = 60 * spd;
            b.g = g / 4;
            b.power = b.charge[side_of(h)];
            b.state = 2;
            b.vel[1] = target_z - 2 * q[1] - dist;
            b.events.push(BallEvent::Text("!".into(), 2));
        } else if airborne {
            b.charge_up(side_of(h), dmg);
            let s = (dmg + 15).clamp(20, 40);
            b.speed = (s + b.rally) * spd + if b.hit_charged { b.speed / 16 } else { 0 };
            (b.g, b.state, b.homing) = (g / 2, 2, true);
            b.vel[1] = target_z - q[1] - dist;
        } else if matches!(mask, 0x217 | 0x31F) {
            b.charge_up(side_of(h), dmg);
            let s = (dmg + 18).clamp(20, 46);
            b.speed = (s + b.rally) * spd + if b.hit_charged { b.speed / 16 } else { 0 };
            (b.g, b.state, b.homing) = (g / 4, 2, true);
            b.vel[1] = target_z - 2 * q[1] - dist;
        } else if mask == 0x412 && b.state == 3 {
            // the smash
            b.state = 4;
            b.smash_target = o;
            (b.vel[1], b.g) = (-72 * g, g);
            b.speed = 10 * lob;
            b.events.push(BallEvent::Sound(gd.ball.sounds.smash));
            b.charge_up(side_of(h), dmg);
            b.homing = true;
        } else if mask == 0x412 {
            (b.vel[1], b.g) = (-37 * g, g);
            b.speed = ((dmg + 10) * lob + ox - b.speed / 4).max(0);
            (b.state, b.power) = (2, 0);
        } else if matches!(mask, 0x10F | 0x51F) {
            (b.vel[1], b.g) = (-50 * g, g);
            b.speed = (dmg >> 2) * lob + ox;
            (b.state, b.power) = (2, 0);
        }
        true
    }

    /// The distance between the fighters (P1+0xF8): with four, between the hitter and its foe.
    fn ball_distance(&self, a: usize, c: usize) -> i32 {
        let (Some(a), Some(c)) = (self.actor(a), self.actor(c)) else { return 0 };
        let (a, c) = (a.root(), c.root());
        glam::Vec2::new(a.x - c.x, a.z - c.z).length() as i32
    }

    /// The ball's segments against the fighters' hurt spheres (0x80047F14).
    fn ball_body(&mut self, gd: &GameData, bodies: &[&crate::BodyVolumes], b: &mut Ball) {
        let mut hit = None;
        for v in self.ball_actors() {
            let Some(body) = bodies.get(v) else { continue };
            // after the K.O. only the loser can be hit
            if self.state != crate::RoundState::Fighting {
                if let crate::RoundState::Ko { winner } = self.state {
                    if side_of(v) == winner {
                        continue;
                    }
                }
            }
            let found = b.segs.iter().any(|s| {
                let (p, q) = (to_sim(s[0]), to_sim(s[1]));
                body.hurt.iter().any(|sp| segment_hits(p, q, &Sphere { centre: sp.centre, radius: sp.radius, zone: sp.zone }))
            });
            if found {
                hit = Some(v);
                break;
            }
        }
        let Some(v) = hit else { return };
        let o = self.ball_target(v);
        // your own (your side's) Attack Ball passes through you
        let own = side_of(v) == side_of(b.owner);
        if own && b.power != 0 {
            return;
        }
        let g = b.kind.gravity;
        (b.cooldown, b.homing) = (0, false);
        if b.state != 3 {
            b.state = 2;
        }
        let (p, q) = (self.pos_of(o), b.pos);
        b.heading = atan2_12(p[0] - q[0], p[2] - q[2]);
        let dist = self.ball_distance(v, o);
        let t = (dist >> 7) + 12;
        b.vel[1] = ((-g * t) >> 1) - (q[1] + 0x1000);
        b.g = g;
        b.speed = ((if dist > 0x800 { dist - 0x800 } else { 0x400 }) << 8) / t;
        if b.power != 0 && !own {
            // the Attack Ball hits
            let power = b.power;
            (b.owner, b.receiver, b.state) = (v, o, 2);
            b.power = 0;
            let attacker = b.p3.unwrap_or(o);
            let guarding = self.actor(v).is_some_and(|f| f.posture(gd) & posture::GUARDS != 0);
            self.ball_strike(gd, v, attacker, power, to_sim(q), true);
            if !guarding {
                if power >= 100 || b.visual == 9 {
                    b.events.push(BallEvent::Burn(v));
                    if b.kind.gravity > 0x200 {
                        b.replay_passes = 6;
                    }
                }
                (b.speed, b.rally) = (0, 0);
                b.charge = [0; 2];
                (b.state, b.owner, b.receiver, b.cooldown) = (5, v, v, 5);
                (b.vel[1], b.g) = (-37 * g, g / 2);
                b.events.push(BallEvent::Sound(gd.ball.sounds.hit[usize::from(power < 50)]));
            } else {
                b.events.push(BallEvent::Sound(gd.ball.sounds.guarded));
            }
        } else {
            if b.bump_cooldown == 0 {
                b.events.push(BallEvent::Sound(gd.ball.sounds.bump));
                b.bump_cooldown = 10;
            }
            (b.owner, b.receiver) = (v, o);
        }
    }

    /// A ball strike on fighter `v` (the damage pass with the third player struct as the attacker, 0x8004439C):
    /// `damage` off its health unless it guards (`guardable`), and the reaction of the last hitter's move, from
    /// the ball's spot.
    fn ball_strike(&mut self, gd: &GameData, v: usize, attacker: usize, damage: i32, from: Vec3, guardable: bool) {
        let Some(mv) = self.actor(attacker).map(|f| f.current_move(gd).clone()) else { return };
        let Some(fv) = self.actor(v) else { return };
        let victim_posture = fv.posture(gd);
        let to_ball = from - fv.root();
        let facing = Vec3::new(fv.yaw.dsin(), 0.0, fv.yaw.dcos());
        let blocked = guardable && victim_posture & posture::GUARDS != 0 && facing.dot(to_ball) > 0.0;
        let quadrant = crate::direction_quadrant(fv.yaw, fv.root(), from);
        let entry = if mv.reaction.is_throw() { None } else { gd.reactions.entries.get(mv.reaction.index) };
        let param = |k: usize| entry.and_then(|e| e.params.get(k).copied()).unwrap_or(0);
        let st = usize::from(victim_posture & posture::STANDING == 0);
        let (slot, curve, angle) = if blocked { (10 + st, param(6), param(0)) } else { ([0, 4, 8, 6][quadrant] + st, param(1), param(0)) };
        let Some(fv) = self.actor_mut(v) else { return };
        if !blocked {
            fv.health = (fv.health - damage * ONE).max(0);
            fv.hit_flash = 8;
        }
        if let Some(&alias) = entry.and_then(|e| e.victims.get(slot)) {
            if let Some(to) = fv.resolve(alias) {
                fv.start_reaction(to, alias);
            }
        }
        let away = -glam::Vec2::new(to_ball.x, to_ball.z).normalize_or_zero();
        let turn = angle as f32 * std::f32::consts::TAU / 65536.0;
        let direction = glam::Vec2::from_angle(turn).rotate(away);
        fv.pushback = gd.reactions.pushback_curves.entries.iter().find(|c| c.index as i32 == curve).map(|c| crate::Pushback { curve: c.clone(), frame: 0, direction });
        fv.struck = true;
        fv.struck_in_move = true;
        log::info!("P{} {} the ball ({damage}, reaction of {}) -> health {}", v + 1, if blocked { "blocks" } else { "is hit by" }, mv.anim, fv.health / ONE);
    }
}

/// What the ball's motion needs of the court ([`Fight::court_view`]).
struct CourtView {
    /// The z band the ball keeps to.
    band: (i32, i32),
    /// The receiver's place and its root's height (y down).
    receiver: [i32; 3],
    receiver_y: i32,
}

impl Ball {
    fn cooldown_active(&self) -> bool {
        self.cooldown != 0
    }

    /// CHARGE(): the hitter's side's charge grows by the hit's damage times the ball's percent; the ball takes it.
    fn charge_up(&mut self, h: usize, dmg: i32) {
        if self.power != 0 {
            self.hit_charged = true;
            self.rally += 1;
        }
        self.charge[h] += dmg * self.kind.damage_percent / 100;
        self.power = self.charge[h];
        self.events.push(BallEvent::Text(self.power.to_string(), if h == 0 { 2 } else { 5 }));
    }

    /// MOVE: velocity from the heading, integrate, gravity, spin, the z band, homing (0x800B127C's tail).
    fn move_on(&mut self, court: &CourtView, round_frame: u64) {
        self.vel[0] = ((cos12(self.heading) as i64 * self.speed as i64) >> 12) as i32;
        self.vel[2] = ((sin12(self.heading) as i64 * self.speed as i64) >> 12) as i32;
        if self.state == 9 {
            self.pos[1] = (sin12((self.timer * 32) & 0xFE0) >> 4) - 0x500;
            self.timer += 1;
            // a serve nobody hits drops on the server's side after the round's frame 600
            if round_frame >= 600 {
                (self.state, self.vel[1]) = (2, 0);
            }
        } else {
            for k in 0..3 {
                self.pos[k] += self.vel[k] >> 8;
            }
        }
        self.rebuild_segments();
        self.vel[1] += self.g;
        for k in 0..3 {
            self.spin[k] = (self.spin[k] + self.spin_rate[k]) & 0xFFF;
        }
        let z = self.pos[2].clamp(court.band.0, court.band.1);
        self.spin[0] = (self.spin[0] + (z - self.pos[2])) & 0xFFF;
        self.pos[2] = z;
        self.heading = (self.heading + 0x800).rem_euclid(0x1000) - 0x800;
        if self.homing {
            let r = court.receiver;
            let (dx, dz) = (r[0] - self.pos[0], r[2] - self.pos[2]);
            self.heading = atan2_12(dx, dz);
            if self.speed > 0x1000 {
                let d = isqrt(dx as i64 * dx as i64 + dz as i64 * dz as i64) as i32;
                let t = (d / (self.speed >> 8)).max(1);
                // the receiver's bone 1 height (y down)
                let ry = court.receiver_y;
                let e = (self.pos[1] + ((self.vel[1] + t * self.g) >> 8) - ry) << 8;
                let c = (self.vel[1] - e) / t;
                self.vel[1] = (((13 * self.vel[1] + 3 * c) as i64) << 8 >> 12) as i32;
            }
        }
    }

    /// The visual mode and squash (0x800B2A4C).
    fn visual_state(&mut self) {
        if self.homing && self.squash < 0x80 {
            self.just_hit = false;
            self.squash = (2 * self.squash) / 3 + (self.speed >> 6) / 3;
            self.squash_axis[2] += 0x400;
        } else if !self.homing || self.just_hit {
            self.squash -= (self.squash + 16) >> 4;
        }
        self.visual = match self.state {
            1 | 8 => 0,
            6 => 1,
            7 => 2,
            _ if self.power == 0 => {
                if self.cooldown != 0 {
                    5
                } else if self.state == 3 {
                    4
                } else {
                    3
                }
            }
            _ => {
                let mut m = if self.power >= 100 {
                    9
                } else if side_of(self.owner) == 1 {
                    7
                } else {
                    6
                };
                if self.cooldown != 0 {
                    m = if self.hit_charged { 8 } else { 5 };
                }
                m
            }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles() {
        assert_eq!(sin12(0), 0);
        assert_eq!(sin12(0x400), 4096);
        assert_eq!(cos12(0), 4096);
        // dx = cos·r, dz = sin·r
        assert_eq!(atan2_12(100, 0), 0);
        assert_eq!(atan2_12(0, 100), 0x400);
        assert_eq!(atan2_12(-100, 0).abs(), 0x800);
    }
}
