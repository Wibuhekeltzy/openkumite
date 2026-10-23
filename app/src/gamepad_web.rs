//! Gamepads in a browser (the Gamepad API, its standard layout; and the page's on-screen pad on touch screens), as
//! the native ones: the first pad seen plays P1,
//! the second P2 (the tag brawl: up to P4), the buttons as the player's KEY CONFIGURATION says. A browser shows pads only
//! after a button is pressed on one with the page open.

use crate::controls::{merge, raw_menu_key, Controls, PadButton, Step};
use crate::menu::Key;
use kumite_sim::input::Pad;
use wasm_bindgen::JsCast;

/// The standard layout's buttons by index (w3c.github.io/gamepad, "standard" mapping).
const STANDARD: [PadButton; 17] = [
    PadButton::South,
    PadButton::East,
    PadButton::West,
    PadButton::North,
    PadButton::L1,
    PadButton::R1,
    PadButton::L2,
    PadButton::R2,
    PadButton::Select,
    PadButton::Start,
    PadButton::L3,
    PadButton::R3,
    PadButton::Up,
    PadButton::Down,
    PadButton::Left,
    PadButton::Right,
    PadButton::Home,
];

/// One pad's state: the buttons held, the left stick (y up positive).
#[derive(Clone, Default)]
struct State {
    held: Vec<PadButton>,
    stick: (f32, f32),
}

pub struct Gamepads {
    /// The browser's pad indices in the order they were first seen (index = player).
    order: Vec<u32>,
    /// Each player's pad as last read (for the presses).
    last: [State; 4],
    /// Buttons pressed since the last step (a tap shorter than a frame is not lost).
    tapped: [Pad; 4],
    /// The sidestep buttons, per player.
    steps: [Step; 4],
    /// Rumble: each player's motors as last set (large, small), and when they were last sent.
    rumble: [(u8, f32, Option<web_time::Instant>); 4],
}

/// While a level lasts it is sent again this often (each effect plays a little longer, so it never gaps).
const RENEW_MS: u128 = 300;

/// The pads connected now: (browser index, state).
fn read() -> Vec<(u32, State)> {
    let mut out = Vec::new();
    // (a page that is not a secure context, e.g. plain http on the local network, has no Gamepad API: the
    // on-screen pad still plays)
    let pads = web_sys::window().map(|w| w.navigator()).and_then(|nav| nav.get_gamepads().ok());
    for v in pads.iter().flat_map(|p| p.iter()) {
        let Ok(pad) = v.dyn_into::<web_sys::Gamepad>() else { continue };
        if !pad.connected() {
            continue;
        }
        let buttons = pad.buttons();
        let held = STANDARD.iter().enumerate().filter(|(i, _)| buttons.get(*i as u32).dyn_into::<web_sys::GamepadButton>().is_ok_and(|b| b.pressed())).map(|(_, b)| *b).collect();
        let axes = pad.axes();
        let axis = |i: u32| axes.get(i).as_f64().unwrap_or(0.0) as f32;
        out.push((pad.index(), State { held, stick: (axis(0), -axis(1)) }));
    }
    // the page's on-screen pad (touch screens): `window.__oktouch`, the standard layout's buttons held
    if let Some(held) = touch_pad() {
        out.push((TOUCH_INDEX, State { held, stick: (0.0, 0.0) }));
    }
    out
}

/// The on-screen pad's index among the pads (above any the browser numbers).
const TOUCH_INDEX: u32 = 1000;

/// The page's on-screen pad (web/index.html, shown on touch screens): an array of 17 booleans in the standard
/// layout's order, or None when the page has none.
fn touch_pad() -> Option<Vec<PadButton>> {
    let w = web_sys::window()?;
    let v = js_sys::Reflect::get(&w, &"__oktouch".into()).ok()?;
    let a: js_sys::Array = v.dyn_into().ok()?;
    Some(STANDARD.iter().enumerate().filter(|(i, _)| a.get(*i as u32).as_bool() == Some(true)).map(|(_, b)| *b).collect())
}

impl Gamepads {
    pub fn new() -> Self {
        Self { order: Vec::new(), last: Default::default(), tapped: [Pad::default(); 4], steps: [Step::default(); 4], rumble: [(0, 0.0, None); 4] }
    }

    fn player(&mut self, index: u32) -> Option<usize> {
        if !self.order.contains(&index) {
            self.order.push(index);
            log::info!("gamepad {index} plays P{}", self.order.len());
        }
        self.order.iter().position(|&g| g == index).filter(|&p| p < 4)
    }

