//! The versus fight camera, integer-exact (kumite_converter's reference camera.md, and its prototype in
//! kumite_converter).
//!
//! Game space: Y points down, floor = 0, units ≈ mm; angles 4096 per turn, kept `<< 6` inside the module.
//! Routines: 0x8006465C sample, 0x80064810 yaw control, 0x80064E10 framing, 0x80063D00/0x80064170 blend,
//! 0x80068394 intro fly-in, 0x80062B74 slot mixer, 0x80064080 apply.
//! The sine/atan tables and the intro offsets come from the converted `gamedata/camera.json`.

use anyhow::Result;
use serde::Deserialize;
use std::path::Path;
use std::sync::Arc;

#[derive(Deserialize)]
struct Json {
    projection_h: i64,
    tables: JsonTables,
    fight: JsonFight,
    intro: JsonIntro,
    gsqrt: JsonValues,
    end: EndData,
}
#[derive(Deserialize)]
struct JsonTables {
    sin: JsonValues,
    atan: JsonValues,
}
#[derive(Deserialize)]
struct JsonValues {
    values: Vec<i64>,
}
#[derive(Deserialize)]
struct JsonFight {
    distance: i64,
    initial_eye: [i64; 3],
}
#[derive(Deserialize)]
struct JsonIntro {
    frames_first_round: i64,
    frames_other: i64,
    offsets: Vec<[i64; 3]>,
}

/// Round-end constants (camera.json `end`).
#[derive(Deserialize, Clone, Debug)]
pub struct EndData {
    pub ko_frames: u32,
    pub draw_frames: u32,
    pub loser_frames: u32,
    pub pose_cap: u32,
    pub replay: EndReplay,
    aim: EndAim,
    cam_a: EndCamA,
    cam_b: EndCamB,
    overhead: EndOverhead,
    orbit: EndOrbit,
    pub motions: EndMotions,
}
#[derive(Deserialize, Clone, Debug)]
pub struct EndReplay {
    pub pre_roll: u32,
    pub min_frames: u32,
    pub ring: u32,
    pub blink: u32,
    variant_a: EndVariantA,
    variant_b: EndVariantB,
    offset_easing_shift: u32,
}
#[derive(Deserialize, Clone, Debug)]
struct EndVariantA {
    yaw_from: i64,
    yaw_range: i64,
    offset_degrees: [i64; 2],
}
#[derive(Deserialize, Clone, Debug)]
struct EndVariantB {
    yaw_from: i64,
    yaw_range: i64,
}
#[derive(Deserialize, Clone, Debug)]
struct EndAim {
    min_distance: i64,
    chest_clamp: i64,
    floor_clamp: i64,
    chest_offset: i64,
}
#[derive(Deserialize, Clone, Debug)]
struct EndCamA {
    arc_from: i64,
    arc_range: i64,
    radius: i64,
    eye_y: i64,
    stop_distance: i64,
    approach: i64,
    pitch_shift: u32,
    yaw_shift: u32,
}
#[derive(Deserialize, Clone, Debug)]
struct EndCamB {
    radius: [i64; 3],
    side: [i64; 3],
    height: [i64; 3],
    drift_shift: u32,
    push_distance: i64,
    push_shift: u32,
    pitch_shift: u32,
    yaw_shift: u32,
    height_reach: i64,
    height_shift: u32,
}
#[derive(Deserialize, Clone, Debug)]
struct EndOverhead {
    height: i64,
    pitch: i64,
    yaw_from: i64,
    jitter: [i64; 2],
}
#[derive(Deserialize, Clone, Debug)]
struct EndOrbit {
    arc_from: i64,
    arc_range: i64,
    radius: [i64; 2],
    eye_y: i64,
    target_y: i64,
    stop_distance: [i64; 2],
    approach: i64,
    target_easing: [i64; 2],
    /// The large character's move set.
    large_moveset_key: i64,
}
#[derive(Deserialize, Clone, Debug)]
pub struct EndMotions {
    pub win: Vec<usize>,
    pub lose_ko: usize,
    pub lose_time_up: usize,
}

/// Camera constants and tables from camera.json.
pub struct CameraData {
    sin: Vec<i64>,
    gsqrt_table: Vec<i64>,
    pub end: EndData,
    atn: Vec<i64>,
    pub distance: i64,
    pub h: i64,
    pub initial_eye: [i64; 3],
    pub intro_frames: [i64; 2],
    pub intro_offsets: Vec<[i64; 3]>,
}

impl CameraData {
    pub fn load(path: &Path) -> Result<Self> {
        let j: Json = crate::data::load(path)?;
        anyhow::ensure!(j.tables.sin.values.len() == 5120, "camera.json: sin table needs 5120 entries");
        anyhow::ensure!(j.tables.atan.values.len() == 1025, "camera.json: atan table needs 1025 entries");
        anyhow::ensure!(!j.intro.offsets.is_empty(), "camera.json: no intro offsets");
        anyhow::ensure!(j.gsqrt.values.len() == 192, "camera.json: gsqrt table needs 192 entries");
        Ok(Self {
            sin: j.tables.sin.values,
            gsqrt_table: j.gsqrt.values,
            end: j.end,
            atn: j.tables.atan.values,
            distance: j.fight.distance,
            h: j.projection_h,
            initial_eye: j.fight.initial_eye,
            intro_frames: [j.intro.frames_first_round, j.intro.frames_other],
            intro_offsets: j.intro.offsets,
        })
    }

