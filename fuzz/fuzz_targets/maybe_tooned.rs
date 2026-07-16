#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let opts = tooned_core::ConversionOptions::default();
    let _ = tooned_core::maybe_tooned(data, &opts);
});
