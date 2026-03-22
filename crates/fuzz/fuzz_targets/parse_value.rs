#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let arena = toml_spanner::Arena::new();
    let mut value = toml_spanner::parse(text, &arena);
    std::hint::black_box(&mut value);
});
