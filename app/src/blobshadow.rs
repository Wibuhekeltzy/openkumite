//! Force's shadows (0x800793B8): no fighter casts its shadow mesh there; under each one lies a blob,
//! four Gouraud quads (POLY_GT4, subtractive) around its two feet, darkest along the ridge between them, fading to
//! nothing at the rim. Lying down (posture 0x204, the head under 500 units), it stretches from the head instead.
//! Fixed size; straight down on the floor y = 0 (not along the light), whatever the fighter's height.

use kumite_render::StaticVertex;

const UNITS: f32 = kumite_game::UNITS;

/// The blob of a fighter whose feet (bone slots 14 and 17) and head (slot 2) stand at these game positions (y
/// down), `colour` the stage light table's +0x18: the triangles, each vertex's colour the amount taken off the
/// floor (the texel 31 × the colour / 128).
pub fn blob(foot_a: [i32; 3], foot_b: [i32; 3], head: [i32; 3], lying: bool, colour: [u8; 3]) -> Vec<StaticVertex> {
    let (mx, mz) = ((foot_b[0] + foot_a[0]) >> 1, (foot_b[2] + foot_a[2]) >> 1);
    let (mut px, mut pz, mut qx, mut qz) = (foot_b[0] - mx, foot_b[2] - mz, foot_a[0] - mx, foot_a[2] - mz);
    let (mut t0, mut a3, mut a2) = (160, 220, 300);
    if lying && head[1] >= -499 {
        (t0, a3, a2) = (200, 400, 400);
        let out = |v: i32| if v >= 0 { v + 128 } else { v - 128 };
        (px, pz) = (out(head[0] - mx), out(head[2] - mz));
        qx = if px >= 0 { -64 } else { 64 };
        qz = if pz >= 0 { -64 } else { 64 };
    }
    let (dx, dz) = ((px - qx).abs(), (pz - qz).abs());
    let v: [(i32, i32); 8] = if dz >= 2 * dx {
        // along z
        if pz < qz {
            (px, pz, qx, qz) = (qx, qz, px, pz);
        }
        if dz < 2 * a3 {
            (pz, qz) = (a3, -a3);
        }
        [(px, pz - t0), (px + a2, pz - a3), (px, pz), (px - a2, pz - a3), (qx - a2, qz + a3), (qx, qz), (qx + a2, qz + a3), (qx, qz + t0)]
    } else {
        let diagonal = (dx >> 1) < dz;
        if diagonal {
            (t0, a3, a2) = (t0 * 71 / 100, a3 * 71 / 100, a2 * 71 / 100);
        }
        if px < qx {
            (px, pz, qx, qz) = (qx, qz, px, pz);
        }
        if dx < 2 * a3 {
            (px, qx) = (a3, -a3);
        }
        let h = a2 >> 2;
        match (diagonal, pz < 0) {
            (false, _) => [(px - t0, pz), (px - a3, pz - a2), (px, pz), (px - a3, pz + a2), (qx + a3, qz + a2), (qx, qz), (qx + a3, qz - a2), (qx + t0, qz)],
            (true, true) => [(px - t0, pz + t0), (px + h, pz + 2 * a2), (px, pz), (px - a2, pz - a3), (qx + a3 - h, qz - a2 - t0), (qx, qz), (qx + a2, qz + a3 + h), (qx + t0, qz - t0)],
            (true, false) => [(px - t0, pz - t0), (px + h, pz - 2 * a2), (px, pz), (px - a2, pz + a3), (qx + a3 - h, qz + a2 + t0), (qx, qz), (qx + a2, qz - a3 - h), (qx + t0, qz + t0)],
        }
    };
    let amount = colour.map(|c| c as f32 / 128.0);
    let vertex = |k: usize| {
        let (x, z) = v[k];
        // only the ridge (v0, v7) carries the colour
        let c = if k == 0 || k == 7 { [amount[0], amount[1], amount[2], 1.0] } else { [0.0, 0.0, 0.0, 1.0] };
        StaticVertex { pos: [(mx + x) as f32 / UNITS, 0.003, -((mz + z) as f32) / UNITS], uv: [0.0; 2], color: c }
    };
    // the quads (0x80098A54, read from its end), each two triangles as the GPU splits a quad
    [[1, 0, 2, 3], [3, 0, 4, 7], [4, 7, 5, 6], [6, 7, 1, 0]].iter().flat_map(|q| [q[0], q[1], q[2], q[1], q[3], q[2]]).map(vertex).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feet close together along x: the blob spreads to ±a3 (220) and ±a2 (300), dark only at its ridge.
    #[test]
    fn feet_along_x() {
        let v = blob([-100, 0, 0], [100, 0, 0], [0, -1500, 0], false, [64, 64, 64]);
        assert_eq!(v.len(), 24);
        let xs: Vec<f32> = v.iter().map(|p| p.pos[0] * UNITS).collect();
        let zs: Vec<f32> = v.iter().map(|p| -p.pos[2] * UNITS).collect();
        assert!(xs.iter().all(|x| x.abs() <= 220.5) && xs.iter().any(|x| (x - 220.0).abs() < 0.5));
        assert!(zs.iter().all(|z| z.abs() <= 300.5) && zs.iter().any(|z| (z + 300.0).abs() < 0.5));
        assert!(v.iter().all(|p| p.color[0] == 0.0 || p.color[0] == 0.5));
    }
}
