mod common;
use common::*;
use sdk::wire::{self, body_chunk::Part, envelope::Msg};
use sdk::{ConnStatus, DeviceConfig, FetchOutcome};
use std::time::Duration;

#[tokio::test]
async fn listener_follows_the_toggle() {
    let h = Harness::start().await;
    let fp1 = {
        let c = h.client().await;
        c.fingerprint
    };
    assert_eq!(hex(&fp1), h.sdk.fingerprint_hex());
    h.sdk.set_enabled(false).await.unwrap();
    assert!(!h.sdk.is_enabled());
    assert!(
        Client::connect(h.port).await.is_err(),
        "nothing listens while disabled"
    );
    h.sdk.set_enabled(true).await.unwrap();
    let port = h.sdk.local_port().unwrap();
    let c = Client::connect(port).await.unwrap();
    assert_eq!(c.fingerprint, fp1, "the fingerprint survives a toggle");
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[tokio::test]
async fn pairing_then_token_reconnect_streams_a_flow() {
    let h = Harness::start().await;
    let mut c = h.client().await;
    let ack = c.hello("install-1", "com.shop", &[]).await;
    assert_eq!(ack.state, wire::hello_ack::State::PairingRequired as i32);
    assert_eq!(ack.desktop_name, "test-desktop");
    let (code, res) = c.pair(&h.sdk, true).await;
    assert_eq!(code.len(), 6);
    assert!(res.allowed);
    assert_eq!(res.token.len(), 32);
    // Config follows a successful pairing.
    match c.next().await {
        Some(Msg::Config(cfg)) => {
            assert!(cfg.capture_enabled);
            assert_eq!(cfg.body_cap_bytes, 1 << 20);
        }
        other => panic!("{other:?}"),
    }
    c.send(Msg::CallStart(call_start(
        "c1",
        "https://api.shop.test/v1/me",
    )))
    .await;
    c.send(Msg::CallComplete(complete("c1"))).await;
    h.wait_for("the flow", || !h.sink.finished().is_empty())
        .await;
    let f = &h.sink.finished()[0];
    assert_eq!(f.host, "api.shop.test");
    let sdk = f.sdk.as_ref().unwrap();
    assert_eq!(
        (sdk.device_id.as_str(), sdk.app_id.as_str()),
        ("install-1", "com.shop")
    );
    assert_eq!(sdk.app_version, "1.2.3");
    assert_eq!(sdk.connection, "usb");
    drop(c);

    // Reconnect with the token: authenticated, no prompt.
    let mut c2 = h.client().await;
    let ack = c2.hello("install-1", "com.shop", &res.token).await;
    assert_eq!(ack.state, wire::hello_ack::State::Authenticated as i32);
    assert!(h.sdk.pending().is_empty());
    assert!(matches!(c2.next().await, Some(Msg::Config(_))));

    // A wrong token, or the right token for another app, must pair again.
    let mut c3 = h.client().await;
    let ack = c3.hello("install-1", "com.shop", &[9u8; 32]).await;
    assert_eq!(ack.state, wire::hello_ack::State::PairingRequired as i32);
    let mut c4 = h.client().await;
    let ack = c4.hello("install-1", "com.other", &res.token).await;
    assert_eq!(ack.state, wire::hello_ack::State::PairingRequired as i32);
}

#[tokio::test]
async fn the_snapshot_lists_pending_prompts_and_devices() {
    let h = Harness::start().await;
    let mut rx = h.sdk.subscribe();
    let mut c = h.client().await;
    c.hello("install-9", "com.shop", &[]).await;
    c.send(Msg::PairRequest(wire::PairRequest {
        commit: sdk::pairing::commit_of(&[1u8; 32]).to_vec(),
    }))
    .await;
    let ch = match c.next().await {
        Some(Msg::PairChallenge(c)) => c,
        o => panic!("{o:?}"),
    };
    c.send(Msg::PairReveal(wire::PairReveal {
        client_nonce: vec![1u8; 32],
    }))
    .await;
    tokio::time::timeout(Duration::from_millis(500), async {
        loop {
            rx.changed().await.unwrap();
            if !rx.borrow().pending.is_empty() {
                break;
            }
        }
    })
    .await
    .expect("the prompt shows within 500 ms");
    let snap = h.sdk.snapshot();
    let p = &snap.pending[0];
    assert_eq!(p.device_name, "Pixel 8");
    assert_eq!(p.app_id, "com.shop");
    assert_eq!(
        p.code,
        sdk::pairing::pairing_code(&c.fingerprint, &[1u8; 32], &ch.server_nonce)
    );
    assert!(h.sdk.allow(p.id));
    assert!(matches!(c.next().await, Some(Msg::PairResult(r)) if r.allowed));
    h.wait_for("connected", || {
        h.sdk
            .snapshot()
            .devices
            .iter()
            .any(|d| d.status == ConnStatus::Connected)
    })
    .await;
    let d = &h.sdk.snapshot().devices[0];
    assert_eq!(d.facts.device_name, "Pixel 8");
    assert_eq!(d.connection, "usb");
}

#[tokio::test]
async fn a_mismatched_certificate_shows_a_different_code() {
    // What a man in the middle would cause: the device computes the code from
    // the fingerprint it saw. Simulate a different one.
    let h = Harness::start().await;
    let mut c = h.client().await;
    c.hello("i", "a.b", &[]).await;
    let nonce = [3u8; 32];
    c.send(Msg::PairRequest(wire::PairRequest {
        commit: sdk::pairing::commit_of(&nonce).to_vec(),
    }))
    .await;
    let ch = match c.next().await {
        Some(Msg::PairChallenge(c)) => c,
        o => panic!("{o:?}"),
    };
    c.send(Msg::PairReveal(wire::PairReveal {
        client_nonce: nonce.to_vec(),
    }))
    .await;
    h.wait_for("prompt", || !h.sdk.pending().is_empty()).await;
    let desktop = h.sdk.pending()[0].code.clone();
    let device_if_mitm = sdk::pairing::pairing_code(&[0xEE; 32], &nonce, &ch.server_nonce);
    assert_ne!(desktop, device_if_mitm);
}

#[tokio::test]
async fn deny_is_remembered_and_the_device_is_told_to_back_off() {
    let h = Harness::start().await;
    let mut c = h.client().await;
    c.hello("i1", "com.shop", &[]).await;
    let (_, res) = c.pair(&h.sdk, false).await;
    assert!(!res.allowed);
    c.expect_closed().await;
    let mut c2 = h.client().await;
    let ack = c2.hello("i1", "com.shop", &[]).await;
    assert_eq!(ack.state, wire::hello_ack::State::Denied as i32);
    assert!(ack.retry_after_s >= 60);
    h.wait_for("denied row", || {
        h.sdk
            .snapshot()
            .devices
            .iter()
            .any(|d| d.status == ConnStatus::Denied)
    })
    .await;
    let _ = &h.sdk.snapshot();
    // Forgetting clears the denial.
    assert!(h.sdk.forget("i1"));
    let mut c3 = h.client().await;
    let ack = c3.hello("i1", "com.shop", &[]).await;
    assert_eq!(ack.state, wire::hello_ack::State::PairingRequired as i32);
}

#[tokio::test]
async fn revoke_drops_the_session_and_forces_a_new_pairing() {
    let h = Harness::start().await;
    let token = h.paired("i2", "com.shop").await;
    let mut c = h.client().await;
    assert_eq!(
        c.hello("i2", "com.shop", &token).await.state,
        wire::hello_ack::State::Authenticated as i32
    );
    h.wait_for("connected", || h.sdk.status().sessions == 1)
        .await;
    assert!(h.sdk.revoke("i2", "com.shop"));
    c.expect_closed().await;
    h.wait_for("revoked", || {
        h.sdk
            .snapshot()
            .devices
            .iter()
            .any(|d| d.status == ConnStatus::Revoked)
    })
    .await;
    let mut again = h.client().await;
    assert_eq!(
        again.hello("i2", "com.shop", &token).await.state,
        wire::hello_ack::State::PairingRequired as i32
    );
}

#[tokio::test]
async fn no_call_data_is_accepted_before_authentication() {
    let h = Harness::start().await;
    let mut c = h.client().await;
    let ack = c.hello("i3", "com.shop", &[]).await;
    assert_eq!(ack.state, wire::hello_ack::State::PairingRequired as i32);
    c.send(Msg::CallStart(call_start("x", "https://a.test/")))
        .await;
    c.send(Msg::CallComplete(complete("x"))).await;
    c.expect_closed().await;
    assert_eq!(h.sink.count(), 0);
    assert!(h.sdk.stats().protocol_violations >= 1);

    // Call data as the very first message is a violation too.
    let mut c = h.client().await;
    c.send(Msg::CallStart(call_start("y", "https://a.test/")))
        .await;
    c.expect_closed().await;
    assert_eq!(h.sink.count(), 0);
}

#[tokio::test]
async fn unsupported_protocol_versions_get_a_clear_error() {
    let h = Harness::start().await;
    let mut c = h.client().await;
    let mut hello = Client::hello_msg("i4", "com.shop", &[]);
    hello.protocol_version = 99;
    c.send(Msg::Hello(hello)).await;
    match c.recv().await {
        Some(Msg::Error(e)) => {
            assert_eq!(e.code, wire::error::Code::UnsupportedVersion as i32);
            assert!(e.message.contains("99"));
            assert_eq!(e.max_protocol_version, wire::PROTOCOL_VERSION);
        }
        other => panic!("{other:?}"),
    }
    c.expect_closed().await;
    let mut c = h.client().await;
    let mut hello = Client::hello_msg("i4", "com.shop", &[]);
    hello.protocol_version = 0;
    c.send(Msg::Hello(hello)).await;
    assert!(matches!(c.recv().await, Some(Msg::Error(_))));
}

#[tokio::test]
async fn hostile_input_closes_the_connection_and_leaves_the_core_standing() {
    let h = Harness::start().await;

    // Oversized frame.
    let mut c = h.client().await;
    c.send_raw(&(5_000_000u32).to_be_bytes()).await;
    match c.recv().await {
        Some(Msg::Error(e)) => assert_eq!(e.code, wire::error::Code::FrameTooLarge as i32),
        other => panic!("{other:?}"),
    }
    c.expect_closed().await;

    // Garbage that is not an envelope.
    let mut c = h.client().await;
    c.send_raw(&[0, 0, 0, 4, 0xff, 0xff, 0xff, 0xff]).await;
    c.expect_closed().await;

    // Malformed ids in Hello.
    let mut c = h.client().await;
    c.send(Msg::Hello(Client::hello_msg(
        "bad id with spaces",
        "com.shop",
        &[],
    )))
    .await;
    c.expect_closed().await;
    let mut c = h.client().await;
    c.send(Msg::Hello(Client::hello_msg("", "com.shop", &[])))
        .await;
    c.expect_closed().await;

    // A duplicate Hello after authentication.
    let token = h.paired("dup", "com.shop").await;
    let mut c = h.client().await;
    c.hello("dup", "com.shop", &token).await;
    c.send(Msg::Hello(Client::hello_msg("dup", "com.shop", &token)))
        .await;
    c.expect_closed().await;

    // Server-to-client messages sent by a client.
    let mut c = h.client().await;
    c.hello("dup", "com.shop", &token).await;
    c.send(Msg::Config(wire::Config::default())).await;
    c.expect_closed().await;

    // Slowloris: connect, say nothing.
    let mut c = h.client().await;
    let t = std::time::Instant::now();
    c.expect_closed().await;
    assert!(t.elapsed() < Duration::from_secs(3));

    // Slow trickle of a frame.
    let mut c = h.client().await;
    c.send_raw(&[0, 0]).await;
    c.expect_closed().await;

    // Still healthy afterwards.
    let mut c = h.client().await;
    assert_eq!(
        c.hello("dup", "com.shop", &token).await.state,
        wire::hello_ack::State::Authenticated as i32
    );
}

#[tokio::test]
async fn pre_authentication_connections_are_capped_per_address() {
    let h = Harness::with(|o| o.limits.handshake_timeout = Duration::from_secs(3)).await;
    let cap = h.sdk.limits().max_handshaking_per_ip;
    let mut held = Vec::new();
    for _ in 0..cap {
        held.push(h.client().await);
    }
    // The next connection from the same address is turned away before TLS completes.
    let extra = Client::connect(h.port).await;
    assert!(
        extra.is_err() || {
            let mut c = extra.unwrap();
            c.send(Msg::Hello(Client::hello_msg("x", "y", &[]))).await;
            c.recv().await.is_none()
        }
    );
    assert!(h.sdk.stats().rejected_connections >= 1);
    drop(held);
}

#[tokio::test]
async fn too_many_sessions_are_refused() {
    let h = Harness::with(|o| o.limits.max_sessions = 2).await;
    let mut keep = Vec::new();
    for i in 0..2 {
        let name = format!("s{i}");
        let t = h.paired(&name, "com.shop").await;
        let mut c = h.client().await;
        assert_eq!(
            c.hello(&name, "com.shop", &t).await.state,
            wire::hello_ack::State::Authenticated as i32
        );
        keep.push(c);
    }
    h.wait_for("two sessions", || h.sdk.status().sessions == 2)
        .await;
    let t = h.paired("s2", "com.shop").await;
    let mut c = h.client().await;
    c.hello("s2", "com.shop", &t).await;
    match c.next().await {
        Some(Msg::Error(e)) => assert_eq!(e.code, wire::error::Code::TooManySessions as i32),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_new_session_for_the_same_app_replaces_the_old_one() {
    let h = Harness::start().await;
    let t = h.paired("r1", "com.shop").await;
    let mut a = h.client().await;
    a.hello("r1", "com.shop", &t).await;
    h.wait_for("a", || h.sdk.status().sessions == 1).await;
    let mut b = h.client().await;
    b.hello("r1", "com.shop", &t).await;
    a.expect_closed().await;
    assert!(matches!(b.next().await, Some(Msg::Config(_))));
}

#[tokio::test]
async fn chunked_bodies_and_response_assembly_over_the_wire() {
    let h = Harness::start().await;
    let t = h.paired("b1", "com.shop").await;
    let mut c = h.client().await;
    c.hello("b1", "com.shop", &t).await;
    let mut start = call_start("big", "https://api.shop.test/upload");
    start.method = "POST".into();
    c.send(Msg::CallStart(start)).await;
    for (i, part) in ["{\"items\":", "[1,2,", "3]}"].iter().enumerate() {
        c.send(Msg::BodyChunk(wire::BodyChunk {
            call_id: "big".into(),
            part: Part::Request as i32,
            seq: i as u32,
            data: part.as_bytes().to_vec(),
            last: i == 2,
            ..Default::default()
        }))
        .await;
    }
    c.send(Msg::ResponseStart(wire::ResponseStart {
        call_id: "big".into(),
        status: 201,
        headers: vec![wire::Header {
            name: "Content-Type".into(),
            value: "application/json".into(),
        }],
        http_version: "h2".into(),
        ..Default::default()
    }))
    .await;
    c.send(Msg::BodyChunk(wire::BodyChunk {
        call_id: "big".into(),
        part: Part::Response as i32,
        seq: 0,
        data: b"{}".to_vec(),
        last: true,
        ..Default::default()
    }))
    .await;
    c.send(Msg::CallComplete(complete("big"))).await;
    h.wait_for("flow", || !h.sink.finished().is_empty()).await;
    let f = &h.sink.finished()[0];
    assert_eq!(&f.req_body[..], b"{\"items\":[1,2,3]}");
    assert_eq!(&f.res_body[..], b"{}");
    assert_eq!(f.status, 201);
}

#[tokio::test]
async fn lazy_bodies_are_fetched_on_demand_and_gone_ones_say_so() {
    let h = Harness::start().await;
    let t = h.paired("l1", "com.shop").await;
    let mut c = h.client().await;
    c.hello("l1", "com.shop", &t).await;
    for id in ["lazy", "evicted"] {
        c.send(Msg::CallStart(call_start(id, "https://api.shop.test/big")))
            .await;
        let mut done = complete(id);
        done.response_body_available = false;
        done.response_body_size = 3_000_000;
        c.send(Msg::CallComplete(done)).await;
    }
    h.wait_for("two flows", || h.sink.finished().len() == 2)
        .await;
    for r in h.sink.finished() {
        h.store.enqueue(r);
    }
    h.store.flush();
    let ids: Vec<_> = h
        .sink
        .finished()
        .iter()
        .map(|r| (r.sdk.as_ref().unwrap().call_id.clone(), r.id))
        .collect();
    let id_of = |call: &str| ids.iter().find(|(c, _)| c == call).unwrap().1;

    // The device answers BodyFetch in the background.
    let sdk = h.sdk.clone();
    let lazy_id = id_of("lazy");
    let fetch =
        tokio::spawn(async move { sdk.fetch_body(lazy_id, store::BodyPart::Response).await });
    match c.next_non_config().await {
        Some(Msg::BodyFetch(f)) => {
            assert_eq!(f.call_id, "lazy");
            c.send(Msg::BodyFetchResult(wire::BodyFetchResult {
                call_id: "lazy".into(),
                part: f.part,
                available: true,
            }))
            .await;
            for (i, d) in [&b"hello "[..], &b"world"[..]].iter().enumerate() {
                c.send(Msg::BodyChunk(wire::BodyChunk {
                    call_id: "lazy".into(),
                    part: f.part,
                    seq: i as u32,
                    data: d.to_vec(),
                    last: i == 1,
                    ..Default::default()
                }))
                .await;
            }
        }
        other => panic!("{other:?}"),
    }
    match fetch.await.unwrap().unwrap() {
        FetchOutcome::Body { data, .. } => assert_eq!(&data[..], b"hello world"),
        other => panic!("{other:?}"),
    }
    h.store.flush();
    assert_eq!(
        &h.store.get(lazy_id).unwrap().unwrap().res_body[..],
        b"hello world"
    );

    let sdk = h.sdk.clone();
    let ev_id = id_of("evicted");
    let fetch = tokio::spawn(async move { sdk.fetch_body(ev_id, store::BodyPart::Response).await });
    match c.next_non_config().await {
        Some(Msg::BodyFetch(f)) => {
            c.send(Msg::BodyFetchResult(wire::BodyFetchResult {
                call_id: f.call_id,
                part: f.part,
                available: false,
            }))
            .await;
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fetch.await.unwrap().unwrap(), FetchOutcome::Gone);

    // Offline device.
    drop(c);
    h.wait_for("gone", || h.sdk.status().sessions == 0).await;
    let r = h
        .sdk
        .fetch_body(id_of("lazy"), store::BodyPart::Request)
        .await;
    assert!(r.is_ok() || r == Err(sdk::FetchError::Offline));
}

#[tokio::test]
async fn a_dropped_session_marks_open_calls_incomplete_after_the_grace_period() {
    let h = Harness::start().await;
    let t = h.paired("g1", "com.shop").await;
    let mut c = h.client().await;
    c.hello("g1", "com.shop", &t).await;
    c.send(Msg::CallStart(call_start(
        "open",
        "https://api.shop.test/slow",
    )))
    .await;
    h.wait_for("started", || h.sink.count() >= 1).await;
    drop(c);
    h.wait_for("incomplete", || {
        h.sink
            .finished()
            .iter()
            .any(|f| f.flags & flow::flags::INCOMPLETE != 0)
    })
    .await;

    // A reconnect inside the grace period keeps the call alive.
    let mut c = h.client().await;
    c.hello("g1", "com.shop", &t).await;
    c.send(Msg::CallStart(call_start(
        "kept",
        "https://api.shop.test/slow",
    )))
    .await;
    h.wait_for("started", || h.sink.count() >= 3).await;
    drop(c);
    let mut c = h.client().await;
    c.hello("g1", "com.shop", &t).await;
    c.send(Msg::CallComplete(complete("kept"))).await;
    h.wait_for("completed", || {
        h.sink.finished().iter().any(|f| {
            f.sdk.as_ref().unwrap().call_id == "kept" && f.flags & flow::flags::INCOMPLETE == 0
        })
    })
    .await;
}

#[tokio::test]
async fn config_changes_reach_the_live_session() {
    let h = Harness::start().await;
    let t = h.paired("cfg", "com.shop").await;
    let mut c = h.client().await;
    c.hello("cfg", "com.shop", &t).await;
    assert!(matches!(c.next().await, Some(Msg::Config(_))));
    let cfg = DeviceConfig {
        capture_enabled: false,
        host_deny: vec!["ads.test".into()],
        redaction: sdk::Redaction::Strict,
        ..Default::default()
    };
    assert!(h.sdk.set_config("cfg", "com.shop", cfg));
    match c.next().await {
        Some(Msg::Config(w)) => {
            assert!(!w.capture_enabled);
            assert_eq!(w.host_deny, vec!["ads.test"]);
            assert_eq!(w.redaction, wire::config::Redaction::Strict as i32);
        }
        other => panic!("{other:?}"),
    }
    // And is applied to the next connection.
    drop(c);
    let mut c = h.client().await;
    c.hello("cfg", "com.shop", &t).await;
    assert!(matches!(c.next().await, Some(Msg::Config(w)) if !w.capture_enabled));
}

#[tokio::test]
async fn device_clock_offsets_are_corrected() {
    let h = Harness::start().await;
    let t = h.paired("clk", "com.shop").await;
    let mut c = h.client().await;
    // This device's clock runs two minutes behind.
    let skew = 120_000;
    let mut hello = Client::hello_msg("clk", "com.shop", &t);
    hello.device_time_ms -= skew;
    c.send(Msg::Hello(hello)).await;
    let _ack = c.next().await;
    let mut s = call_start("t", "https://api.shop.test/");
    s.started_at_ms -= skew;
    c.send(Msg::CallStart(s)).await;
    let mut done = complete("t");
    done.ended_at_ms -= skew - 50;
    c.send(Msg::CallComplete(done)).await;
    h.wait_for("flow", || !h.sink.finished().is_empty()).await;
    let f = &h.sink.finished()[0];
    assert!(
        (f.started_at_ms - now_ms()).abs() < 2_000,
        "start corrected to core time"
    );
}

#[tokio::test]
async fn ingest_disabled_midway_closes_sessions() {
    let h = Harness::start().await;
    let t = h.paired("d1", "com.shop").await;
    let mut c = h.client().await;
    c.hello("d1", "com.shop", &t).await;
    h.wait_for("session", || h.sdk.status().sessions == 1).await;
    h.sdk.set_enabled(false).await.unwrap();
    c.expect_closed().await;
    h.wait_for("no sessions", || h.sdk.status().sessions == 0)
        .await;
}

#[tokio::test]
async fn floods_are_dropped_and_counted_without_stalling_other_sessions() {
    let h = Harness::with(|o| o.limits.max_events_per_sec = 200).await;
    let t1 = h.paired("flood", "com.shop").await;
    let t2 = h.paired("calm", "com.shop").await;
    let mut f = h.client().await;
    f.hello("flood", "com.shop", &t1).await;
    let mut calm = h.client().await;
    calm.hello("calm", "com.shop", &t2).await;
    for i in 0..2_000 {
        f.send(Msg::CallStart(call_start(
            &format!("f{i}"),
            "https://flood.test/",
        )))
        .await;
    }
    // The calm session still works while the flood is being handled.
    calm.send(Msg::CallStart(call_start("ok", "https://calm.test/")))
        .await;
    calm.send(Msg::CallComplete(complete("ok"))).await;
    h.wait_for("calm flow", || {
        h.sink.finished().iter().any(|r| r.host == "calm.test")
    })
    .await;
    h.wait_for("drops counted", || h.sdk.stats().dropped_events > 0)
        .await;
}

#[tokio::test]
async fn the_service_is_advertised_while_enabled_and_withdrawn_after() {
    use mdns_sd::{ServiceDaemon, ServiceEvent};
    // Unique, so a stale answer from an earlier run cannot match.
    let name = format!("mdns-test-{}-{}", std::process::id(), now_ms());
    let n2 = name.clone();
    let h = Harness::with(move |o| {
        o.advertise = true;
        o.desktop_name = n2;
    })
    .await;
    if let sdk::discovery::DiscoveryStatus::Error(e) = h.sdk.status().discovery {
        eprintln!("mDNS unavailable here ({e}); skipping the browse check");
        return;
    }
    assert_eq!(
        h.sdk.status().discovery,
        sdk::discovery::DiscoveryStatus::Advertising
    );
    let daemon = ServiceDaemon::new().unwrap();
    let rx = daemon.browse(sdk::discovery::SERVICE_TYPE).unwrap();
    let want = h.sdk.fingerprint_hex()[..16].to_string();
    let name = name.clone();
    let found = tokio::task::spawn_blocking(move || {
        let end = std::time::Instant::now() + Duration::from_secs(6);
        while std::time::Instant::now() < end {
            if let Ok(ServiceEvent::ServiceResolved(info)) =
                rx.recv_timeout(Duration::from_millis(300))
            {
                if info.get_property_val_str("name") == Some(name.as_str()) {
                    assert_eq!(info.get_property_val_str("v"), Some("1"));
                    assert_eq!(info.get_property_val_str("fp"), Some(want.as_str()));
                    return Some(info.get_port());
                }
            }
        }
        None
    })
    .await
    .unwrap();
    let _ = daemon.shutdown();
    match found {
        Some(port) => assert_eq!(port, h.port),
        None => eprintln!(
            "no mDNS response in this environment (multicast blocked?); advertised status was fine"
        ),
    }
    h.sdk.set_enabled(false).await.unwrap();
    assert_eq!(
        h.sdk.status().discovery,
        sdk::discovery::DiscoveryStatus::Off
    );
}

#[tokio::test]
async fn connections_from_this_computer_are_accepted_without_a_prompt_when_the_setting_is_on() {
    let h = Harness::with_settings(|_| {}, sdk::SdkSettings::default()).await;
    let mut c = h.client().await;
    assert_eq!(c.hello("local-1", "com.shop", &[]).await.state, wire::hello_ack::State::PairingRequired as i32);
    // No person involved: the exchange still runs, and ends with a token.
    let nonce = [4u8; 32];
    c.send(Msg::PairRequest(wire::PairRequest { commit: sdk::pairing::commit_of(&nonce).to_vec() })).await;
    assert!(matches!(c.next().await, Some(Msg::PairChallenge(_))));
    c.send(Msg::PairReveal(wire::PairReveal { client_nonce: nonce.to_vec() })).await;
    match c.next().await {
        Some(Msg::PairResult(r)) => {
            assert!(r.allowed);
            assert_eq!(r.token.len(), 32);
        }
        other => panic!("{other:?}"),
    }
    assert!(h.sdk.pending().is_empty(), "no prompt was ever shown");
    assert!(matches!(c.next().await, Some(Msg::Config(_))));
    // The device is a normal paired device afterwards: connected, and it can be revoked.
    h.wait_for("connected", || h.sdk.snapshot().devices.iter().any(|d| d.status == ConnStatus::Connected)).await;
}

#[tokio::test]
async fn an_explicit_deny_is_still_honoured_for_local_devices() {
    let h = Harness::with_settings(|_| {}, sdk::SdkSettings::default()).await;
    // Deny through the registry as the desktop would after a prompt.
    h.sdk.update_settings(|s| s.auto_accept_local = false).await.unwrap();
    let mut c = h.client().await;
    c.hello("local-2", "com.shop", &[]).await;
    let (_, r) = c.pair(&h.sdk, false).await;
    assert!(!r.allowed);
    h.sdk.update_settings(|s| s.auto_accept_local = true).await.unwrap();
    let mut again = h.client().await;
    assert_eq!(again.hello("local-2", "com.shop", &[]).await.state, wire::hello_ack::State::Denied as i32);
}

#[tokio::test]
async fn settings_are_remembered_and_the_listener_follows_them() {
    let h = Harness::start().await;
    assert!(h.sdk.settings().enabled && h.sdk.settings().lan);
    // Loopback only: the status shows no LAN address.
    h.sdk.update_settings(|s| s.lan = false).await.unwrap();
    assert!(h.sdk.is_enabled());
    let st = h.sdk.status();
    assert!(!st.listening.is_empty() && st.listening.iter().all(|a| a.starts_with("127.0.0.1")), "{:?}", st.listening);
    // Turning it off is remembered, so a new service on the same directory starts off.
    h.sdk.set_enabled(false).await.unwrap();
    assert!(!h.sdk.is_enabled());
    let dir = h.dir.path().join("sdk");
    let saved = sdk::SdkSettings::load(&dir);
    assert!(!saved.enabled && !saved.lan);
    let sink = std::sync::Arc::new(Collect::default());
    let store = store::Store::open(&h.dir.path().join("store2")).unwrap();
    let mut o = sdk::SdkOptions::new(dir);
    o.port = 0; o.advertise = false; o.watch_adb = false;
    let again = sdk::Sdk::new(o, sink, store, std::sync::Arc::new(flow::IdGen::starting_after(0))).unwrap();
    again.start_saved().await.unwrap();
    assert!(!again.is_enabled(), "saved as off, so it stays off");
    again.update_settings(|s| s.enabled = true).await.unwrap();
    assert!(again.is_enabled());
    again.set_enabled(false).await.unwrap();
}

#[tokio::test]
async fn a_busy_port_is_reported_with_a_fix_instead_of_failing_silently() {
    use sdk::health::{Health, Level};
    let squatter = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = squatter.local_addr().unwrap().port();
    let dir = tempfile::tempdir().unwrap();
    let sink = std::sync::Arc::new(Collect::default());
    let store = store::Store::open(&dir.path().join("store")).unwrap();
    let mut o = sdk::SdkOptions::new(dir.path().join("sdk"));
    o.port = port;
    o.bind = [127, 0, 0, 1].into();
    o.advertise = false;
    o.watch_adb = false;
    let s = sdk::Sdk::new(o, sink, store, std::sync::Arc::new(flow::IdGen::starting_after(0))).unwrap();
    assert!(s.start_saved().await.is_err());
    let r = s.health().await;
    assert_eq!(r.health, Health::Problem);
    let c = r.checks.iter().find(|c| c.id == "listener").unwrap();
    assert_eq!(c.level, Level::Error);
    assert!(c.detail.contains(&port.to_string()), "{}", c.detail);
    assert!(c.fix.contains("--sdk-port"), "{}", c.fix);
    // Freeing the port and trying again clears the problem.
    drop(squatter);
    s.set_enabled(true).await.unwrap();
    assert_ne!(s.health().await.health, Health::Problem);
    s.set_enabled(false).await.unwrap();
}
