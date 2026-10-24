//! Arbitrary bytes into the proxy's real HTTP connection handler, with
//! upstream connections disabled. Covers request-line and header parsing,
//! target resolution, CONNECT, upgrades and the body tee.
//!
//!     cargo +nightly fuzz run http_parse
#![no_main]

use libfuzzer_sys::fuzz_target;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

fn proxy() -> &'static neko_net::Proxy {
    static PROXY: OnceLock<neko_net::Proxy> = OnceLock::new();
    PROXY.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        let ca = Arc::new(neko_tls::CaStore::load_or_create(&dir, neko_tls::KeyStorage::File).unwrap());
        let mut cfg = neko_net::ProxyConfig::new("127.0.0.1:8899".parse().unwrap());
        cfg.offline = true;
        neko_net::Proxy::new(
            cfg,
            ca,
            Arc::new(flow::NullSink),
            Arc::new(flow::IdGen::starting_after(0)),
            flow::InterceptorChain::empty(),
        )
    })
}

fuzz_target!(|data: &[u8]| {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let (mut client, server) = duplex(64 * 1024);
        let p = proxy().clone();
        let handle = tokio::spawn(async move {
            p.serve_io(server, "127.0.0.1:50000".parse().unwrap()).await;
        });
        let _ = client.write_all(data).await;
        let _ = client.shutdown().await;
        let mut out = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(1), client.read_to_end(&mut out)).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    });
});
