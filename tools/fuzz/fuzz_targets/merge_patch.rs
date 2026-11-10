#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    if bytes.len() > nacre_protocol::MAX_FRAME { return; }
    if let Ok([before,after])=serde_json::from_slice::<[serde_json::Value;2]>(bytes) {
        let change=nacre_protocol::patch::diff(&before,&after);
        let mut observed=before;
        nacre_protocol::patch::apply_diff(&mut observed,&change);
        assert_eq!(observed,after);
    }
});
