//! Ball's 3D pictures (record 8 0x800B2CD0 / 0x800B38DC): the ball mesh in its type's face colours, lit,
//! spun and squashed, its visual modes (fading in and out, the white flash), its shadow on the floor, and the
//! court lines (the glowing net strip and the net-zone limits). Untextured triangles, sorted back to front as
//! the original hardware's ordering table draws them.

use crate::art::num;
use glam::{Mat3, Vec3};
use kumite_render::{Camera, StaticVertex};
use kumite_sim::ball::{to_sim, Ball, BallData};

const UNITS: f32 = kumite_game::UNITS;

fn vertex(p: Vec3, c: [f32; 4]) -> StaticVertex {
    StaticVertex { pos: (p / UNITS).to_array(), uv: [0.5, 0.5], color: c }
}

/// A 4096-per-turn angle in radians.
fn rad(a: i32) -> f32 {
    a as f32 * std::f32::consts::TAU / 4096.0
}

/// The ball, its shadow and the court lines (ball.json `constants.court_lines`) into the blended / additive lists.
#[allow(clippy::too_many_arguments)]
pub fn draw(ball: &Ball, data: &BallData, lines: &serde_json::Value, cam: &Camera, frame: u64, shade: Option<Vec3>, alpha: &mut Vec<StaticVertex>, add: &mut Vec<StaticVertex>) {
    court_lines(ball.kind.zone, lines, add);
    if !ball.visible() {
        return;
    }
    let centre = to_sim(ball.pos);
    // the spin, then the squash along its axis (S = diag(1 - s, 1 + s, 1 + s) in the axis frame)
    let spin = Mat3::from_rotation_z(rad(ball.spin[2])) * Mat3::from_rotation_y(rad(ball.spin[1])) * Mat3::from_rotation_x(rad(ball.spin[0]));
    let axis = Mat3::from_rotation_y(rad(ball.squash_axis[1])) * Mat3::from_rotation_z(rad(ball.squash_axis[2]));
    let s = ball.squash as f32 / 4096.0;
    let squash = axis * Mat3::from_diagonal(Vec3::new(1.0 - s, 1.0 + s, 1.0 + s)) * axis.transpose();
    let m = squash * spin;
    let mesh = &data.mesh;
    // game axes (y down) to the simulation's
    let flip = |v: [i32; 3]| Vec3::new(v[0] as f32, -v[1] as f32, -v[2] as f32);
    let verts: Vec<Vec3> = mesh.vertices.iter().map(|v| centre + m * flip(*v)).collect();
    let normals: Vec<Vec3> = mesh.normals.iter().map(|n| (spin * flip(*n)).normalize_or_zero()).collect();
    let light = Vec3::new(0.35, 0.8, 0.5).normalize();
    let ambient = Vec3::from_array(ball.kind.ambient.map(|c| c as f32 / 255.0));
    // the "!" pop pulses the back colour
    let ambient = if ball.visual == 4 {
        let t = menu_triangle((frame as u32) << 4) as f32 / 255.0;
        Vec3::splat(t)
    } else {
        ambient
    };
    let flat = |v: f32| [v, v, v, 1.0];
    let colour = |face: usize, tri: bool, normal: Vec3| -> [f32; 4] {
        match ball.visual {
            0 => flat(((-8 * ball.timer) & 0xFF) as f32 / 255.0),
            1 => flat(((8 * ball.timer - 1).max(0) & 0xFF) as f32 / 255.0),
            2 | 5 | 8 => flat(1.0),
            _ => {
                let k = if tri { face & 7 } else { 8 + (face & 7) };
                let c = ball.kind.colours.get(k).copied().unwrap_or([255; 3]);
                let lit = ambient + Vec3::splat(0.85 * normal.dot(light).max(0.0));
                [c[0] as f32 / 255.0 * lit.x.min(1.5), c[1] as f32 / 255.0 * lit.y.min(1.5), c[2] as f32 / 255.0 * lit.z.min(1.5), 1.0]
            }
        }
    };
    let mut tris: Vec<([Vec3; 3], [f32; 4])> = Vec::new();
    for (k, t) in mesh.triangles.iter().enumerate() {
        let n = (normals[t[0]] + normals[t[1]] + normals[t[2]]).normalize_or_zero();
        tris.push(([verts[t[0]], verts[t[1]], verts[t[2]]], colour(k, true, n)));
    }
    for (k, q) in mesh.quads.iter().enumerate() {
        let n = (normals[q[0]] + normals[q[1]] + normals[q[2]] + normals[q[3]]).normalize_or_zero();
        let c = colour(k, false, n);
        tris.push(([verts[q[0]], verts[q[1]], verts[q[2]]], c));
        tris.push(([verts[q[1]], verts[q[3]], verts[q[2]]], c));
    }
    let eye = cam.eye * UNITS;
    tris.sort_by(|a, b| {
        let d = |t: &[Vec3; 3]| ((t[0] + t[1] + t[2]) / 3.0).distance_squared(eye);
        d(&b.0).total_cmp(&d(&a.0))
    });
    // the shadow (0x800B3640): the mesh's equator (its vertices 18..25, at +0x90) on the floor straight below, turned
    // and squashed as the ball and smaller the higher it is (x and z × (y / 2 + 4096) / 4096); four flat opaque quads
    // in the stage's shadow colour
    if let (Some(shade), Some(octagon)) = (shade, mesh.vertices.get(18..26)) {
        let lin = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        let c = [lin(shade.x), lin(shade.y), lin(shade.z), 1.0];
        let k = ((ball.pos[1] as f32 / 2.0 + 4096.0) / 4096.0).max(0.0);
        let turn = Mat3::from_rotation_y(rad(ball.squash_axis[1]));
        let m = Mat3::from_rotation_y(rad(ball.spin[1])) * turn * Mat3::from_diagonal(Vec3::new(1.0 - s, 1.0 + s, 1.0 + s)) * turn.transpose();
        let ring: Vec<Vec3> = octagon
            .iter()
            .map(|&[x, _, z]| {
                let p = m * flip([x, 0, z]);
                Vec3::new(centre.x + p.x * k, 3.0, centre.z + p.z * k)
            })
            .collect();
        for q in 0..4 {
            let [a, b, c4, d] = [q, q + 1, (q + 5) & 7, (q + 4) & 7].map(|i| ring[i]);
            alpha.extend([vertex(a, c), vertex(b, c), vertex(c4, c), vertex(b, c), vertex(d, c), vertex(c4, c)]);
        }
    }
    for (t, c) in tris {
        alpha.extend(t.map(|p| vertex(p, c)));
    }
}

