//! Gamepads (gilrs). The first pad seen plays P1, the second P2 (the tag brawl: up to P4). The buttons do what
//! the player's KEY CONFIGURATION says ([`crate::controls::PadLayout`]; by default the original's: square LP,
//! triangle RP, cross LK, circle RK, START, SELECT back); the d-pad or the left stick moves.

use crate::controls::{merge, raw_menu_key, Controls, PadButton, Step};
use crate::menu::Key;
use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};
use kumite_sim::input::Pad;

pub struct Gamepads {
    gilrs: Option<Gilrs>,
    /// Pads in the order they were first seen (index = player; a new pad takes a gone one's seat).
    order: Vec<GamepadId>,
    /// Buttons/directions pressed since the last step (a tap shorter than a frame is not lost).
    tapped: [Pad; 4],
    /// The sidestep buttons, per player.
    steps: [Step; 4],
    /// Rumble: each player's motors as last set, and when they were last sent.
    rumble: [Rumble; 4],
    #[cfg(target_os = "macos")]
    haptics: crate::rumble_mac::Haptics,
}

/// A pad's motors as the DualShock's: the large one 0..255, the small one's strength 0..1.
#[derive(Default)]
struct Rumble {
    large: u8,
    small: f32,
    #[cfg(not(target_os = "macos"))]
    sent: Option<web_time::Instant>,
    #[cfg(not(target_os = "macos"))]
    effect: Option<gilrs::ff::Effect>,
}

/// While a level lasts it is sent again this often (an effect plays a little longer, so it never gaps, and it
/// stops by itself if the game does).
#[cfg(not(target_os = "macos"))]
const RENEW_MS: u64 = 300;

/// A gilrs button's place.
pub fn pad_button(b: Button) -> Option<PadButton> {
    Some(match b {
        Button::South => PadButton::South,
        Button::East => PadButton::East,
        Button::West => PadButton::West,
        Button::North => PadButton::North,
        Button::LeftTrigger => PadButton::L1,
        Button::RightTrigger => PadButton::R1,
        Button::LeftTrigger2 => PadButton::L2,
        Button::RightTrigger2 => PadButton::R2,
        Button::Select => PadButton::Select,
        Button::Start => PadButton::Start,
        Button::LeftThumb => PadButton::L3,
        Button::RightThumb => PadButton::R3,
        Button::DPadUp => PadButton::Up,
        Button::DPadDown => PadButton::Down,
        Button::DPadLeft => PadButton::Left,
        Button::DPadRight => PadButton::Right,
        Button::Mode => PadButton::Home,
        _ => return None,
    })
}

const BUTTONS: [Button; 17] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::LeftThumb,
    Button::RightThumb,
    Button::DPadUp,
    Button::DPadDown,
    Button::DPadLeft,
    Button::DPadRight,
    Button::Mode,
];

