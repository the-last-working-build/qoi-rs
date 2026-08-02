#![no_main]

use libfuzzer_sys::fuzz_target;
use qoi_rs::{Channels, decode};

fuzz_target!(|data: &[u8]| {
    let _ = decode(data, None);
    let _ = decode(data, Some(Channels::Rgb));
    let _ = decode(data, Some(Channels::Rgba));
});
