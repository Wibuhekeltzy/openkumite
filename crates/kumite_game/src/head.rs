//! The head turning to the opponent (EXE 0x80039074, run by the skeleton 0x8003AFCC between the pose solver
//! and the world matrices). While the round is on and the move allows it (move record +0x24 bit 30; an attack
//! only from frame max(+0x2E, +0x18 − 17)), the head's local rotation is replaced by base × Rzyx(angles): the
//! angles head for the midpoint of the opponent's chest and head seen from the chest, pitch within −60°..25°,
//! yaw within ±80°..30° by how far the chest leans, half the way each frame at most 10°; while the opponent stays
//! near the line ahead the aim is renewed only every 60 frames. Taking over from the animation, the angles start
//! from the animated head. Everything here is in game space (y down, game units, 65536 per turn).

use glam::{Mat3, Vec3};
use kumite_sim::detmath::DetMath;

/// The head's base rotation (angles 0x8001A0F4: x 90°, z 90°).
const BASE: [i32; 3] = [0x4000, 0, 0x4000];

/// Rz · Ry · Rx of angles in 65536ths of a turn (EXE 0x8003A338, the pose solver's Euler angles).
pub fn rot_zyx(a: [i32; 3]) -> Mat3 {
    let r = |v: i32| v as f32 * std::f32::consts::TAU / 65536.0;
    Mat3::from_rotation_z(r(a[2])) * Mat3::from_rotation_y(r(a[1])) * Mat3::from_rotation_x(r(a[0]))
}

/// atan2(y, x) in 65536ths of a turn on the game's 16-step grid (0x8002A224).
fn ratan2(y: f32, x: f32) -> i32 {
    if x == 0.0 && y == 0.0 {
        return 0;
    }
    let a = (y.datan2(x) * 65536.0 / std::f32::consts::TAU).round() as i32;
    (a as i16 as i32) & !15
}

/// The pose the aim is taken from (game space).
pub struct Frame {
    /// The chest's world rotation (bone slot 1).
    pub chest: Mat3,
    /// The head's world position (slot 2) and its animated local rotation (relative to the chest).
    pub head: Vec3,
    pub head_local: Mat3,
    /// The target: the midpoint of the opponent's chest and head.
    pub target: Vec3,
    /// That character's head does not turn sideways.
    pub no_yaw: bool,
}

/// One fighter's state (0x8009E578 + 20 per player).
#[derive(Clone, Copy, Debug, Default)]
pub struct HeadLook {
    cur: [i32; 3],
    target: [i32; 3],
    active: bool,
    snap: bool,
    timer: u8,
    /// The head's local rotation this frame (game space), or None: the animation's.
    pub local: Option<Mat3>,
}

impl HeadLook {
    /// The angles (x, pitch, yaw) while the head follows.
    pub fn angles(&self) -> Option<[i32; 3]> {
        self.active.then_some(self.cur)
    }

    /// The round start (0x80039034): angles (0, 0x38E, 0), off, and the first aim taken at once.
    pub fn reset(&mut self) {
        self.cur = [0, 0x38E, 0];
        self.active = false;
        self.snap = true;
    }