    /// The move set of the large character ([`Subject::large`]).
    pub fn large_moveset(&self) -> i64 {
        self.end.orbit.large_moveset_key
    }

    /// rsin / rcos of a 4096-unit angle.
    pub fn sin(&self, a: i64) -> i64 {
        self.sin[(a & 0xFFF) as usize]
    }
    pub fn cos(&self, a: i64) -> i64 {
        self.sin[(a & 0xFFF) as usize + 1024]
    }

    /// 0x8004AEDC: the GTE's approximate square root (SquareRoot0, table 0x8004AF40).
    pub fn gsqrt(&self, a: i64) -> i64 {
        if a <= 0 {
            return 0;
        }
        let a = a.min(u32::MAX as i64) as u32;
        let lz = (a.leading_zeros() & !1) as i64;
        let m = if lz <= 24 { (a >> (24 - lz)) as i64 } else { (a as i64) << (lz - 24) };
        (self.gsqrt_table[(m - 64).clamp(0, 191) as usize] << ((31 - lz) >> 1)) >> 12
    }

    fn sin6(&self, a6: i64) -> i64 {
        self.sin[idx6(a6)]
    }
    fn cos6(&self, a6: i64) -> i64 {
        self.sin[idx6(a6) + 1024]
    }

    /// 0x8004B39C: atan2(y, x) in 0..4096.
    pub fn ratan2(&self, x: i64, y: i64) -> i64 {
        if x == 0 && y == 0 {
            return 0;
        }
        let at = |q: i64| self.atn[q as usize] + (q >> 1);
        let r = if x >= 0 {
            if y >= 0 {
                if x >= y {
                    at(tdiv(y << 10, x))
                } else {
                    0x400 - at(tdiv(x << 10, y))
                }
            } else {
                let ny = -y;
                if x >= ny {
                    0x1000 - at(tdiv(ny << 10, x))
                } else {
                    at(tdiv(x << 10, ny)) + 0xC00
                }
            }
        } else {
            let nx = -x;
            if y >= 0 {
                if nx >= y {
                    0x800 - at(tdiv(y << 10, nx))
                } else {
                    at(tdiv(nx << 10, y)) + 0x400
                }
            } else if nx < -y {
                0xC00 - at(tdiv(x << 10, y))
            } else {
                at(tdiv(y << 10, x)) + 0x800
            }
        };
        s16(r)
    }
}

// ---------------------------------------------------------------- MIPS integer helpers

/// MIPS `div`: truncate toward zero (by zero, which the original's data never does: 0).
pub fn tdiv(a: i64, b: i64) -> i64 {
    a.checked_div(b).unwrap_or(0)
}
/// `bgez; addiu 2^n-1; sra n`: divide by 2^n toward zero.
pub fn sra_tz(a: i64, n: u32) -> i64 {
    (if a < 0 { a + (1 << n) - 1 } else { a }) >> n
}
fn half(a: i64) -> i64 {
    a / 2
}
/// Angle difference in <<6 units wrapped to [-0x20000, 0x20000).
pub fn wrap18(v: i64) -> i64 {
    let v = v & 0x3FFFF;
    if v & 0x20000 != 0 {
        v - 0x40000
    } else {
        v
    }
}
fn s16(v: i64) -> i64 {
    v as i16 as i64
}
fn idx6(a6: i64) -> usize {
    (sra_tz(a6, 6) & 0xFFF) as usize
}
/// 0x8002A324 (Newton, unsigned 32-bit).
pub fn isqrt(n: i64) -> i64 {
    let n = n & 0xFFFF_FFFF;
    if n == 0 {
        return 0;
    }
    let (mut v, mut a) = (1i64, n);
    while v < a {
        v <<= 1;
        a >>= 1;
    }
    loop {
        a = v;
        v = (n / a + a) >> 1;
        if v >= a {
            return a;
        }
    }
}

// ---------------------------------------------------------------- state

/// 24-byte key slot (table 0x800A02A8): eye, pitch<<6, yaw<<6, projection distance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Key {
    pub x: i64,
    pub y: i64,
    pub z: i64,
    pub p: i64,
    pub w: i64,
    pub h: i64,
}

/// 48-byte per-slot state (0x800A0328 + 0x30*slot).
#[derive(Clone, Copy, Debug, Default)]
struct SlotState {
    state: i64,
    moving: bool,
    count: i64,
    yawvel: i64,
    yawacc: i64,
    pitch: i64,
    pitch_tgt: i64,
    pitch_step: i64,
    yawerr: i64,
    vstep: i64,
    vpull: i64,
    hpull: i64,
}

/// What the camera samples from one fighter (0x8006465C), game space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub x: i64,
    /// Chest (bone slot 2) Y − 290: about −1715 standing.
    pub y: i64,
    pub z: i64,
    /// Start side: −100 for the player who began on the left, +100 for the right.
    pub side: i64,
}

