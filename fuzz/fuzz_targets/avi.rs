//! A movie arriving in pieces: any bytes, cut anywhere; every frame found lies inside the file.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&cut, d)) = data.split_first() else { return };
    let mut a = kumite_assets::avi::AviIndex::default();
    let step = (cut as usize).max(1);
    for n in (0..d.len()).step_by(step).chain([d.len()]) {
        if a.advance(&d[..n]).is_err() {
            return;
        }
    }
    for &(at, len) in &a.frames {
        assert!(at + len <= d.len());
    }
});
