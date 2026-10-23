//! A relayed packet's header: any bytes.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some((_, _, rest)) = kumite_net::relay_header(data) {
        assert_eq!(rest.len() + 16, data.len());
    }
});