/// Render view (RVIEW): eye in game space, pitch/yaw in 4096 units, projection distance H.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct View {
    pub eye: [i64; 3],
    pub pitch: i64,
    pub yaw: i64,
    pub h: i64,
}

#[derive(Clone)]
pub struct FightCamera {
    data: Arc<CameraData>,
    h: i64,
    hmin: i64,
    hmax: i64,
    keys: [Key; 5],
    st: [SlotState; 5],
    samp: [Sample; 2],
    /// Ball: the ball, a third framing point (record 8 0x800B4484; game units, y down).
    pub ball: Option<[i64; 3]>,
    w: [i64; 5],
    wprev: [i64; 5],
    pub frame: i64,
    intro: bool,
    intro_len: i64,
    intro_t: Option<i64>,
    intro_var: usize,
    intro_base: Key,
    replay: Option<Replay>,
    pub view: View,
}

/// The replay camera's block (0x800A04C8): distance, yaw and its sweeping offset (both kept in 16 bits, as the
/// game stores them: replay angles wrap every 90 degrees), first-frame snap.
#[derive(Clone, Copy, Debug)]
struct Replay {
    dist: i64,
    yaw: i64,
    off: i64,
    target: i64,
    snap: bool,
}

impl FightCamera {
    /// `intro`: play the round-start fly-in (variant = round counter % 10).
    pub fn new(data: Arc<CameraData>, intro: bool, intro_variant: usize, first_round: bool) -> Self {
        let h = data.h;
        let [ex, ey, ez] = data.initial_eye;
        let mut keys = [Key::default(); 5];
        for k in &mut keys[..2] {
            *k = Key { x: ex, y: ey, z: ez, p: 0, w: 0, h };
        }
        let n = data.intro_offsets.len();
        let intro_len = data.intro_frames[if first_round { 0 } else { 1 }];
        Self {
            data,
            h,
            hmin: h,
            hmax: h,
            keys,
            st: [SlotState::default(); 5],
            samp: [Sample { x: 0, y: 0, z: 0, side: 0 }; 2],
            ball: None,
            w: [0x1000, 0, 0, 0, 0],
            wprev: [0; 5],
            frame: 0,
            intro,
            intro_len,
            intro_t: None,
            intro_var: intro_variant % n,
            intro_base: Key::default(),
            replay: None,
            view: View::default(),
        }
    }

    pub fn data(&self) -> &Arc<CameraData> {
        &self.data
    }

    /// True while the round-intro fly-in is still playing.
    pub fn in_intro(&self) -> bool {
        self.intro
    }

    /// One frame of the versus camera director (phase 1 intro, phase 2 normal).
    /// `frames_since_start`: the yaw controller stays frozen for the first 200 frames of a round.
    /// `move_cam`: camera-control values of the two current moves (0x27..0x2A cap the yaw speed).
    pub fn step(&mut self, p: [Sample; 2], frames_since_start: i64, move_cam: [i64; 2]) -> View {
        self.sample(p);
        if self.intro && self.intro_t.is_none() {
            self.slot0_update(true, frames_since_start, move_cam);
            self.intro_base = self.keys[0];
            self.intro_t = Some(self.intro_len);
            self.w = [0, 0, 0x1000, 0, 0];
            self.wprev = [0; 5];
        }
        if self.intro && self.intro_t.is_some_and(|t| t > 0) {
            let v = self.intro_key();
            self.w = [0x1000 - v, 0, v, 0, 0];
            if self.w[0] > 0 {
                self.slot0_update(self.wprev[0] < 1, frames_since_start, move_cam);
            }
            self.mix();
            if self.w[2] <= 0 {
                self.intro = false;
            }
        } else {
            self.w = [0x1000, 0, 0, 0, 0];
            self.slot0_update(self.wprev[0] < 1, frames_since_start, move_cam);
            self.mix();
        }
        self.frame += 1;
        self.view
    }

    /// 0x8006953C: start the replay camera. `variant_a`: the sweeping variant (the game alternates, B first);
    /// `random(n)`: 0..n. The samples are the replay's (root x/z, y = min(chest, root y)).
    pub fn start_replay(&mut self, p: [Sample; 2], variant_a: bool, random: &mut dyn FnMut(i64) -> i64) {
        let [ex, ey, ez] = self.data.initial_eye;
        self.keys[1] = Key { x: ex, y: ey, z: ez, p: 0, w: 0, h: self.data.h };
        self.st[1] = SlotState::default();
        self.intro = false;
        self.sample(p);
        let r = &self.data.end.replay;
        let [a, b] = self.samp;
        let axis = self.data.ratan2(b.x - a.x, b.z - a.z);
        let yaw = s16(axis << 6);
        let deg6 = |d: i64| d * 0x40000 / 360;
        let (yaw, off, target) = if variant_a {
            let va = &r.variant_a;
            let yaw = s16(u16v(yaw) + va.yaw_from + random(va.yaw_range));
            let [lo, span] = va.offset_degrees;
            let (mut off, mut tgt) = (s16(deg6(lo + random(span))), -s16(deg6(lo + random(span))));
            if random(2) == 1 {
                off = -off;
                tgt = -tgt;
            }
            (yaw, off, tgt)
        } else {
            let vb = &r.variant_b;
            (s16(u16v(yaw) + vb.yaw_from + random(vb.yaw_range)), 0, 0)
        };
        self.replay = Some(Replay { dist: 0x2B1A, yaw, off, target, snap: true });
    }