impl Gamepads {
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(g) => {
                for (id, pad) in g.gamepads() {
                    log::info!("gamepad {id}: {}", pad.name());
                }
                Some(g)
            }
            Err(e) => {
                log::warn!("gamepads unavailable: {e}");
                None
            }
        };
        Self {
            gilrs,
            order: Vec::new(),
            tapped: [Pad::default(); 4],
            steps: [Step::default(); 4],
            rumble: Default::default(),
            #[cfg(target_os = "macos")]
            haptics: Default::default(),
        }
    }

    fn player(&mut self, id: GamepadId) -> Option<usize> {
        let gilrs = self.gilrs.as_ref();
        Some(seat(&mut self.order, id, |g| gilrs.is_some_and(|gilrs| gilrs.connected_gamepad(g).is_some()))).filter(|&p| p < 4)
    }

    /// Handle pending events: the buttons pressed (player, button, its menu key), and latch taps for the next
    /// step.
    pub fn poll(&mut self, controls: &Controls) -> Vec<(usize, PadButton, Option<Key>)> {
        let mut out = Vec::new();
        let mut events = Vec::new();
        if let Some(g) = self.gilrs.as_mut() {
            while let Some(e) = g.next_event() {
                events.push(e);
            }
        }
        for e in events {
            let Some(player) = self.player(e.id) else { continue };
            if let EventType::ButtonPressed(b, _) = e.event {
                let Some(b) = pad_button(b) else { continue };
                let layout = controls.pad(player);
                out.push((player, b, raw_menu_key(b)));
                let tap = layout.pad(std::iter::once(b), (0.0, 0.0)).0;
                self.tapped[player] = merge(self.tapped[player], tap);
            }
        }
        out
    }

    /// Player `p`'s pad's motors this frame (the DualShock's: the large one 0..255; the small one's strength 0..1).
    pub fn rumble(&mut self, p: usize, large: u8, small: f32) {
        let Some(&id) = self.order.get(p) else { return };
        let Some(g) = self.gilrs.as_mut() else { return };
        if !g.gamepad(id).is_connected() {
            return;
        }
        let r = &mut self.rumble[p];
        let changed = (large, small) != (r.large, r.small);
        (r.large, r.small) = (large, small);
        #[cfg(target_os = "macos")]
        {
            let _ = changed;
            let name = g.gamepad(id).name().to_string();
            self.haptics.set(&name, p, large as f32 / 255.0, small);
        }
        #[cfg(not(target_os = "macos"))]
        {
            use gilrs::ff::{BaseEffect, BaseEffectType, EffectBuilder, Replay, Ticks};
            let on = large > 0 || small > 0.0;
            let renew = on && r.sent.is_none_or(|t| t.elapsed().as_millis() as u64 >= RENEW_MS);
            if !changed && !renew {
                return;
            }
            // (dropping the effect stops it)
            r.effect = None;
            r.sent = None;
            if !on || !g.gamepad(id).is_ff_supported() {
                return;
            }
            let play = Replay { play_for: Ticks::from_ms(RENEW_MS as u32 + 200), ..Default::default() };
            let effect = EffectBuilder::new()
                .add_effect(BaseEffect { kind: BaseEffectType::Strong { magnitude: large as u16 * 257 }, scheduling: play, ..Default::default() })
                .add_effect(BaseEffect { kind: BaseEffectType::Weak { magnitude: (small.clamp(0.0, 1.0) * 65535.0) as u16 }, scheduling: play, ..Default::default() })
                .gamepads(&[id])
                .finish(g);
            if let Ok(e) = effect {
                if e.play().is_ok() {
                    r.effect = Some(e);
                    r.sent = Some(web_time::Instant::now());
                }
            }
        }
    }

    /// Each player's gamepad: the buttons held now (None: no pad).
    pub fn held(&self) -> [Option<Vec<PadButton>>; 4] {
        let mut out: [Option<Vec<PadButton>>; 4] = Default::default();
        let Some(g) = self.gilrs.as_ref() else { return out };
        for (player, id) in self.order.iter().enumerate().take(4) {
            let pad = g.gamepad(*id);
            if pad.is_connected() {
                out[player] = Some(BUTTONS.into_iter().filter(|&b| pad.is_pressed(b)).filter_map(pad_button).collect());
            }
        }
        out
    }

    /// Current pad of each player who has a gamepad, up to four (taps since the last call included).
    pub fn pads4(&mut self, controls: &Controls) -> [Option<Pad>; 4] {
        let mut out = [None; 4];
        let Some(g) = self.gilrs.as_ref() else { return out };
        for (player, id) in self.order.iter().enumerate().take(4) {
            let pad = g.gamepad(*id);
            if !pad.is_connected() {
                continue;
            }
            let held = BUTTONS.into_iter().filter(|&b| pad.is_pressed(b)).filter_map(pad_button);
            let (state, steps) = controls.pad(player).pad(held, (pad.value(Axis::LeftStickX), pad.value(Axis::LeftStickY)));
            out[player] = Some(self.steps[player].apply(merge(state, self.tapped[player]), steps));
        }
        self.tapped = [Pad::default(); 4];
        out
    }
}

/// The player of pad `id`: its seat, else the first whose pad is gone (a new pad after P1's leaves plays P1),
/// else the next.
fn seat<T: Copy + PartialEq + std::fmt::Display>(order: &mut Vec<T>, id: T, connected: impl Fn(T) -> bool) -> usize {
    if let Some(p) = order.iter().position(|&g| g == id) {
        return p;
    }
    let p = match order.iter().position(|&g| !connected(g)) {
        Some(p) => {
            order[p] = id;
            p
        }
        None => {
            order.push(id);
            order.len() - 1
        }
    };
    log::info!("gamepad {id} plays P{}", p + 1);
    p
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_pad_takes_a_gone_pads_seat() {
        let mut order = vec![1, 2];
        assert_eq!(super::seat(&mut order, 2, |_| true), 1);
        assert_eq!(super::seat(&mut order, 3, |g| g != 1), 0);
        assert_eq!(super::seat(&mut order, 4, |_| true), 2);
        assert_eq!(order, [3, 2, 4]);
    }
}
