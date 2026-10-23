//! A rollback Input whose run-length code kumite_net lets through: ggrs then decodes it with bitfield-rle, which must
//! neither panic nor ask for more than kumite_net's cap. The input: the code; it is wrapped in an Input message here.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|code: &[u8]| {
    let mut b = 0u32.to_le_bytes().to_vec();
    b.extend(0u16.to_le_bytes());
    b.extend(2u32.to_le_bytes());
    b.extend(1u64.to_le_bytes());
    b.push(0);
    b.extend((-1i32).to_le_bytes());
    b.push(0);
    b.extend(0i32.to_le_bytes());
    b.extend((-1i32).to_le_bytes());
    b.extend((code.len() as u64).to_le_bytes());
    b.extend(code);
    if kumite_net::ggrs_message(&b, 0).is_some() {
        let len = bitfield_rle::decode_len(code).expect("a code kumite_net passes is whole");
        assert!(len as u64 <= kumite_net::MAX_INPUT_BYTES);
        assert_eq!(bitfield_rle::decode(code).expect("and decodes").len(), len);
    }
});