    /// 0x80069414: one replay camera frame (slot 1 framing, the offset and distance eased).
    pub fn replay_step(&mut self, p: [Sample; 2]) -> View {
        self.sample(p);
        let Some(mut r) = self.replay else { return self.view };
        self.frame_key(1, 0, r.yaw + r.off, r.snap, r.dist);
        r.snap = false;
        r.off = s16(r.off + sra_tz(wrap18(r.target - r.off), self.data.end.replay.offset_easing_shift));
        r.dist += sra_tz((0x113D - r.dist) * 6, 8);
        self.replay = Some(r);
        let k = self.keys[1];
        self.apply(k);
        self.view
    }

    fn sample(&mut self, p: [Sample; 2]) {
        self.samp = p;
        let [a, b] = &mut self.samp;
        if a.x == b.x && a.z == b.z {
            a.x -= 1;
            a.z -= 1;
            b.x += 1;
            b.z += 1;
        }
    }

    fn slot0_update(&mut self, snap: bool, fss: i64, mc: [i64; 2]) {
        let (yaw, flag) = self.yaw_control(snap, fss, mc);
        self.frame_key(0, self.st[0].pitch, yaw, flag, self.data.distance);
    }

    /// 0x80064810 yaw controller (slot 0) -> (yaw<<6, snap).
    fn yaw_control(&mut self, snap: bool, fss: i64, move_cam: [i64; 2]) -> (i64, bool) {
        let d_ = self.data.clone();
        let key = self.keys[0];
        let st = &mut self.st[0];
        let [a, b] = self.samp;
        let mut ang = d_.ratan2(b.x - a.x, b.z - a.z);
        if a.side >= b.side {
            ang = (ang + 0x800) & 0xFFF;
        }
        let tgt = ang << 6;
        let diff = wrap18(tgt - key.w);
        let sgn = diff.signum();
        let d = diff.abs();
        if snap {
            st.yawvel = 0;
            st.yawacc = 0;
            st.pitch = 0;
            st.pitch_tgt = 0;
            st.pitch_step = 0;
            return (tgt, true);
        }
        if d > 0x1AAAA || fss < 200 {
            return (key.w, false); // frozen (side switch / round start)
        }
        let mut dyaw = 0;
        if st.state == 0 {
            st.count = if d >= 0xE38 { st.count + 1 } else { 0 };
            if st.count > 0 {
                st.state = 1;
                st.yawacc += sgn * 0x600;
                st.count = 0;
            }
        } else if st.state == 1 {
            if d < 0x38E4 {
                let w = st.yawvel;
                st.yawacc -= half(st.yawacc);
                st.yawvel = w - sra_tz(w * 14, 7);
                st.yawacc += sgn * 0x300;
                if d < 0xE39 {
                    st.yawacc -= half(st.yawacc);
                    st.count += 1;
                    if st.count >= 0x20 {
                        st.yawvel = 0;
                        st.state = 0;
                        st.count = 0;
                    }
                } else {
                    st.count = 0;
                }
            } else {
                st.yawacc += sgn * 0x600;
            }
            st.yawvel += st.yawacc;
            let lim = [0x27, 0x28, 0x29, 0x2A].into_iter().find(|mc| move_cam.contains(mc)).map_or(-1, |mc| [0x200, 0x180, 0x80, 0x40][(mc - 0x27) as usize]);
            if lim > 0 {
                let l = (lim * 56784) >> 8;
                if l < st.yawvel {
                    st.yawacc -= 0x600;
                    st.yawvel = l;
                } else if st.yawvel < -l {
                    st.yawacc += 0x600;
                    st.yawvel = -l;
                }
            } else if st.yawvel > 0xDDD0 {
                st.yawvel = 0xDDD0;
                st.yawacc -= 0x600;
            } else if st.yawvel < -0xDDD0 {
                st.yawvel = -0xDDD0;
                st.yawacc += 0x600;
            }
            dyaw = sra_tz(st.yawvel, 6);
        }
        // pitch lag
        st.yawerr = d;
        st.pitch_tgt = if d >= 0x38E3 {
            let v = (((d - 0x38E3) << 12) * 0x14923A73) >> 44;
            sra_tz(d_.sin[((sra_tz(v, 7) & 0x1FFE) >> 1) as usize] * 279616, 12)
        } else {
            0
        };
        if st.moving {
            let step = sra_tz(st.pitch_tgt - st.pitch, 4).clamp(-0x100, 0x100);
            st.pitch_step = step;
            st.pitch += step;
        } else {
            st.pitch_step = 0;
        }
        (key.w + dyaw, false)
    }