    /// One frame: `on` whether the head follows this frame; `frame` the pose to aim from.
    pub fn step(&mut self, on: bool, frame: impl FnOnce() -> Frame) {
        let started = on && !self.active;
        self.active = on;
        if !on {
            self.local = None;
            return;
        }
        let f = frame();
        // the target in the chest's frame (0x8004B218: chestᵀ · d), and the chest's x axis in the world
        let d = f.target - f.head;
        let l = f.chest.transpose() * d;
        let w = f.chest.x_axis;
        let mut pitch = -ratan2(l.x, (l.y * l.y + l.z * l.z).sqrt());
        pitch = pitch.clamp(-0x2AAA, 0x11C7);
        let mut yaw = if f.no_yaw { 0 } else { ratan2(l.z, l.y) };
        // the sideways limit: 80° with the chest upright down to 30° bent over
        let lean = ratan2(w.y, (w.x * w.x + w.z * w.z).sqrt());
        let a0 = lean + 0x4000;
        let limit = if a0 < 0xAAB {
            0x38E3
        } else if a0 >= 0x38E3 {
            0x1555
        } else {
            0x38E3 - ((lean + 0x3556) as i64 * 9102 * 9 / 26 / 4096) as i32
        };
        yaw = yaw.clamp(-limit, limit);
        // renewed at once when taking over or when the opponent is off the line ahead, else every 60 frames
        let renew = if started || l.y.abs() >= 400.0 || l.z.abs() >= 300.0 {
            self.timer = 0;
            true
        } else {
            self.timer += 1;
            if self.timer >= 60 {
                self.timer = 0;
                true
            } else {
                false
            }
        };
        if renew {
            self.target = [0, pitch, yaw];
        }
        if self.snap {
            self.cur = self.target;
            self.snap = false;
        } else if started {
            self.cur = decompose(f.head_local);
        } else {
            let approach = |cur: i32, to: i32| {
                let diff = to - cur;
                let step = diff - diff / 2;
                cur + step.clamp(-0x71C, 0x71C)
            };
            self.cur = [self.target[0], approach(self.cur[1], self.target[1]) as i16 as i32, approach(self.cur[2], self.target[2]) as i16 as i32];
        }
        self.local = Some(rot_zyx(BASE) * rot_zyx(self.cur));
    }
}

/// The angles whose base × Rzyx gives `h` (0x800394AC): from its x and z columns.
fn decompose(h: Mat3) -> [i32; 3] {
    let (mut b0, mut b1) = (h.x_axis, h.z_axis);
    let rad = |a: i32| a as f32 * std::f32::consts::TAU / 65536.0;
    let tz = ratan2(b0.z, b0.y);
    let (s, c) = rad(-tz).dsin_cos();
    (b0.y, b0.z) = (b0.y * c - b0.z * s, b0.y * s + b0.z * c);
    (b1.y, b1.z) = (b1.y * c - b1.z * s, b1.y * s + b1.z * c);
    let ty = (ratan2(b0.y, b0.x) - 0x4000) as i16 as i32;
    let (s, c) = rad(-ty).dsin_cos();
    b1.x = b1.x * c - b1.y * s;
    let tx = (ratan2(b1.x, b1.z) - 0x4000) as i16 as i32;
    [tx, ty, tz]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_inverts_base_times_euler() {
        for a in [[100, -2000, 3000], [-5000, 4000, -1000], [0, 0x38E, 0], [6000, -6000, 2000]] {
            let h = rot_zyx(BASE) * rot_zyx(a);
            let back = rot_zyx(BASE) * rot_zyx(decompose(h));
            assert!((h - back).to_cols_array().iter().all(|v| v.abs() < 0.01), "{a:?}");
        }
    }

    #[test]
    fn turns_half_way_capped() {
        let mut h = HeadLook::default();
        // an upright chest: its x axis (the spine) points up (−y in game space), y ahead (+x)
        let chest = Mat3::from_cols(Vec3::new(0.0, -1.0, 0.0), Vec3::X, Vec3::Z);
        let frame = |target: Vec3| Frame { chest, head: Vec3::ZERO, head_local: rot_zyx(BASE), target, no_yaw: false };
        // taking over: the animated head (angles 0); then towards an opponent off to the side (+z, yaw 90° → 80°)
        h.step(true, || frame(Vec3::new(1.0, 0.0, 2000.0)));
        assert_eq!(h.cur, [0, 0, 0]);
        h.step(true, || frame(Vec3::new(1.0, 0.0, 2000.0)));
        assert_eq!(h.target[2], 0x38E3);
        assert_eq!(h.cur[2], 0x71C, "at most 10° a frame");
        for _ in 0..40 {
            h.step(true, || frame(Vec3::new(1.0, 0.0, 2000.0)));
        }
        assert!((h.cur[2] - 0x38E3).abs() <= 16);
        h.step(false, || frame(Vec3::ZERO));
        assert!(h.local.is_none());
    }
}
