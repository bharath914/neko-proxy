//! Golden frames for the two example sessions in `docs/sdk/protocol.md`.
//! The Kotlin and Swift test suites decode the same file, so all three
//! implementations agree on the bytes. Regenerate with `UPDATE_FIXTURES=1`.

use sdk::wire::{self, body_chunk::Part, envelope::Msg, Envelope};
use serde_json::json;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn frame(dir: &str, name: &str, msg: Msg) -> serde_json::Value {
    let bytes = sdk::frame::encode(&Envelope { msg: Some(msg) });
    // Every fixture must decode back to itself.
    let back = sdk::frame::decode(&bytes[4..]).unwrap();
    assert_eq!(sdk::frame::encode(&back), bytes);
    json!({"dir": dir, "name": name, "hex": hex(&bytes)})
}

fn hello(token: &[u8]) -> wire::Hello {
    wire::Hello {
        protocol_version: 1,
        sdk_name: "android".into(),
        sdk_version: "0.1.0".into(),
        app_id: "com.demo.shop".into(),
        app_name: "Shop".into(),
        app_version: "1.2.3".into(),
        build: "42".into(),
        device_install_id: "c0ffee00-1111-4222-8333-444455556666".into(),
        device_name: "Pixel 8".into(),
        device_model: "Google Pixel 8".into(),
        os_version: "Android 15".into(),
        device_time_ms: 1_800_000_000_000,
        token: token.to_vec(),
        connection: "usb".into(),
    }
}

fn session_calls() -> Vec<serde_json::Value> {
    vec![
        frame("c2s", "call_start", Msg::CallStart(wire::CallStart {
            call_id: "0191a2b3-0000-7000-8000-000000000001".into(),
            started_at_ms: 1_800_000_001_000,
            method: "POST".into(),
            url: "https://api.demo-shop.test/v1/cart/7/coupon".into(),
            headers: vec![wire::Header { name: "Accept".into(), value: "application/json".into() }],
            http_version: "h2".into(),
            client_stack: "okhttp".into(),
            route_template: "/v1/cart/{id}/coupon".into(),
            name: "Apply coupon".into(),
            owner: "payments".into(),
            screen: "CartActivity".into(),
            request_body_size: 19,
            ..Default::default()
        })),
        frame("c2s", "body_chunk", Msg::BodyChunk(wire::BodyChunk {
            call_id: "0191a2b3-0000-7000-8000-000000000001".into(),
            part: Part::Request as i32,
            seq: 0,
            data: br#"{"code":"SPRING25"}"#.to_vec(),
            last: true,
            ..Default::default()
        })),
        frame("c2s", "response_start", Msg::ResponseStart(wire::ResponseStart {
            call_id: "0191a2b3-0000-7000-8000-000000000001".into(),
            status: 200,
            headers: vec![wire::Header { name: "Content-Type".into(), value: "application/json".into() }],
            http_version: "h2".into(),
            response_body_size: 17,
        })),
        frame("c2s", "call_complete", Msg::CallComplete(wire::CallComplete {
            call_id: "0191a2b3-0000-7000-8000-000000000001".into(),
            ended_at_ms: 1_800_000_001_084,
            timings: Some(wire::Timings { dns: 0, connect: 0, tls: -1, request_sent: 1, ttfb: 60, download: 3 }),
            response_body_available: true,
            request_body_available: true,
            ..Default::default()
        })),
    ]
}

fn build() -> serde_json::Value {
    let fp: Vec<u8> = (0u8..32).collect();
    let client_nonce: Vec<u8> = (32u8..64).collect();
    let server_nonce: Vec<u8> = (64u8..96).collect();
    let token: Vec<u8> = (100u8..132).collect();
    let code = sdk::pairing::pairing_code(fp.as_slice().try_into().unwrap(), &client_nonce, &server_nonce);

    let fresh = vec![
        frame("c2s", "hello", Msg::Hello(hello(&[]))),
        frame("s2c", "hello_ack", Msg::HelloAck(wire::HelloAck {
            state: wire::hello_ack::State::PairingRequired as i32,
            desktop_name: "fedora".into(),
            session_id: "0000000000000001".into(),
            server_time_ms: 1_800_000_000_020,
            ..Default::default()
        })),
        frame("c2s", "pair_request", Msg::PairRequest(wire::PairRequest {
            commit: sdk::pairing::commit_of(&client_nonce).to_vec(),
        })),
        frame("s2c", "pair_challenge", Msg::PairChallenge(wire::PairChallenge {
            server_nonce: server_nonce.clone(),
            timeout_s: 60,
        })),
        frame("c2s", "pair_reveal", Msg::PairReveal(wire::PairReveal { client_nonce: client_nonce.clone() })),
        frame("s2c", "pair_result", Msg::PairResult(wire::PairResult { allowed: true, token: token.clone(), message: String::new() })),
        frame("s2c", "config", Msg::Config(sdk::DeviceConfig::default().to_wire())),
    ];
    let reconnect = {
        let mut v = vec![
            frame("c2s", "hello", Msg::Hello(hello(&token))),
            frame("s2c", "hello_ack", Msg::HelloAck(wire::HelloAck {
                state: wire::hello_ack::State::Authenticated as i32,
                desktop_name: "fedora".into(),
                session_id: "0000000000000002".into(),
                server_time_ms: 1_800_000_000_020,
                ..Default::default()
            })),
            frame("s2c", "config", Msg::Config(sdk::DeviceConfig::default().to_wire())),
        ];
        v.extend(session_calls());
        v
    };
    json!({
        "pairing": {
            "fingerprint_hex": hex(&fp),
            "client_nonce_hex": hex(&client_nonce),
            "server_nonce_hex": hex(&server_nonce),
            "token_hex": hex(&token),
            "code": code,
        },
        "fresh_pairing": fresh,
        "reconnect": reconnect,
    })
}

#[test]
fn golden_frames_match() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../proto/sdk-fixtures.json");
    let now = serde_json::to_string_pretty(&build()).unwrap() + "\n";
    if std::env::var("UPDATE_FIXTURES").is_ok() || !path.exists() {
        std::fs::write(&path, &now).unwrap();
    }
    let on_disk = std::fs::read_to_string(&path).unwrap();
    assert_eq!(on_disk, now, "protocol bytes changed; rerun with UPDATE_FIXTURES=1 if intended");
}

#[test]
fn the_example_sessions_decode_in_order() {
    let v = build();
    for session in ["fresh_pairing", "reconnect"] {
        for f in v[session].as_array().unwrap() {
            let hex = f["hex"].as_str().unwrap();
            let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
            let len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
            assert_eq!(len, bytes.len() - 4);
            assert!(sdk::frame::decode(&bytes[4..]).is_ok(), "{session}/{}", f["name"]);
        }
    }
    assert_eq!(v["pairing"]["code"], "730832");
}