    /// 0x80064E10 framing solver ("camera key") for a slot (0 = fight, 1 = replay).
    fn frame_key(&mut self, slot: usize, pitch6: i64, yaw6: i64, flag: bool, dist: i64) {
        let d_ = self.data.clone();
        let (sin6, cos6) = (|a| d_.sin6(a), |a| d_.cos6(a));
        let h = self.h;
        let dist = if dist < 0x321 { 0x320 } else { dist };
        let key = self.keys[slot];
        let st = &mut self.st[slot];
        let (s0, s5) = (sin6(-pitch6), cos6(-pitch6));
        let (s4, s3) = (sin6(-yaw6), cos6(-yaw6));
        let [a, b] = self.samp;
        let (mx, mz) = (half(a.x + b.x), half(a.z + b.z));
        let (ex, ez) = if flag {
            (mx + sra_tz(dist * cos6(yaw6 + 0x30000), 12), mz + sra_tz(dist * sin6(yaw6 + 0x30000), 12))
        } else {
            let dd = wrap18(yaw6 - key.w);
            let (c, s) = (cos6(dd), sin6(dd));
            let (ox, oz) = (key.x - mx, key.z - mz);
            (mx + sra_tz(c * ox - s * oz, 12), mz + sra_tz(s * ox + c * oz, 12))
        };
        let ey = key.y;
        // the fighters' samples, and in Ball the ball as a third point (index 2)
        let (mut xs, mut ys, mut zs, mut hds) = ([0i64; 3], [0i64; 3], [0i64; 3], [0i64; 3]);
        let ball = self.ball;
        let points = self.samp.iter().map(|p| [p.x, p.y, p.z]).chain(ball);
        for (i, p) in points.enumerate() {
            let (dx, dz, dy) = (p[0] - ex, p[2] - ez, p[1] - ey);
            let hd = sra_tz(s4 * dx + s3 * dz, 12);
            xs[i] = sra_tz(s3 * dx - s4 * dz, 12);
            zs[i] = sra_tz(s5 * hd - s0 * dy, 12);
            ys[i] = sra_tz(s0 * hd + s5 * dy, 12);
            hds[i] = hd;
        }
        let (mut dx_, mut dz_) = (0i64, 0i64);
        // depth: centre point (mid horizontal depth, height -450) to depth = dist
        let ahd = half(hds[0] + hds[1]);
        let dyc = -0x1C2 - ey;
        let mut zc = sra_tz(s5 * ahd - s0 * dyc, 12);
        let yc = sra_tz(s0 * ahd + s5 * dyc, 12);
        let a0 = dist - zc;
        zc = dist;
        dz_ -= a0;
        zs[0] += a0;
        zs[1] += a0;
        zs[2] += a0;
        // horizontal framing (150-unit margins, 280-px window, 48-px dead zone)
        let (mut l, mut r) = if xs[0] < xs[1] {
            xs[0] -= 150;
            xs[1] += 150;
            (0, 1)
        } else {
            xs[1] -= 150;
            xs[0] += 150;
            (1, 0)
        };
        // the ball widens the pair on the side it is out on (0x80064E10, battle type 7)
        if ball.is_some() {
            if xs[2] < xs[l] {
                xs[2] -= 150;
                l = 2;
            } else if xs[2] > xs[r] {
                xs[2] += 150;
                r = 2;
            }
        }
        let sxl = tdiv(xs[l] * h, zs[l]);
        let sxr = tdiv(xs[r] * h, zs[r]);
        if sxr - sxl >= 0x119 {
            let v1 = tdiv(h * (xs[r] - xs[l]) - 140 * (zs[r] - zs[l]), 280);
            let mut a1 = zs[l] - v1;
            if !flag {
                let prev = st.hpull;
                a1 = if prev < a1 { prev + sra_tz(a1 - prev, 5) } else { prev - sra_tz(prev - a1, 3) };
            }
            st.hpull = a1;
            dz_ += a1;
            dx_ = sra_tz(xs[l] - tdiv(-(140 * v1), h), 3);
            zs[0] -= a1;
            zs[1] -= a1;
            zs[2] -= a1;
            zc -= a1;
        } else {
            let mut decay = true;
            let b2 = tdiv(48 * zs[r], h);
            let a2 = tdiv(-48 * zs[l], h);
            if flag || (b2 - (xs[r] - xs[l])) < a2 {
                dx_ = sra_tz(xs[l] + tdiv((xs[r] - xs[l]) * zs[l], zs[l] + zs[r]), 3);
                if flag {
                    st.hpull = 0;
                    decay = false;
                }
            } else if sxl < -0x30 {
                dx_ += sra_tz(xs[l] - a2, 3);
            } else if sxr > 0x30 {
                dx_ += sra_tz(xs[r] - b2, 3);
            }
            if decay {
                let prev = st.hpull;
                let n = if prev < 0 {
                    prev - sra_tz(prev, 5)
                } else if prev > 0 {
                    prev - (prev >> 3)
                } else {
                    0
                };
                st.hpull = n;
                dz_ += n;
                zs[0] -= n;
                zs[1] -= n;
                zs[2] -= n;
                zc -= n;
            }
        }
        // vertical framing
        let sy = [tdiv(ys[0] * h, zs[0]), tdiv(ys[1] * h, zs[1])];
        let mut top = if sy[0] < sy[1] { 0 } else { 1 };
        // the ball can be the top point
        if ball.is_some() && zs[2] != 0 && tdiv(ys[2] * h, zs[2]) < sy[top] {
            top = 2;
        }
        let (ytop, ztop) = (ys[top], zs[top]);
        let t1 = 100;
        let mut a3 = sra_tz(s5 * (t1 - tdiv(450 * h, dist)), 12);
        let mut pull = (tdiv(-(112 * (ztop - zc)) - h * (ytop - yc), a3 + 0x70) - zc).max(0);
        if flag {
            pull = 0;
            st.vstep = 0;
        } else {
            let prev = st.vpull;
            if pull < prev {
                let dlt = (prev - pull).min(0x4B0);
                st.vstep = sra_tz(dlt * 120, 7) - dlt;
                pull = prev + st.vstep;
            } else if pull > prev {
                let dlt = (pull - prev).min(0x320);
                st.vstep = sra_tz(dlt, 3);
                pull = prev + st.vstep;
            }
        }
        st.vpull = pull;
        if pull > 0 {
            dz_ -= pull;
            zc += pull;
        }
        a3 += sra_tz(tdiv(sra_tz(pull * s5, 12) * h, dist), 4);
        let dy_ = yc - tdiv(a3 * zc, h);
        // view-space delta -> world
        let (sp, cp) = (sin6(pitch6), cos6(pitch6));
        let hd = sra_tz(dz_ * cp - dy_ * sp, 12);
        let wdy = sra_tz(dz_ * sp + dy_ * cp, 12);
        let (sy_, cy_) = (sin6(yaw6), cos6(yaw6));
        let wdx = sra_tz(dx_ * cy_ - hd * sy_, 12);
        let wdz = sra_tz(dx_ * sy_ + hd * cy_, 12);
        let (nx, ny, nz) = (ex + wdx, ey + wdy, ez + wdz);
        if !flag {
            let (ddx, ddz) = (nx - key.x, nz - key.z);
            st.moving = ddx * ddx + ddz * ddz > 1000;
        }
        self.keys[slot] = Key { x: nx, y: ny, z: nz, p: pitch6, w: yaw6, h: key.h };
    }