    /// The buttons pressed since the last poll (player, button, its menu key); taps latched for the next step.
    pub fn poll(&mut self, controls: &Controls) -> Vec<(usize, PadButton, Option<Key>)> {
        let mut out = Vec::new();
        for (index, state) in read() {
            // (a pad joins with its first press)
            if !self.order.contains(&index) && state.held.is_empty() {
                continue;
            }
            let Some(player) = self.player(index) else { continue };
            for &b in &state.held {
                if !self.last[player].held.contains(&b) {
                    let layout = controls.pad(player);
                    out.push((player, b, raw_menu_key(b)));
                    self.tapped[player] = merge(self.tapped[player], layout.pad(std::iter::once(b), (0.0, 0.0)).0);
                }
            }
            self.last[player] = state;
        }
        out
    }

    /// Player `p`'s pad's motors this frame (the DualShock's: the large one 0..255; the small one's strength 0..1),
    /// through the pad's `vibrationActuator` ("dual-rumble": the strong and the weak motor).
    pub fn rumble(&mut self, p: usize, large: u8, small: f32) {
        use js_sys::{Function, Object, Reflect};
        let Some(&index) = self.order.get(p) else { return };
        let r = &mut self.rumble[p];
        let on = large > 0 || small > 0.0;
        let renew = on && r.2.is_none_or(|t| t.elapsed().as_millis() >= RENEW_MS);
        if (large, small) == (r.0, r.1) && !renew {
            return;
        }
        (r.0, r.1) = (large, small);
        r.2 = on.then(web_time::Instant::now);
        let Some(nav) = web_sys::window().map(|w| w.navigator()) else { return };
        // the on-screen pad: the phone's own motor, a short pulse while the large motor runs
        if index == TOUCH_INDEX {
            if on && large > 0 {
                let _ = nav.vibrate_with_duration(((RENEW_MS + 50) as f32 * large as f32 / 255.0) as u32);
            } else {
                let _ = nav.vibrate_with_duration(0);
            }
            return;
        }
        let Ok(pads) = nav.get_gamepads() else { return };
        let pad = pads.get(index);
        let Ok(actuator) = Reflect::get(&pad, &"vibrationActuator".into()) else { return };
        if actuator.is_undefined() || actuator.is_null() {
            return;
        }
        let call = |name: &str, args: &[wasm_bindgen::JsValue]| {
            if let Ok(f) = Reflect::get(&actuator, &name.into()).and_then(|f| f.dyn_into::<Function>()) {
                let args: js_sys::Array = args.iter().collect();
                // (the promise: refused or replaced, nothing to report)
                if let Ok(p) = Reflect::apply(&f, &actuator, &args).and_then(|p| p.dyn_into::<js_sys::Promise>()) {
                    thread_local! {
                        static IGNORE: wasm_bindgen::closure::Closure<dyn FnMut(wasm_bindgen::JsValue)> = wasm_bindgen::closure::Closure::new(|_| ());
                    }
                    IGNORE.with(|i| {
                        let _ = p.catch(i);
                    });
                }
            }
        };
        if on {
            let params = Object::new();
            let set = |k: &str, v: f64| {
                let _ = Reflect::set(&params, &k.into(), &v.into());
            };
            set("duration", (RENEW_MS + 200) as f64);
            set("startDelay", 0.0);
            set("strongMagnitude", large as f64 / 255.0);
            set("weakMagnitude", small.clamp(0.0, 1.0) as f64);
            call("playEffect", &["dual-rumble".into(), params.into()]);
        } else {
            call("reset", &[]);
        }
    }

    /// Each player's gamepad: the buttons held as last read (None: no pad).
    pub fn held(&self) -> [Option<Vec<PadButton>>; 4] {
        let now = read();
        std::array::from_fn(|p| self.order.get(p).and_then(|i| now.iter().find(|(x, _)| x == i)).map(|(_, s)| s.held.clone()))
    }

    /// Current pad of each player who has a gamepad, up to four (taps since the last call included).
    pub fn pads4(&mut self, controls: &Controls) -> [Option<Pad>; 4] {
        let mut out = [None; 4];
        let now = read();
        for (player, index) in self.order.iter().enumerate().take(4) {
            if let Some((_, s)) = now.iter().find(|(i, _)| i == index) {
                let (state, steps) = controls.pad(player).pad(s.held.iter().copied(), s.stick);
                out[player] = Some(self.steps[player].apply(merge(state, self.tapped[player]), steps));
            }
        }
        self.tapped = [Pad::default(); 4];
        out
    }
}
