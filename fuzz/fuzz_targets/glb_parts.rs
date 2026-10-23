//! A glb built here from its two parts (the JSON, a zero byte, the binary data): mutations reach the JSON's structure
//! instead of breaking the container. Stage and character alike: an error, never a panic.
#![no_main]
use libfuzzer_sys::fuzz_target;

fn glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
    let pad = |v: &[u8], with: u8| {
        let mut v = v.to_vec();
        while !v.len().is_multiple_of(4) {
            v.push(with);
        }
        v
    };
    let (j, b) = (pad(json, b' '), pad(bin, 0));
    let total = 12 + 8 + j.len() + 8 + b.len();
    let mut o = b"glTF".to_vec();
    o.extend(2u32.to_le_bytes());
    o.extend((total as u32).to_le_bytes());
    o.extend((j.len() as u32).to_le_bytes());
    o.extend(b"JSON");
    o.extend(j);
    o.extend((b.len() as u32).to_le_bytes());
    o.extend(b"BIN\0");
    o.extend(b);
    o
}

fuzz_target!(|data: &[u8]| {
    let (json, bin) = match data.iter().position(|&b| b == 0) {
        Some(i) => (&data[..i], &data[i + 1..]),
        None => (data, &[][..]),
    };
    let b = glb(json, bin);
    let _ = kumite_assets::Stage::from_glb(&b);
    let _ = kumite_assets::Character::from_glb(&b);
});
