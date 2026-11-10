#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > nacre_protocol::MAX_FRAME + 1 { return; }
    let _ = nacre_protocol::framing::decode(bytes);
    let mut decoder=nacre_protocol::framing::Decoder::default();
    for chunk in bytes.chunks(127) {
        if decoder.push(chunk).is_err() { break; }
        assert!(decoder.pending_bytes()<=nacre_protocol::MAX_FRAME);
    }
});