/// The court lines (0x800B38DC): an additive strip at the net and a line at each net-zone limit (±zone), from z0 in
/// steps of dz, their brightness by step.
fn court_lines(zone: i32, lines: &serde_json::Value, add: &mut Vec<StaticVertex>) {
    let bright: Vec<f32> = lines["brightness"].as_array().into_iter().flatten().map(num).collect();
    let (z0, dz, half) = (num(&lines["z0"]), num(&lines["dz"]), num(&lines["centre_half_width"]));
    let z = |k: usize| -(z0 + dz * k as f32);
    let y = 2.0;
    let quad = |add: &mut Vec<StaticVertex>, x0: f32, x1: f32, k: usize| {
        let (c0, c1) = (bright[k] / 255.0, bright[k + 1] / 255.0);
        let (a, b) = ([c0, c0, c0, 0.0], [c1, c1, c1, 0.0]);
        let p = [Vec3::new(x0, y, z(k)), Vec3::new(x1, y, z(k)), Vec3::new(x0, y, z(k + 1)), Vec3::new(x1, y, z(k + 1))];
        add.extend([vertex(p[0], a), vertex(p[1], a), vertex(p[2], b), vertex(p[1], a), vertex(p[3], b), vertex(p[2], b)]);
    };
    for k in 0..bright.len().saturating_sub(1) {
        quad(add, -half, half, k);
        // the zone limits: lines (a few units wide here)
        for x in [-(zone as f32), zone as f32] {
            quad(add, x - 8.0, x + 8.0, k);
        }
    }
}

fn menu_triangle(a: u32) -> u32 {
    crate::menu::triangle(a)
}
