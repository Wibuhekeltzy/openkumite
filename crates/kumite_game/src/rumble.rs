//! DualShock vibration as the original drives it (gamedata/rumble.json): a request (0x80028EC0: pad, pattern)
//! puts the pattern's two scripts (small motor, large motor) into its priority slot; a 60 Hz tick (0x800291D8)
//! steps every slot's channel and, per motor, the highest active slot gives the output: the large motor
//! L * 16 + 15, the small one on or off by bit (tick & 7) of its level's duty mask. (Pads with proportional
//! motors get that duty as the small motor's strength: [`Rumble::tick`].) The fight's events request
//! the patterns (0x800759D4, [`Rumble::fight`]): blocks, hits (light, heavy, close, counter), throws, a floor hit,
//! the K.O., a ki charge, the moves' shakes, ground impacts and bone effects, fire attacks.

use kumite_sim::{Fight, GameData, MoveEffect};
use serde::Deserialize;

/// A script step: frames, level 0..15.
type Script = Vec<[u32; 2]>;

#[derive(Clone, Debug, Deserialize)]
struct Pattern {
    slot: usize,
    small: Option<Script>,
    large: Option<Script>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RumbleData {
    small_masks: Vec<u8>,
    patterns: Vec<Pattern>,
    /// The patterns the fight's events request (none: the fight does not vibrate).
    #[serde(default)]
    requests: Option<Requests>,
}

/// The pattern each event requests (rumble.json `requests`, 0x800759D4): on the fighter's own pad, on the other's
/// (`_attacker`, `_other`, `thrower`), and the moves' effects.
#[derive(Clone, Copy, Debug, Deserialize)]
struct Requests {
    blocked: usize,
    hit: usize,
    heavy: usize,
    close: usize,
    counter: usize,
    thrown: usize,
    floor: usize,
    ko: usize,
    ki: usize,
    ki_other: usize,
    block_attacker: usize,
    hit_attacker: usize,
    heavy_attacker: usize,
    close_attacker: usize,
    counter_attacker: usize,
    thrower: usize,
    ko_other: usize,
    fire_hit: usize,
    fire_attacker: usize,
    bone_own: usize,
    bone_other: usize,
    ground: usize,
    breath: usize,
    /// A fire attack's, and KEY CONFIGURATION's test.
    test: usize,
    fire_descriptors: FireDescriptors,
}

/// The fire moves' descriptor bytes (byte 0, 0x8006F120): the breath's and the other fire's (the `test` pattern).
#[derive(Clone, Copy, Debug, Deserialize)]
struct FireDescriptors {
    breath: u32,
    test: u32,
}

impl FireDescriptors {
    fn has(&self, limb: u32) -> bool {
        limb == self.breath || limb == self.test
    }
}

impl RumbleData {
    pub fn load(assets: &kumite_assets::Assets) -> Option<Self> {
        let bytes = kumite_fs::read(&assets.root.join("gamedata/rumble.json")).ok()?;
        serde_json::from_slice(&bytes).map_err(|e| log::warn!("rumble.json: {e}")).ok()
    }
}

/// One slot's running script: the pattern, the step and its frames left.
#[derive(Clone, Copy, Debug)]
struct Channel {
    pattern: usize,
    step: usize,
    left: u32,
}

/// A clean hit's damage from which it is a heavy one (0x800759D4: +0xE8 >= 21).
const HEAVY_DAMAGE: i32 = 21;
/// Pads (the original's two; OpenKumite's P3 / P4 too).
pub const PADS: usize = 4;

/// The pads' vibration.
#[derive(Clone, Debug)]
pub struct Rumble {
    data: Option<RumbleData>,
    /// Per pad, motor (small, large) and slot.
    channels: [[[Option<Channel>; 4]; 2]; PADS],
    /// The tick counter (0x80097F00; the DualShock's small motor duty).
    tick: u32,
    /// Per fighter: health and move last frame (the K.O., a move's start).
    last: Vec<(i32, Option<(kumite_sim::Current, u32)>)>,
}

impl Rumble {
    pub fn new(data: Option<RumbleData>) -> Self {
        Self { data, channels: [[[None; 4]; 2]; PADS], tick: 0, last: Vec::new() }
    }

    /// Everything off (0x80028F5C: mode changes, the title, the menus).
    pub fn stop(&mut self) {
        self.channels = [[[None; 4]; 2]; PADS];
        self.last.clear();
    }

    /// KEY CONFIGURATION's vibration test on `pad`.
    pub fn request_test(&mut self, pad: usize) {
        if let Some(q) = self.data.as_ref().and_then(|d| d.requests) {
            self.request(pad, q.test);
        }
    }

