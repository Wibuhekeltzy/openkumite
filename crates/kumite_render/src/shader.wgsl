// OpenKumite shaders. Positions are in the converted assets' space (metres, y up).

struct Globals {
    view_proj: mat4x4<f32>,
    sky_view_proj: mat4x4<f32>,
    floor: mat4x4<f32>,
    light_dir: vec4<f32>,
    // the hit light (0x80039F0C): colour × strength, and where it is (metres)
    hit_colour: vec4<f32>,
    hit_pos: vec4<f32>,
    // the stage meshes kept inside x0..x1, z0..z1 (metres) when x0 < x1: the floor's finite patch
    floor_clip: vec4<f32>,
    // the extra stages' floor lighting: x mode (0 none, 1 depth fade, 2 from each fighter, 3 from their midpoint), y shade
    floor_light: vec4<f32>,
    fighters: vec4<f32>,
    eye: vec4<f32>,
    forward: vec4<f32>,
    fade: array<vec4<f32>, 8>,
    // an arena with walls: the fighter nearer the camera (xyz, metres) and the margin (w; 0 = off)
    cutaway: vec4<f32>,
    cutaway2: vec4<f32>,
    // a level laid along a route (Force's extra stages): above the floor, nothing beyond z = x (the camera's
    // side) is drawn (y = 1: on)
    near_cut: vec4<f32>,
    // the stage's light for the fighters (w = 1 when there is one): key colour, ambient, material (128 = 1)
    key_color: vec4<f32>,
    ambient: vec4<f32>,
    material: vec4<f32>,
    // the fighters' shadows: the light's x / y and z / y, the floor's height, w = 1 when on; their colour (a: alpha)
    shadow: vec4<f32>,
    shadow_colour: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var<uniform> bones: array<mat4x4<f32>, 64>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) normal: vec3<f32>,
};

// Standard 4-joint linear blend skinning.
@vertex
fn vs_skinned(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>,
              @location(2) joints: vec4<u32>, @location(3) weights: vec4<f32>, @location(4) normal: vec3<f32>) -> VsOut {
    let skin = weights.x * bones[joints.x] + weights.y * bones[joints.y]
             + weights.z * bones[joints.z] + weights.w * bones[joints.w];
    let p = (skin * vec4(pos, 1.0)).xyz;
    var o: VsOut;
    o.clip = g.view_proj * vec4(p, 1.0);
    o.uv = uv;
    o.color = vec4(1.0);
    o.world = p;
    o.normal = (skin * vec4(normal, 0.0)).xyz;
    return o;
}

// A fighter's shadow (0x80035F2C's matrix): each vertex moved along the light onto the floor.
@vertex
fn vs_shadow(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>,
             @location(2) joints: vec4<u32>, @location(3) weights: vec4<f32>, @location(4) normal: vec3<f32>) -> VsOut {
    let skin = weights.x * bones[joints.x] + weights.y * bones[joints.y]
             + weights.z * bones[joints.z] + weights.w * bones[joints.w];
    var p = (skin * vec4(pos, 1.0)).xyz;
    let h = p.y - g.shadow.z;
    p = vec3(p.x - h * g.shadow.x, g.shadow.z + 0.003, p.z - h * g.shadow.y);
    var o: VsOut;
    o.clip = g.view_proj * vec4(p, 1.0);
    o.uv = uv;
    o.color = vec4(1.0);
    o.world = p;
    o.normal = vec3(0.0);
    return o;
}

@fragment
fn fs_shadow(i: VsOut) -> @location(0) vec4<f32> {
    return g.shadow_colour;
}

// World-space triangles (metres), not moved with the floor.
@vertex
fn vs_world(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.clip = g.view_proj * vec4(pos, 1.0);
    o.uv = uv;
    o.color = color;
    o.world = pos;
    o.normal = vec3(0.0);
    return o;
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

// A blob shadow: the original hardware takes the amount off the displayed colour (subtraction in gamma space). The frame here
// is linear and its colour unknown to the shader, so the factor is that subtraction's for a mid-grey floor,
// multiplied in.
@fragment
fn fs_blob(i: VsOut) -> @location(0) vec4<f32> {
    let grey = 0.5;
    let k = srgb_to_linear(max(vec3(grey) - i.color.rgb, vec3(0.0))) / srgb_to_linear(vec3(grey));
    return vec4(k, 1.0);
}

@vertex
fn vs_static(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VsOut {
    var o: VsOut;
    let p = (g.floor * vec4(pos, 1.0)).xyz;
    o.clip = g.view_proj * vec4(p, 1.0);
    o.uv = uv;
    o.color = color;
    o.world = p;
    o.normal = vec3(0.0);
    return o;
}

// The backdrop drum: rotation-only view, so it stays centred on the camera; drawn first, no depth.
@vertex
fn vs_sky(@location(0) pos: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.clip = g.sky_view_proj * vec4(pos, 1.0);
    o.uv = uv;
    o.color = color;
    o.world = vec3(0.0);
    o.normal = vec3(0.0);
    return o;
}

@fragment
fn fs_unlit(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.5) { discard; }
    return vec4(t.rgb * i.color.rgb, 1.0);
}

// Stage meshes: unlit, cut to the floor patch when one is set.
@fragment
fn fs_static(i: VsOut) -> @location(0) vec4<f32> {
    let c = g.floor_clip;
    if (c.x < c.y && (i.world.x < c.x || i.world.x > c.y || i.world.z < c.z || i.world.z > c.w)) { discard; }
    // walls between the camera and the fight are cut away: above the floor, inside a 2 m tube around the line from
    // the camera to either fighter's chest, and nearer the camera than that fighter by more than the margin
    if (g.cutaway.w > 0.0 && i.world.y > 0.05) {
        if (in_tube(i.world, g.cutaway.xyz, g.cutaway.w) || in_tube(i.world, g.cutaway2.xyz, g.cutaway.w)) { discard; }
        // the camera raised over the walls: everything nearer than the nearer fighter goes
        let f = g.forward.xyz;
        let near = min(dot(g.cutaway.xyz - g.eye.xyz, f), dot(g.cutaway2.xyz - g.eye.xyz, f));
        if (g.cutaway2.w > 0.5 && dot(i.world - g.eye.xyz, f) < near - g.cutaway.w) { discard; }
    }
    if (g.near_cut.y > 0.5 && i.world.y > 0.05 && i.world.z > g.near_cut.x) { discard; }
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.5) { discard; }
    return vec4(t.rgb * i.color.rgb * floor_light(i.world), 1.0);
}