    /// 0x80063D00: look target of a key (distance = mean eye-fighter distance).
    fn target(&self, k: &Key) -> (i64, [i64; 3]) {
        let d_ = &self.data;
        let (sp, cp) = (d_.sin6(k.p), d_.cos6(k.p));
        let yy = k.w + 0x10000;
        let (sy, cy) = (d_.sin6(yy), d_.cos6(yy));
        let mut d = 0;
        for p in &self.samp {
            d += isqrt((p.x - k.x).pow(2) + (p.y - k.y).pow(2) + (p.z - k.z).pow(2));
        }
        d >>= 1;
        (d, [k.x + sra_tz(sra_tz(d * cy, 12) * cp, 12), k.y + sra_tz(d * sp, 12), k.z + sra_tz(sra_tz(d * sy, 12) * cp, 12)])
    }

    /// 0x80064170: orbit blend of two keys around their look targets.
    fn blend(&self, a: &Key, b: &Key, wa: i64) -> Key {
        let d_ = &self.data;
        let wb = 0x1000 - wa;
        let (da, ta) = self.target(a);
        let (db, tb) = self.target(b);
        let d = sra_tz(da * wa + db * wb, 12);
        let t: [i64; 3] = std::array::from_fn(|i| sra_tz(ta[i] * wa + tb[i] * wb, 12));
        let p = a.p + sra_tz(wrap18(b.p - a.p) * wb, 12);
        let yw = a.w + sra_tz(wrap18(b.w - a.w) * wb, 12);
        let (sp, cp) = (d_.sin6(p), d_.cos6(p));
        let i = ((sra_tz(yw, 6) + 0x400) & 0xFFF) as usize;
        let e = [t[0] - sra_tz(sra_tz(d * d_.sin[i + 1024], 12) * cp, 12), t[1] - sra_tz(d * sp, 12), t[2] - sra_tz(sra_tz(d * d_.sin[i], 12) * cp, 12)];
        let h = sra_tz(a.h * wa + b.h * wb, 12);
        self.lookat(e, t, if h > 0 { h } else { self.keys[3].h })
    }

    /// 0x80063EEC: key from eye and target.
    fn lookat(&self, e: [i64; 3], t: [i64; 3], h: i64) -> Key {
        let (dx, dy, dz) = (t[0] - e[0], t[1] - e[1], t[2] - e[2]);
        let hd = isqrt(dx * dx + dz * dz);
        Key { x: e[0], y: e[1], z: e[2], p: s16(self.data.ratan2(hd, dy)) << 6, w: s16(self.data.ratan2(dz, -dx)) << 6, h }
    }

    /// 0x80064080: apply a key -> RVIEW + H.
    fn apply(&mut self, k: Key) {
        self.keys[3] = k;
        self.h = k.h.clamp(self.hmin, self.hmax);
        self.view = View { eye: [k.x, k.y, k.z], pitch: sra_tz(k.p, 6), yaw: sra_tz(k.w, 6), h: self.h };
    }

