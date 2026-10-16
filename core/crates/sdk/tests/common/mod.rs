#![allow(dead_code)]
//! A fake SDK client and a harness around a real ingest service.

use flow::{FlowEvent, FlowRecord, FlowSink, IdGen};
use sdk::{Limits, Sdk, SdkOptions};
use std::sync::{Arc, Mutex};
use std::time::Duration;
pub use sdk::fake_client::*;
pub use sdk::wire;

#[derive(Default)]
pub struct Collect(pub Mutex<Vec<FlowEvent>>);

impl FlowSink for Collect {
    fn publish(&self, e: FlowEvent) {
        self.0.lock().unwrap().push(e);
    }
}

impl Collect {
    pub fn finished(&self) -> Vec<Arc<FlowRecord>> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                FlowEvent::Finished(r) => Some(r.clone()),
                _ => None,
            })
            .collect()
    }
    pub fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

pub struct Harness {
    pub sdk: Sdk,
    pub sink: Arc<Collect>,
    pub store: store::Store,
    pub port: u16,
    pub dir: tempfile::TempDir,
}

pub fn test_limits() -> Limits {
    Limits {
        handshake_timeout: Duration::from_millis(800),
        pair_timeout: Duration::from_secs(5),
        grace: Duration::from_millis(300),
        fetch_timeout: Duration::from_secs(2),
        prompt_cooldown: Duration::from_millis(100),
        ..Limits::default()
    }
}

impl Harness {
    pub async fn start() -> Harness {
        Self::with(|_| {}).await
    }

    pub async fn with(f: impl FnOnce(&mut SdkOptions)) -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let store = store::Store::open(&dir.path().join("store")).unwrap();
        let mut opts = SdkOptions::new(dir.path().join("sdk"));
        opts.port = 0;
        opts.bind = [127, 0, 0, 1].into();
        opts.advertise = false;
        opts.watch_adb = false;
        opts.desktop_name = "test-desktop".into();
        opts.limits = test_limits();
        f(&mut opts);
        let sdk = Sdk::new(opts, sink.clone(), store.clone(), Arc::new(IdGen::starting_after(0))).unwrap();
        sdk.set_enabled(true).await.unwrap();
        let port = sdk.local_port().unwrap();
        Harness { sdk, sink, store, port, dir }
    }

    pub async fn client(&self) -> Client {
        Client::connect(self.port).await.unwrap()
    }

    /// Pair a fresh install and return its token.
    pub async fn paired(&self, install: &str, app: &str) -> Vec<u8> {
        let mut c = self.client().await;
        let ack = c.hello(install, app, &[]).await;
        assert_eq!(ack.state, wire::hello_ack::State::PairingRequired as i32);
        let (_, res) = c.pair(&self.sdk, true).await;
        assert!(res.allowed);
        res.token
    }

    pub async fn wait_for(&self, what: &str, f: impl Fn() -> bool) {
        for _ in 0..200 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {what}");
    }
}