fn in_tube(p: vec3<f32>, fighter: vec3<f32>, margin: f32) -> bool {
    let d = fighter + vec3(0.0, 1.0, 0.0) - g.eye.xyz;
    let v = p - g.eye.xyz;
    let len = length(d);
    let t = dot(v, d) / (len * len);
    return t < 1.0 - margin / len && length(v - t * d) < 2.0;
}

// The extra stages' floor colour (128 = 1.0, as drawn; up to 254 brightens, as the original hardware's texture modulation does)
fn fade_at(k: f32) -> f32 {
    let i = clamp(k / 32.0, 0.0, 31.0);
    let a = u32(floor(i));
    let b = min(a + 1u, 31u);
    return mix(g.fade[a / 4u][a % 4u], g.fade[b / 4u][b % 4u], i - f32(a));
}

fn floor_light(p: vec3<f32>) -> f32 {
    let mode = g.floor_light.x;
    if (mode < 0.5) { return 1.0; }
    if (mode < 1.5) {
        let sz = dot(p - g.eye.xyz, g.forward.xyz) * 1000.0;
        let k = clamp((sz - 10000.0) * 6710.0 / 65536.0, 0.0, 1023.0);
        return clamp(128.0 + fade_at(k) * g.floor_light.y / 128.0, 0.0, 254.0) / 128.0;
    }
    let a = (p.xz - g.fighters.xy) * 1000.0;
    let b = (p.xz - g.fighters.zw) * 1000.0;
    if (mode < 2.5) {
        return min(254.0, 134217728.0 / max(1.0, dot(a, a)) + 134217728.0 / max(1.0, dot(b, b))) / 128.0;
    }
    let m = (a + b) * 0.5;
    return min(192.0, 2048000000.0 / max(1.0, dot(m, m))) / 128.0;
}

// Characters: flat per-face normal from screen derivatives, one key light plus ambient
// (the engine lights per vertex with NCCS/NCCT; this is a stand-in until normals are ported).
@fragment
fn fs_lit(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.5) { discard; }
    let flat = normalize(cross(dpdx(i.world), dpdy(i.world)));
    // the hit light: a second light at the hit, in the attacker's colour, fading over 48 frames
    let to_hit = normalize(g.hit_pos.xyz - i.world + vec3(0.0, 0.0001, 0.0));
    if (g.key_color.w > 0.5) {
        // the stage's light (NCCS): ambient + key × max(0, n·L) + a fill from above at half the key; times the
        // material (128 = 1) and the texture
        var n = flat;
        if (dot(i.normal, i.normal) > 1e-8) {
            n = normalize(i.normal);
        }
        let lum = g.ambient.rgb + g.key_color.rgb * max(0.0, dot(n, g.light_dir.xyz)) + 0.5 * g.key_color.rgb * max(0.0, n.y);
        let hit = g.hit_colour.rgb * max(0.0, dot(n, to_hit));
        return vec4(min(t.rgb * (g.material.rgb * lum + hit), vec3(1.0)), 1.0);
    }
    let l = abs(dot(flat, g.light_dir.xyz));
    let h = abs(dot(flat, to_hit));
    return vec4(t.rgb * (0.55 + 0.6 * l + g.hit_colour.rgb * h), 1.0);
}

// Debug lines (hit volumes, skeleton, attack paths): world-space, vertex colour, drawn on top.
struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>) -> LineOut {
    var o: LineOut;
    o.clip = g.view_proj * vec4(pos, 1.0);
    o.color = color;
    return o;
}

@fragment
fn fs_line(i: LineOut) -> @location(0) vec4<f32> {
    return i.color;
}

// Effect sprites, coloured in the game's palettes (index 0 transparent). Additive: texture × colour added;
// half: texture × colour over the picture at the colour's alpha (the dust's 50 %).
@fragment
fn fs_particle_add(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.5) { discard; }
    return vec4(t.rgb * i.color.rgb, 0.0);
}

@fragment
fn fs_particle_alpha(i: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(tex, samp, i.uv);
    if (t.a < 0.5) { discard; }
    return vec4(t.rgb * i.color.rgb, i.color.a);
}