    /// 0x80068394 / 0x800684B4: intro fly-in key (slot 2); returns its weight.
    fn intro_key(&mut self) -> i64 {
        let base = self.intro_base;
        let t = self.intro_t.unwrap_or(0);
        let f = self.data.sin[((0xC00 + tdiv(t << 10, self.intro_len)) & 0xFFF) as usize] + 0x1000;
        let o = self.data.intro_offsets[self.intro_var];
        let eye = [base.x + sra_tz(o[0] * f, 12), base.y + sra_tz(o[1] * f, 12), base.z + sra_tz(o[2] * f, 12)];
        self.keys[2] = self.lookat(eye, [base.x, base.y, 0], base.h);
        let t = t - 1;
        self.intro_t = Some(t);
        if t > 0x10 {
            0x1000
        } else {
            t << 8
        }
    }

    /// 0x80062B74 slot mixer (the versus director only uses slots 0 and 2).
    fn mix(&mut self) {
        let m: u32 = (0..5).filter(|&i| self.w[i] > 0).map(|i| 1 << i).sum();
        let k = self.keys;
        let key = match m {
            1 => Some(k[0]),
            2 => Some(k[1]),
            4 => Some(k[2]),
            3 => Some(self.blend(&k[0], &k[1], self.w[0])),
            5 => Some(self.blend(&k[0], &k[2], self.w[0])),
            0x10 => Some(k[4]),
            0x11 => {
                let ww = self.data.sin[((half(self.w[0]) - 0x400) & 0xFFF) as usize];
                Some(self.blend(&k[0], &k[4], half(ww) + 0x800))
            }
            _ => None,
        };
        if let Some(key) = key {
            self.apply(key);
        }
        self.wprev = self.w;
    }
}

/// The 16-bit unsigned view of a value (the game's `lhu`).
fn u16v(v: i64) -> i64 {
    v & 0xFFFF
}

// ---------------------------------------------------------------- round-end cameras

/// What an end camera looks at (game space): the root bone, the chest height, the facing as the angle of the
/// fighter's front in the XZ plane (4096 units), and whether it is the large character (moveset key 11).
#[derive(Clone, Copy, Debug)]
pub struct Subject {
    pub root: [i64; 3],
    pub chest: i64,
    pub front: i64,
    pub large: bool,
}

/// An end camera's view in `<< 6` units (RVIEW blocks 0x8009E6A0 / 0x8009E6C8).
#[derive(Clone, Copy, Debug, Default)]
pub struct Block {
    eye: [i64; 3],
    p: i64,
    y: i64,
}

/// The cameras after the replay: winner camera A or B (phases 3/4), the loser's overhead shot or the orbit
/// (phases 6/7). Every switch is a cut; H stays 500.
#[derive(Clone, Debug)]
pub enum EndCam {
    WinnerA { block: Block },
    WinnerB { block: Block, drift: [i64; 2] },
    Overhead { yaw: i64 },
    Orbit { eye: [i64; 3], target: [i64; 3] },
}

impl CameraData {
    /// 0x80045FEC: pitch and yaw (`<< 6`, pitch signed) looking from `e` to `t`.
    fn lookat6(&self, e: [i64; 3], t: [i64; 3]) -> (i64, i64) {
        let (dx, dy, dz) = (t[0] - e[0], t[1] - e[1], t[2] - e[2]);
        let hd = isqrt(dx * dx + dz * dz);
        (s12(self.ratan2(hd, dy)) << 6, (self.ratan2(dz, -dx) & 0xFFF) << 6)
    }

    /// 0x80046270: aim at the fighter with the pitch clamps and the 5800-unit minimum horizontal distance.
    fn aim(&self, p: &Subject, eye: [i64; 3]) -> Block {
        let a = &self.end.aim;
        let [rx, _, rz] = p.root;
        let (pt, y) = self.lookat6(eye, p.root);
        let pc = self.lookat6(eye, [rx, p.chest + a.chest_offset, rz]).0;
        let pf = self.lookat6(eye, [rx, 0, rz]).0;
        let pitch = pt.min(pc + a.chest_clamp).min(pf + a.floor_clamp);
        let mut out = Block { eye, p: pitch, y };
        let (dx, dz) = (eye[0] - rx, eye[2] - rz);
        if dx * dx + dz * dz < a.min_distance * a.min_distance {
            let i = sra_tz(y, 6);
            out.eye[0] = rx + sra_tz(self.sin(i) * a.min_distance, 12);
            out.eye[2] = rz - sra_tz(self.cos(i) * a.min_distance, 12);
        }
        out
    }

    fn view6(&self, b: &Block) -> View {
        View { eye: b.eye, pitch: sra_tz(b.p, 6), yaw: sra_tz(b.y, 6), h: self.h }
    }
}

/// A signed 12-bit angle (wrapped to -2048..2047).
fn s12(v: i64) -> i64 {
    ((v + 2048) & 0xFFF) - 2048
}