    /// Request `pattern` on `pad` (0x80028EC0): each of its scripts replaces its slot's channel on that motor;
    /// a motor it has no script for goes on as it was.
    pub fn request(&mut self, pad: usize, pattern: usize) {
        let Some(p) = self.data.as_ref().and_then(|d| d.patterns.get(pattern)) else { return };
        let Some(pad) = self.channels.get_mut(pad) else { return };
        let slot = p.slot.min(3);
        for (motor, script) in [&p.small, &p.large].into_iter().enumerate() {
            if let Some(first) = script.as_ref().and_then(|s| s.first()) {
                pad[motor][slot] = Some(Channel { pattern, step: 0, left: first[0].max(1) });
            }
        }
    }

    /// One 60 Hz tick (0x800291D8): every pad's motors now, then the scripts step on (a step's last frame still
    /// plays). The small motor: its duty mask's share of the 8 ticks (0..1; the DualShock's on / off motor ran it
    /// at bit (tick & 7), a proportional motor gets the strength that gave); the large one 0..255.
    pub fn tick(&mut self) -> [(f32, u8); PADS] {
        let Some(data) = self.data.as_ref() else { return [(0.0, 0); PADS] };
        let mut out = [(0.0f32, 0u8); PADS];
        self.tick = self.tick.wrapping_add(1);
        for (pad, motors) in self.channels.iter_mut().enumerate() {
            for (motor, slots) in motors.iter_mut().enumerate() {
                let script = |c: &Channel| {
                    let p = &data.patterns[c.pattern];
                    if motor == 0 {
                        p.small.as_ref()
                    } else {
                        p.large.as_ref()
                    }
                };
                // the highest active slot gives the motor
                if let Some(c) = slots.iter().rev().flatten().next() {
                    if let Some(&[_, level]) = script(c).and_then(|s| s.get(c.step)) {
                        let level = level.min(15) as usize;
                        if motor == 0 {
                            out[pad].0 = data.small_masks.get(level).map_or(0.0, |m| m.count_ones() as f32 / 8.0);
                        } else {
                            out[pad].1 = (level * 16 + 15) as u8;
                        }
                    }
                }
                for slot in slots.iter_mut() {
                    let Some(c) = slot else { continue };
                    c.left = c.left.saturating_sub(1);
                    if c.left == 0 {
                        let next = script(c).and_then(|s| s.get(c.step + 1)).map(|s| s[0].max(1));
                        match next {
                            Some(n) => {
                                c.step += 1;
                                c.left = n;
                            }
                            None => *slot = None,
                        }
                    }
                }
            }
        }
        out
    }

