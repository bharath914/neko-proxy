//! Body decoding and classification on hostile input.
//!
//!     cargo +nightly fuzz run decode_body
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for enc in ["gzip", "deflate", "br", "zstd", "gzip, br", "bogus"] {
        let d = neko_decode::decode_content(enc, data, 1 << 20);
        let kind = neko_decode::classify("application/json", &d.bytes);
        let _ = neko_decode::build_view(
            "application/json",
            enc,
            data,
            data.len() as u64,
            false,
            &neko_decode::ViewOptions::default(),
        );
        let _ = kind;
    }
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = neko_decode::json_outline(s, 1000);
        let _ = neko_decode::parse_form(s);
    }
});