impl EndCam {
    /// Winner camera A (0x80046460) or B (0x800467A0) on the round winner. `random(n)`: 0..n.
    pub fn winner(d: &CameraData, p: &Subject, b_variant: bool, random: &mut dyn FnMut(i64) -> i64) -> Self {
        let [rx, _, rz] = p.root;
        if !b_variant {
            let c = &d.end.cam_a;
            let a = (c.arc_from + random(c.arc_range) + p.front - 0x400) & 0xFFF;
            let eye = [rx + sra_tz(d.cos(a) * c.radius, 12), c.eye_y, rz + sra_tz(d.sin(a) * c.radius, 12)];
            return EndCam::WinnerA { block: d.aim(p, eye) };
        }
        let c = &d.end.cam_b;
        let pick = |t: &[i64; 3], r: &mut dyn FnMut(i64) -> i64| t[r(3) as usize];
        let (rr, ss, hh) = (pick(&c.radius, random), pick(&c.side, random), pick(&c.height, random));
        let (cs, sn) = (d.cos(p.front), d.sin(p.front));
        let drift = [sra_tz(-cs, c.drift_shift), sra_tz(-sn, c.drift_shift)];
        let sigma = if random(2) == 1 { 1 } else { -1 };
        let eye = [rx + sra_tz(rr * cs + sigma * ss * sn, 12), hh, rz + sra_tz(rr * sn - sigma * ss * cs, 12)];
        EndCam::WinnerB { block: d.aim(p, eye), drift }
    }

    /// The loser camera (0x8006875C): the overhead shot when it was knocked out, else the orbit.
    pub fn loser(d: &CameraData, p: &Subject, knocked_out: bool, random: &mut dyn FnMut(i64) -> i64) -> Self {
        if knocked_out {
            let o = &d.end.overhead;
            let jitter = o.jitter[0] + random(o.jitter[1] - o.jitter[0] + 1);
            return EndCam::Overhead { yaw: o.yaw_from + p.front - 0x400 + jitter };
        }
        let o = &d.end.orbit;
        let [rx, _, rz] = p.root;
        let a = (o.arc_from + random(o.arc_range) + p.front - 0x400) & 0xFFF;
        let r = o.radius[usize::from(p.large)];
        EndCam::Orbit { eye: [rx + sra_tz(d.cos(a) * r, 12), o.eye_y, rz + sra_tz(d.sin(a) * r, 12)], target: [rx, o.target_y, rz] }
    }

    /// One frame on the subject; returns the view.
    pub fn step(&mut self, d: &CameraData, p: &Subject) -> View {
        let [rx, ry, rz] = p.root;
        match self {
            EndCam::WinnerA { block } => {
                let c = &d.end.cam_a;
                let (dx, dy, dz) = (rx - block.eye[0], ry - block.eye[1], rz - block.eye[2]);
                let n = d.gsqrt(dx * dx + dy * dy + dz * dz);
                if n > c.stop_distance {
                    block.eye[0] += tdiv(c.approach * dx, n);
                    block.eye[1] += tdiv(c.approach * dy, n);
                    block.eye[2] += tdiv(c.approach * dz, n);
                }
                let t = d.aim(p, block.eye);
                block.p += sra_tz(wrap18(t.p - block.p), c.pitch_shift);
                block.y += sra_tz(wrap18(t.y - block.y), c.yaw_shift);
                d.view6(block)
            }
            EndCam::WinnerB { block, drift } => {
                let c = &d.end.cam_b;
                block.eye[0] += drift[0];
                block.eye[2] += drift[1];
                let (dx, dy, dz) = (rx - block.eye[0], ry - block.eye[1], rz - block.eye[2]);
                let k = c.push_distance - d.gsqrt(dx * dx + dy * dy + dz * dz);
                if k > 0 {
                    let h = d.gsqrt(dx * dx + dz * dz).max(1);
                    block.eye[0] -= sra_tz(tdiv(k * dx, h), c.push_shift);
                    block.eye[2] -= sra_tz(tdiv(k * dz, h), c.push_shift);
                }
                let t = d.aim(p, block.eye);
                block.p += sra_tz(wrap18(t.p - block.p), c.pitch_shift);
                block.y += sra_tz(wrap18(t.y - block.y), c.yaw_shift);
                if block.p > 0 {
                    let v = sra_tz(c.height_reach * d.sin(block.p >> 6), 12);
                    if v < block.eye[1] {
                        block.eye[1] += sra_tz(v - block.eye[1], c.height_shift);
                    }
                }
                d.view6(block)
            }
            EndCam::Overhead { yaw } => {
                let o = &d.end.overhead;
                View { eye: [rx, o.height, rz], pitch: o.pitch, yaw: *yaw & 0xFFF, h: d.h }
            }
            EndCam::Orbit { eye, target } => {
                let o = &d.end.orbit;
                let (dx, dz) = (target[0] - eye[0], target[2] - eye[2]);
                let n = d.gsqrt(dx * dx + dz * dz);
                if n > o.stop_distance[usize::from(p.large)] {
                    eye[0] += tdiv(o.approach * dx, n);
                    eye[2] += tdiv(o.approach * dz, n);
                }
                let [m, sh] = o.target_easing;
                target[0] += sra_tz((rx - target[0]) * m, sh as u32);
                target[1] += sra_tz((p.chest - target[1]) * m, sh as u32);
                target[2] += sra_tz((rz - target[2]) * m, sh as u32);
                let (pt, y) = d.lookat6(*eye, *target);
                View { eye: *eye, pitch: sra_tz(pt, 6), yaw: sra_tz(y, 6), h: d.h }
            }
        }
    }
}
