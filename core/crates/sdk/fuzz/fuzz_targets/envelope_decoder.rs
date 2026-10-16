#![no_main]
//! Decoded envelopes go straight into the assembler, as a hostile but
//! authenticated device could send them.
use flow::{IdGen, NullSink};
use libfuzzer_sys::fuzz_target;
use sdk::assembler::{CallTable, Env, Identity, Stats};
use sdk::wire::envelope::Msg;
use std::sync::Arc;

fuzz_target!(|data: &[u8]| {
    let Ok(env) = sdk::frame::decode(data) else { return };
    let dir = std::env::temp_dir().join(format!("sdk-fuzz-{}", std::process::id()));
    thread_local! {
        static STATE: std::cell::RefCell<Option<(Env, CallTable)>> = const { std::cell::RefCell::new(None) };
    }
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let (e, t) = s.get_or_insert_with(|| {
            let mut limits = sdk::Limits::default();
            limits.max_body_per_call = 64 * 1024;
            (
                Env {
                    ids: Arc::new(IdGen::starting_after(0)),
                    sink: Arc::new(NullSink),
                    store: store::Store::open(&dir).unwrap(),
                    limits,
                    stats: Arc::new(Stats::default()),
                    observer: None,
                },
                CallTable::default(),
            )
        });
        let who = Identity::default();
        match env.msg.unwrap() {
            Msg::CallStart(m) => e.call_start(t, &who, 0, m),
            Msg::BodyChunk(m) => e.body_chunk(t, m),
            Msg::ResponseStart(m) => e.response_start(t, m),
            Msg::CallComplete(m) => e.call_complete(t, 0, m),
            Msg::CallFailed(m) => e.call_failed(t, 0, m),
            Msg::BodyFetchResult(m) => e.body_fetch_result(t, m),
            _ => {}
        }
        if t.in_flight() > 200 {
            e.fail_open_calls(t, "fuzz reset");
        }
    });
});
