#![no_main]
//! Arbitrary bytes as a TCP stream: the frame reader must return an error or
//! frames, never panic, hang or allocate past its limit.
use libfuzzer_sys::fuzz_target;
use std::time::Duration;

fuzz_target!(|data: &[u8]| {
    let rt = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
    rt.block_on(async {
        let mut r = data;
        // Read until the stream runs out or the reader reports an error.
        for _ in 0..64 {
            if sdk::frame::read_envelope(&mut r, 64 * 1024, None, Duration::from_millis(50))
                .await
                .is_err()
            {
                break;
            }
        }
    });
});
