//! A stage's glb: any bytes.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = kumite_assets::Stage::from_glb(data);
});
