//! A rollback packet from another computer: any bytes; what parses writes back as the same message.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some(m) = kumite_net::ggrs_message(data, 0) {
        let b = kumite_net::ggrs_packet(0, &m).expect("a message read writes back");
        assert_eq!(kumite_net::ggrs_message(&b, 0), Some(m));
    }
});
