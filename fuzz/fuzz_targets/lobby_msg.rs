//! A room message from another computer: any bytes.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = kumite_net::lobby_msg(data);
});
