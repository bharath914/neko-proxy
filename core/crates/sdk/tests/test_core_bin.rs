//! The dev binary that every SDK's integration tests rely on.
mod common;
use common::*;
use sdk::wire::{self, envelope::Msg};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start(extra: &[&str], record: &std::path::Path) -> (Child, u16) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_sdk-test-core"))
        .args(["--port", "0", "--record"])
        .arg(record)
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(c.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(&line).expect("a ready line");
    assert_eq!(v["event"], "ready");
    (Child(c), v["port"].as_u64().unwrap() as u16)
}

fn lines(p: &std::path::Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(p)
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

async fn wait_lines(
    p: &std::path::Path,
    f: impl Fn(&[serde_json::Value]) -> bool,
) -> Vec<serde_json::Value> {
    for _ in 0..200 {
        let l = lines(p);
        if f(&l) {
            return l;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out; recorded: {:#?}", lines(p));
}

#[tokio::test]
async fn auto_allow_pairs_records_and_fetches_lazy_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let rec = dir.path().join("out.jsonl");
    let (_child, port) = start(&["--auto-allow", "--fetch-lazy"], &rec);
    let mut c = Client::connect(port).await.unwrap();
    assert_eq!(
        c.hello("i", "com.x", &[]).await.state,
        wire::hello_ack::State::PairingRequired as i32
    );
    // No person: the binary approves by itself.
    let nonce = [5u8; 32];
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
    let code = sdk::pairing::pairing_code(&c.fingerprint, &nonce, &ch.server_nonce);
    assert!(matches!(c.next().await, Some(Msg::PairResult(r)) if r.allowed));

    c.send(Msg::CallStart(call_start("c1", "https://api.test/x")))
        .await;
    let mut done = complete("c1");
    done.response_body_available = false;
    done.response_body_size = 9;
    c.send(Msg::CallComplete(done)).await;
    // --fetch-lazy asks for the body by itself.
    match c.next_non_config().await {
        Some(Msg::BodyFetch(f)) => {
            c.send(Msg::BodyFetchResult(wire::BodyFetchResult {
                call_id: f.call_id.clone(),
                part: f.part,
                available: true,
            }))
            .await;
            c.send(Msg::BodyChunk(wire::BodyChunk {
                call_id: f.call_id,
                part: f.part,
                seq: 0,
                data: b"lazy body".to_vec(),
                last: true,
                ..Default::default()
            }))
            .await;
        }
        o => panic!("{o:?}"),
    }
    let l = wait_lines(&rec, |l| l.iter().any(|v| v["type"] == "body_chunk")).await;
    let prompt = l
        .iter()
        .find(|v| v["type"] == "pair_prompt")
        .expect("the code is recorded");
    assert_eq!(prompt["code"], code);
    assert!(l
        .iter()
        .any(|v| v["type"] == "session_start" && v["device"] == "i"));
    let flow = l.iter().find(|v| v["type"] == "flow").unwrap();
    assert_eq!(flow["url"], "https://api.test/x");
    assert_eq!(flow["sdk"]["response_body_state"], 1);
    let chunk = l.iter().find(|v| v["type"] == "body_chunk").unwrap();
    assert_eq!(chunk["data_text"], "lazy body");
}

#[tokio::test]
async fn deny_and_revoke_after_exercise_those_paths() {
    let dir = tempfile::tempdir().unwrap();
    let (_c, port) = start(&["--deny"], &dir.path().join("a.jsonl"));
    let mut c = Client::connect(port).await.unwrap();
    c.hello("i", "com.x", &[]).await;
    let nonce = [1u8; 32];
    c.send(Msg::PairRequest(wire::PairRequest {
        commit: sdk::pairing::commit_of(&nonce).to_vec(),
    }))
    .await;
    c.next().await;
    c.send(Msg::PairReveal(wire::PairReveal {
        client_nonce: nonce.to_vec(),
    }))
    .await;
    assert!(matches!(c.next().await, Some(Msg::PairResult(r)) if !r.allowed));

    let (_c2, port) = start(
        &["--auto-allow", "--revoke-after", "1"],
        &dir.path().join("b.jsonl"),
    );
    let mut c = Client::connect(port).await.unwrap();
    c.hello("i", "com.x", &[]).await;
    c.send(Msg::PairRequest(wire::PairRequest {
        commit: sdk::pairing::commit_of(&nonce).to_vec(),
    }))
    .await;
    c.next().await;
    c.send(Msg::PairReveal(wire::PairReveal {
        client_nonce: nonce.to_vec(),
    }))
    .await;
    assert!(matches!(c.next().await, Some(Msg::PairResult(r)) if r.allowed));
    c.send(Msg::CallStart(call_start("c1", "https://api.test/x")))
        .await;
    c.send(Msg::CallComplete(complete("c1"))).await;
    c.expect_closed().await;
}

#[tokio::test]
async fn config_flag_pushes_a_fixed_config() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = dir.path().join("config.json");
    std::fs::write(
        &cfg,
        r#"{"capture_enabled": false, "redaction": "strict", "host_deny": ["ads.test"]}"#,
    )
    .unwrap();
    let (_c, port) = start(
        &["--auto-allow", "--config", cfg.to_str().unwrap()],
        &dir.path().join("o.jsonl"),
    );
    let mut c = Client::connect(port).await.unwrap();
    c.hello("i", "com.x", &[]).await;
    let nonce = [1u8; 32];
    c.send(Msg::PairRequest(wire::PairRequest {
        commit: sdk::pairing::commit_of(&nonce).to_vec(),
    }))
    .await;
    c.next().await;
    c.send(Msg::PairReveal(wire::PairReveal {
        client_nonce: nonce.to_vec(),
    }))
    .await;
    c.next().await;
    match c.next().await {
        Some(Msg::Config(w)) => {
            assert!(!w.capture_enabled);
            assert_eq!(w.redaction, wire::config::Redaction::Strict as i32);
            assert_eq!(w.host_deny, vec!["ads.test"]);
        }
        o => panic!("{o:?}"),
    }
}
