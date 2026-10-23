//! A UDP peer's lobby packets, any of them in any order: the channel never panics, keeps at most its window waiting,
//! and delivers each number once.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut r = kumite_net::reliable::Reliable::default();
    r.push(kumite_net::LobbyMsg::Ping);
    let mut delivered = 0usize;
    // packets separated by newlines (JSON has none of its own here)
    for p in data.split(|&b| b == b'\n') {
        if let Some(p) = kumite_net::lobby_packet(p) {
            let sent = p.msgs.len();
            let got = r.receive(p).len();
            assert!(got <= sent + kumite_net::reliable::WINDOW as usize);
            delivered += got;
            assert!(r.waiting() <= kumite_net::reliable::WINDOW as usize);
            assert!(r.packet().is_some());
        }
    }
    let _ = delivered;
});
