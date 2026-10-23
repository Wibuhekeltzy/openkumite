//! A real ggrs session fed a peer's messages (any bytes, split at each 0xFF 0xFE pair: each a message body, its
//! variant first, given the match's number and the peer's header here) as the link passes them: through kumite_net's
//! checks (`ggrs_message`, `InputGuard`), with the session running frames in between. The peer answers the
//! session's sync requests at once, so the session runs: ggrs must never panic on what gets through.
#![no_main]
use ggrs::{GgrsRequest, Message, NonBlockingSocket, PlayerType, SessionBuilder};
use kumite_net::{InputGuard, NetInput};
use libfuzzer_sys::fuzz_target;
use std::sync::{Arc, Mutex};

struct Cfg;
impl ggrs::Config for Cfg {
    type Input = NetInput;
    type InputPredictor = ggrs::PredictRepeatLast;
    type State = u64;
    type Address = u8;
}

/// The peer's header (its "magic").
const MAGIC: [u8; 2] = [0x34, 0x12];

/// A message of match 0 from the peer: its body's bytes after the header.
fn packet(body: &[u8]) -> Vec<u8> {
    let mut b = vec![0, 0, 0, 0];
    b.extend(MAGIC);
    b.extend(body);
    b
}

#[derive(Default)]
struct Wire {
    incoming: Vec<Vec<u8>>,
    guard: InputGuard,
}

struct Sock(Arc<Mutex<Wire>>);

impl NonBlockingSocket<u8> for Sock {
    fn send_to(&mut self, msg: &Message, _: &u8) {
        if let (Ok(mut w), Some(b)) = (self.0.lock(), kumite_net::ggrs_packet(0, msg)) {
            w.guard.sent(&b);
            // a sync request (variant 0, its random number): the peer's reply (variant 1, the number back)
            if b.get(6..10) == Some(&[0, 0, 0, 0]) {
                if let Some(random) = b.get(10..14) {
                    let mut reply = vec![1, 0, 0, 0];
                    reply.extend(random);
                    w.incoming.push(packet(&reply));
                }
            }
        }
    }
    fn receive_all_messages(&mut self) -> Vec<(u8, Message)> {
        let Ok(mut w) = self.0.lock() else { return Vec::new() };
        let w = &mut *w;
        let mut out = Vec::new();
        for b in std::mem::take(&mut w.incoming) {
            if let Some(m) = kumite_net::ggrs_message(&b, 0) {
                if w.guard.accept(1, &b) {
                    out.push((1, m));
                }
            }
        }
        out
    }
}

fuzz_target!(|data: &[u8]| {
    let wire = Arc::new(Mutex::new(Wire::default()));
    let Ok(b) = SessionBuilder::<Cfg>::new().with_num_players(2) else { return };
    let b = b
        .with_input_delay(kumite_net::INPUT_DELAY)
        .with_max_prediction_window(kumite_net::MAX_PREDICTION)
        .with_disconnect_notify_delay(kumite_net::QUIET_NOTICE)
        .with_disconnect_timeout(kumite_net::GGRS_TIMEOUT);
    let Ok(b) = b.add_player(PlayerType::Local, 0) else { return };
    let Ok(b) = b.add_player(PlayerType::Remote(1), 1) else { return };
    let Ok(mut s) = b.start_p2p_session(Sock(wire.clone())) else { return };
    // the sync's round trips (the peer answers each at once)
    for _ in 0..8 {
        s.poll_remote_clients();
    }
    assert_eq!(s.current_state(), ggrs::SessionState::Running, "the session synchronised");
    let mut state = 0u64;
    let mut watch: kumite_net::Watch<u8> = kumite_net::Watch::default();
    let mut packets: Vec<Vec<u8>> = vec![Vec::new()];
    for w in data.windows(2) {
        if w == [0xFF, 0xFE] {
            packets.push(Vec::new());
        } else if let Some(p) = packets.last_mut() {
            p.push(w[0]);
        }
    }
    for p in packets {
        if let Ok(mut w) = wire.lock() {
            w.incoming.push(packet(&p));
        }
        s.poll_remote_clients();
        // (the app's rule, kumite_net::Watch as NetPlay::tick uses it: no frame runs once the match is over)
        for e in s.events() {
            watch.event(&e);
        }
        if watch.over(&s, [1], false).is_some() {
            return;
        }
        if s.current_state() != ggrs::SessionState::Running || s.add_local_input(0, NetInput(1)).is_err() {
            continue;
        }
        let Ok(requests) = s.advance_frame() else { continue };
        for r in requests {
            match r {
                GgrsRequest::SaveGameState { cell, frame } => cell.save(frame, Some(state), None),
                GgrsRequest::LoadGameState { cell, .. } => state = cell.load().unwrap_or_default(),
                GgrsRequest::AdvanceFrame { inputs } => state = inputs.iter().fold(state, |h, i| h.wrapping_mul(131) ^ u64::from(i.0 .0)),
            }
        }
    }
});
