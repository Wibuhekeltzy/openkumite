//! Gamepad rumble on macOS, where gilrs has none: the GameController framework's haptics (DualSense, DualShock 4,
//! Xbox pads; macOS 11+). A pad's two engines stand for the DualShock's motors: the left handle the large one,
//! the right handle the small one (one engine for both where a pad has no handles). A level is played as a
//! continuous haptic event, restarted when it changes and before it runs out while it lasts.
#![allow(unsafe_code)] // the GameController / Core Haptics FFI

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::AnyThread;
use objc2_core_haptics::{CHHapticEngine, CHHapticEvent, CHHapticEventParameter, CHHapticEventParameterIDHapticIntensity, CHHapticEventTypeHapticContinuous, CHHapticPattern, CHHapticPatternPlayer};
use objc2_foundation::NSArray;
use objc2_game_controller::{GCController, GCDevice, GCHapticsLocalityDefault, GCHapticsLocalityLeftHandle, GCHapticsLocalityRightHandle};
use web_time::{Duration, Instant};

/// How long one haptic event lasts; it is played again before it ends while the level holds.
const EVENT: f64 = 0.5;
const RENEW: Duration = Duration::from_millis(300);

/// One motor: its engine, the event playing, its level.
#[derive(Default)]
struct Motor {
    engine: Option<Retained<CHHapticEngine>>,
    player: Option<Retained<ProtocolObject<dyn CHHapticPatternPlayer>>>,
    level: f32,
    sent: Option<Instant>,
}

impl Motor {
    fn engine(controller: &GCController, locality: &objc2_game_controller::GCHapticsLocality) -> Option<Retained<CHHapticEngine>> {
        unsafe {
            let haptics = controller.haptics()?;
            // (the bindings leave this method out on macOS; it is there from macOS 11)
            let engine: Option<Retained<CHHapticEngine>> = objc2::msg_send![&*haptics, createEngineWithLocality: locality];
            let engine = engine?;
            engine.startAndReturnError().ok()?;
            Some(engine)
        }
    }

    fn stop(&mut self) {
        if let Some(p) = self.player.take() {
            let _ = unsafe { p.stopAtTime_error(0.0) };
        }
    }

    /// Play `level` (0..1): a new event when it changes, or before the one playing ends.
    fn set(&mut self, level: f32) {
        let renew = self.sent.is_some_and(|t| t.elapsed() >= RENEW);
        if (level - self.level).abs() < 1e-3 && !(level > 0.0 && renew) {
            return;
        }
        self.level = level;
        self.stop();
        self.sent = None;
        let Some(engine) = self.engine.as_ref() else { return };
        if level <= 0.0 {
            return;
        }
        let player = unsafe {
            let intensity = CHHapticEventParameter::initWithParameterID_value(CHHapticEventParameter::alloc(), CHHapticEventParameterIDHapticIntensity, level.min(1.0));
            let event =
                CHHapticEvent::initWithEventType_parameters_relativeTime_duration(CHHapticEvent::alloc(), CHHapticEventTypeHapticContinuous, &NSArray::from_retained_slice(&[intensity]), 0.0, EVENT);
            let pattern = CHHapticPattern::initWithEvents_parameters_error(CHHapticPattern::alloc(), &NSArray::from_retained_slice(&[event]), &NSArray::new());
            pattern.ok().and_then(|p| engine.createPlayerWithPattern_error(&p).ok())
        };
        if let Some(p) = player {
            if unsafe { p.startAtTime_error(0.0) }.is_ok() {
                self.player = Some(p);
                self.sent = Some(Instant::now());
            }
        }
    }
}

/// A pad's motors: the large (left), the small (right).
struct Pad {
    controller: Retained<GCController>,
    motors: [Motor; 2],
    /// One engine for both.
    single: bool,
}

#[derive(Default)]
pub struct Haptics {
    pads: Vec<Pad>,
}

impl Haptics {
    /// The controller for a gilrs pad: the one whose vendor name matches `name`, else the `index`-th.
    fn controller(name: &str, index: usize) -> Option<Retained<GCController>> {
        let all = unsafe { GCController::controllers() };
        let all: Vec<Retained<GCController>> = all.iter().collect();
        let named = |c: &&Retained<GCController>| {
            unsafe { c.vendorName() }.is_some_and(|v| {
                let v = v.to_string().to_lowercase();
                let n = name.to_lowercase();
                !v.is_empty() && (n.contains(&v) || v.contains(&n))
            })
        };
        all.iter().find(named).or_else(|| all.get(index)).cloned()
    }

    fn pad(&mut self, name: &str, index: usize) -> Option<&mut Pad> {
        let controller = Self::controller(name, index)?;
        if let Some(k) = self.pads.iter().position(|p| p.controller == controller) {
            return self.pads.get_mut(k);
        }
        let (left, right) = unsafe { (Motor::engine(&controller, GCHapticsLocalityLeftHandle), Motor::engine(&controller, GCHapticsLocalityRightHandle)) };
        let (motors, single) = match (left, right) {
            (Some(l), Some(r)) => ([Motor { engine: Some(l), ..Default::default() }, Motor { engine: Some(r), ..Default::default() }], false),
            _ => {
                let one = unsafe { Motor::engine(&controller, GCHapticsLocalityDefault) };
                ([Motor { engine: one, ..Default::default() }, Motor::default()], true)
            }
        };
        log::info!("rumble: {} ({})", unsafe { controller.vendorName() }.map(|v| v.to_string()).unwrap_or_default(), if single { "one engine" } else { "two handles" });
        self.pads.push(Pad { controller, motors, single });
        self.pads.last_mut()
    }

    /// The motors of the pad gilrs calls `name` (the `index`-th seen): the large and the small one 0..1.
    pub fn set(&mut self, name: &str, index: usize, large: f32, small: f32) {
        // (nothing to stop on a pad never rumbled)
        if large <= 0.0 && small <= 0.0 && !self.pads.iter().any(|p| p.motors.iter().any(|m| m.level > 0.0)) {
            return;
        }
        let Some(pad) = self.pad(name, index) else { return };
        if pad.single {
            pad.motors[0].set(large.max(small));
        } else {
            pad.motors[0].set(large);
            pad.motors[1].set(small);
        }
    }
}
