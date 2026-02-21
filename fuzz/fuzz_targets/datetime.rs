#![no_main]

use std::mem::MaybeUninit;

use libfuzzer_sys::fuzz_target;
use toml_spanner::DateTime;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let Ok(dt) = text.parse::<DateTime>() else {
        return;
    };
    let mut buf = MaybeUninit::uninit();
    let datetime = dt.format(&mut buf);
    let out = datetime.parse::<DateTime>().unwrap();
    assert_eq!(dt.date(), out.date());
    assert_eq!(dt.time(), out.time());
    assert_eq!(dt.offset(), out.offset());
});