    /// A fight frame's events (0x800759D4, after the step): `fighters` (fighter, its pad, the other's pad) of
    /// `fight`. Pass 1 the fighter's own pad, pass 2 the other's (0x1A, 4, 5, 6, 8, 9, 7, 0xB); the moves'
    /// effects first (their ops ran in the step). Later requests in the same slot win.
    pub fn fight(&mut self, fight: &Fight, gd: &GameData, fighters: &[(&kumite_sim::Fighter, usize, usize)]) {
        let Some(q) = self.data.as_ref().and_then(|d| d.requests) else { return };
        if self.last.len() != fighters.len() {
            self.last = fighters.iter().map(|(f, _, _)| (f.health, None)).collect();
        }
        let mut own: Vec<Vec<usize>> = vec![Vec::new(); fighters.len()];
        let mut other: Vec<Vec<usize>> = vec![Vec::new(); fighters.len()];
        let hits_on = |i: usize| fight.hits.iter().filter(move |h| h.victim == i);
        for (i, (f, pad, opp)) in fighters.iter().enumerate() {
            let mv = f.current_move(gd);
            // the moves' effects: shakes on both pads (op n -> pattern n + 1), ground impacts, effects at a bone
            for e in &f.effects {
                match *e {
                    MoveEffect::Shake(n) => {
                        self.request(*pad, n as usize + 1);
                        self.request(*opp, n as usize + 1);
                    }
                    MoveEffect::GroundImpact => self.request(*pad, q.ground),
                    MoveEffect::AtBone { op: 6, .. } => {
                        self.request(*pad, q.bone_own);
                        self.request(*opp, q.bone_other);
                    }
                    _ => {}
                }
            }
            // a fire attack's first active frame (the breath's descriptor, the other fire's)
            let started = f.started_with.is_some();
            let moved = self.last[i].1 != Some((f.current, f.frame));
            if moved && f.frame == mv.active[0] {
                match mv.attack.as_ref().and_then(|a| a.header_bytes.first().copied()) {
                    Some(b) if b == q.fire_descriptors.breath => self.request(*pad, q.breath),
                    Some(b) if b == q.fire_descriptors.test => self.request(*pad, q.test),
                    _ => {}
                }
            }
            // pass 1: what happened to this fighter
            let ki = started && mv.flags24.raw & 0x10000 != 0;
            if ki {
                own[i].push(q.ki);
            }
            for h in hits_on(i) {
                if h.blocked {
                    own[i].push(q.blocked);
                } else if h.throw {
                    own[i].push(q.thrown);
                } else {
                    own[i].push(if h.damage < HEAVY_DAMAGE { q.hit } else { q.heavy });
                    if h.clean {
                        own[i].push(q.close);
                    }
                    if h.counter {
                        own[i].push(q.counter);
                    }
                }
                if q.fire_descriptors.has(h.limb) && !h.blocked {
                    own[i].push(q.fire_hit);
                }
            }
            if f.landing == 1 {
                own[i].push(q.floor);
            }
            let ko = self.last[i].0 > 0 && f.health <= 0;
            if ko {
                own[i].push(q.ko);
            }
            // pass 2: the same flags on the other's pad (the attacker's, the thrower's, the opponent's)
            if ki {
                other[i].push(q.ki_other);
            }
            for h in hits_on(i) {
                if h.blocked {
                    other[i].push(q.block_attacker);
                } else if !h.throw {
                    other[i].push(if h.damage < HEAVY_DAMAGE { q.hit_attacker } else { q.heavy_attacker });
                    if h.clean {
                        other[i].push(q.close_attacker);
                    }
                    if h.counter {
                        other[i].push(q.counter_attacker);
                    }
                    if q.fire_descriptors.has(h.limb) {
                        other[i].push(q.fire_attacker);
                    }
                }
            }
            if hits_on(i).any(|h| h.throw) {
                other[i].push(q.thrower);
            }
            // the K.O. shakes the other pad too when a clean hit or a throw made it
            if ko && hits_on(i).any(|h| !h.blocked) {
                other[i].push(q.ko_other);
            }
            self.last[i] = (f.health, Some((f.current, f.frame)));
        }
        for (i, (_, pad, _)) in fighters.iter().enumerate() {
            for &p in &own[i] {
                self.request(*pad, p);
            }
        }
        for (i, (_, _, opp)) in fighters.iter().enumerate() {
            for &p in &other[i] {
                self.request(*opp, p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> RumbleData {
        // made-up masks (level 13: on 6 ticks of 8, 15: always on);
        // pattern 0: small 3 frames at level 15 (always on), large 2 frames at 4 then 1 at 15; pattern 1: slot 2,
        // large only 2 frames at 8; pattern 2: slot 0, no scripts
        serde_json::from_str(
            r#"{"small_masks":[0,1,2,4,8,16,32,64,1,3,5,9,17,63,127,255],
               "patterns":[{"slot":1,"small":[[3,15]],"large":[[2,4],[1,15]]},
                           {"slot":2,"small":null,"large":[[2,8]]},
                           {"slot":0,"small":null,"large":null}]}"#,
        )
        .unwrap()
    }

    #[test]
    fn scripts_and_slots() {
        let mut r = Rumble::new(Some(data()));
        r.request(0, 0);
        let frames: Vec<_> = (0..4).map(|_| r.tick()[0]).collect();
        assert_eq!(frames, vec![(1.0, 79), (1.0, 79), (1.0, 255), (0.0, 0)], "steps, each frame of a step plays");
        // a higher slot wins its motor; the other motor goes on
        r.request(0, 0);
        r.request(0, 1);
        assert_eq!(r.tick()[0], (1.0, 143));
        assert_eq!(r.tick()[0], (1.0, 143));
        assert_eq!(r.tick()[0], (1.0, 255), "slot 2 done: slot 1's large again");
        // an empty pattern leaves the motors alone; stop clears all
        r.request(1, 0);
        r.request(1, 2);
        assert_eq!(r.tick()[1], (1.0, 79));
        r.stop();
        assert_eq!(r.tick()[1], (0.0, 0));
    }

    #[test]
    fn small_motor_duty() {
        let mut d = data();
        d.patterns[0].small = Some(vec![[8, 13]]);
        let mut r = Rumble::new(Some(d));
        r.request(0, 0);
        // mask 63 = 0b0011_1111: on 6 ticks of 8
        assert_eq!(r.tick()[0].0, 0.75);
    }
}
