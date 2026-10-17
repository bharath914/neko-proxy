use bytes::Bytes;
use flow::{FlowEvent, FlowRecord, FlowSink, FlowState, Timings};
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use proxy_api::pb::{self, client_message::Body as Req, server_message::Body as Res};
use proxy_api::{ApiContext, Control, CoreInfo, Hub};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UnixStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

struct FakeControl {
    recording: AtomicBool,
}

impl Control for FakeControl {
    fn recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
    fn set_recording(&self, on: bool) {
        self.recording.store(on, Ordering::SeqCst);
    }
    fn pinned_hosts(&self) -> Vec<String> {
        vec!["pay.example.com".into()]
    }
    fn clear_pinned(&self) {}
    fn unpin_host(&self, _host: &str) -> bool {
        true
    }
    fn replay(
        &self,
        call: proxy_api::ReplayCall,
    ) -> flow::BoxFuture<'_, Result<(u64, u16), String>> {
        Box::pin(async move {
            if call.url.contains("fail") {
                Err("refused".into())
            } else {
                Ok((99, 200))
            }
        })
    }
}

fn record(id: u64, host: &str, status: u16, body: &str) -> Arc<FlowRecord> {
    Arc::new(FlowRecord {
        id,
        started_at_ms: 1_700_000_000_000 + id as i64,
        scheme: "https".into(),
        host: host.into(),
        port: 443,
        method: "GET".into(),
        path: "/v1/me".into(),
        status,
        protocol: "h2".into(),
        state: FlowState::Complete,
        res_headers: vec![("content-type".into(), "application/json".into())],
        res_body: Bytes::from(body.to_string()),
        res_size: body.len() as u64,
        timings: Timings {
            total_us: 84_000,
            wait_us: 50_000,
            ..Default::default()
        },
        ..Default::default()
    })
}

struct Client {
    ws: WebSocketStream<UnixStream>,
    next: u64,
}

impl Client {
    async fn send(&mut self, body: Req) -> u64 {
        self.next += 1;
        let m = pb::ClientMessage {
            id: self.next,
            body: Some(body),
        };
        self.ws
            .send(Message::binary(m.encode_to_vec()))
            .await
            .unwrap();
        self.next
    }

    async fn recv(&mut self) -> pb::ServerMessage {
        loop {
            match tokio::time::timeout(Duration::from_secs(5), self.ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
            {
                Message::Binary(b) => return pb::ServerMessage::decode(&b[..]).unwrap(),
                _ => continue,
            }
        }
    }

    async fn call(&mut self, body: Req) -> Res {
        let id = self.send(body).await;
        loop {
            let m = self.recv().await;
            if m.id == id {
                return m.body.unwrap();
            }
        }
    }
}

async fn setup() -> (Client, Arc<ApiContext>, tempfile::TempDir) {
    setup_with(false).await
}

async fn setup_with(with_sdk: bool) -> (Client, Arc<ApiContext>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = store::Store::open(&dir.path().join("data")).unwrap();
    let hub = Hub::new();
    let sdk = with_sdk.then(|| {
        // These tests walk through the pairing prompt, so connections from this computer must not skip it.
        sdk::SdkSettings { auto_accept_local: false, ..Default::default() }
            .save(&dir.path().join("sdk"))
            .unwrap();
        let mut o = sdk::SdkOptions::new(dir.path().join("sdk"));
        o.port = 0;
        o.bind = [127, 0, 0, 1].into();
        o.advertise = false;
        o.watch_adb = false;
        Arc::new(
            sdk::Sdk::new(
                o,
                Arc::new(hub.clone()),
                store.clone(),
                Arc::new(flow::IdGen::starting_after(0)),
            )
            .unwrap(),
        )
    });
    let ctx = Arc::new(ApiContext {
        store,
        hub,
        control: Arc::new(FakeControl {
            recording: AtomicBool::new(true),
        }),
        rules: Arc::new(rules::RuleSet::in_memory()),
        approvals: Arc::new(proxy_api::Approvals::default()),
        mcp: None,
        sdk,
        info: CoreInfo {
            version: "test".into(),
            listen: "127.0.0.1:8899".into(),
            data_dir: dir.path().display().to_string(),
            insecure_upstream: false,
            ca_path: "/x/ca.pem".into(),
            ca_sha256: "abc".into(),
            ca_subject: "Proxy Local CA".into(),
            ca_not_after: "2036-01-01T00:00:00Z".into(),
            ca_pem: "PEM".into(),
            ca_spki_sha256: "spki".into(),
            ca_key_storage: "file".into(),
            started: Instant::now(),
        },
    });
    let sock = dir.path().join("run").join("api.sock");
    let c2 = ctx.clone();
    let s2 = sock.clone();
    tokio::spawn(async move {
        proxy_api::serve(c2, &s2, std::future::pending())
            .await
            .unwrap();
    });
    for _ in 0..200 {
        if sock.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let stream = UnixStream::connect(&sock).await.unwrap();
    let (ws, _) = tokio_tungstenite::client_async("ws://localhost/", stream)
        .await
        .unwrap();
    (Client { ws, next: 0 }, ctx, dir)
}

#[tokio::test]
async fn socket_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let (_c, _ctx, dir) = setup().await;
    let mode = std::fs::metadata(dir.path().join("run/api.sock"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn subscribe_batches_events_and_dedupes() {
    let (mut c, ctx, _d) = setup().await;
    let Res::Subscribed(s) = c
        .call(Req::Subscribe(pb::SubscribeRequest::default()))
        .await
    else {
        panic!("expected subscribed")
    };
    assert_eq!(s.total, 0);

    // Three updates for one flow inside one batch window collapse to one.
    let mut summary = record(1, "api.example.com", 0, "").summary();
    summary.state = FlowState::Pending;
    ctx.hub.publish(FlowEvent::Started(summary.clone()));
    summary.state = FlowState::Receiving;
    summary.status = 200;
    ctx.hub.publish(FlowEvent::Updated(summary));
    ctx.hub
        .publish(FlowEvent::Finished(record(1, "api.example.com", 200, "{}")));
    ctx.hub
        .publish(FlowEvent::Finished(record(2, "cdn.example.net", 404, "")));

    let mut seen = std::collections::HashMap::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while seen.len() < 2 && Instant::now() < deadline {
        let m = c.recv().await;
        if let Some(Res::Batch(b)) = m.body {
            assert_eq!(m.id, 0);
            for u in b.upserts {
                seen.insert(u.id, u);
            }
        }
    }
    assert_eq!(seen[&1].state, pb::FlowState::Complete as i32);
    assert_eq!(seen[&2].status, 404);
}

#[tokio::test]
async fn subscription_filter_applies() {
    let (mut c, ctx, _d) = setup().await;
    c.call(Req::Subscribe(pb::SubscribeRequest {
        filter: Some(pb::Filter {
            kind: pb::filter::Kind::Errors as i32,
            text: String::new(),
            ..Default::default()
        }),
    }))
    .await;
    ctx.hub
        .publish(FlowEvent::Finished(record(1, "ok.example.com", 200, "")));
    ctx.hub
        .publish(FlowEvent::Finished(record(2, "bad.example.com", 500, "")));
    let m = c.recv().await;
    let Some(Res::Batch(b)) = m.body else {
        panic!("expected batch")
    };
    assert_eq!(b.upserts.len(), 1);
    assert_eq!(b.upserts[0].id, 2);
}

#[tokio::test]
async fn search_get_export_clear() {
    let (mut c, ctx, _d) = setup().await;
    ctx.store.enqueue(record(
        1,
        "api.example.com",
        200,
        r#"{"name":"Asha","password":"pw"}"#,
    ));
    ctx.store.enqueue(record(2, "cdn.example.net", 404, "gone"));
    ctx.store.flush();

    let Res::Search(r) = c
        .call(Req::Search(pb::SearchRequest {
            filter: Some(pb::Filter {
                kind: 0,
                text: "asha".into(),
                ..Default::default()
            }),
            ..Default::default()
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(r.total, 1);
    assert_eq!(r.items[0].id, 1);

    let Res::Flow(d) = c
        .call(Req::GetFlow(pb::GetFlowRequest {
            id: 1,
            parts: 1 | 4,
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(d.summary.unwrap().host, "api.example.com");
    assert_eq!(d.url, "https://api.example.com/v1/me");
    assert!(d.curl.starts_with("curl"));
    assert_eq!(d.timings.unwrap().total_ms, 84.0);
    let body = d.response_body.unwrap();
    assert_eq!(body.kind, pb::BodyKind::Json as i32);
    assert_eq!(body.json.len(), 4);
    assert!(d.request_body.is_none());

    let Res::Error(e) = c
        .call(Req::GetFlow(pb::GetFlowRequest { id: 99, parts: 1 }))
        .await
    else {
        panic!()
    };
    assert_eq!(e.code, "not_found");

    let Res::Export(x) = c
        .call(Req::Export(pb::ExportRequest {
            kind: "har".into(),
            redact: true,
            ..Default::default()
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(x.count, 2);
    let har: serde_json::Value = serde_json::from_slice(&x.data).unwrap();
    assert_eq!(har["log"]["version"], "1.2");
    assert!(!String::from_utf8_lossy(&x.data).contains("\"pw\""));
    assert!(x.filename.ends_with(".har"));

    let Res::Cleared(cl) = c.call(Req::Clear(pb::ClearRequest {})).await else {
        panic!()
    };
    assert_eq!(cl.removed, 2);
    let Res::Search(r) = c.call(Req::Search(pb::SearchRequest::default())).await else {
        panic!()
    };
    assert_eq!(r.total, 0);
}

#[tokio::test]
async fn html_bodies_arrive_as_preview_blocks_and_source() {
    let (mut c, ctx, _d) = setup().await;
    let page = "<html><head><title>Docs</title><script>boom()</script></head>\
                <body><h1>Hello</h1><p>See <a href=\"/x\">this</a> &amp; that</p></body></html>";
    let mut rec = (*record(7, "docs.example.com", 200, page)).clone();
    rec.res_headers = vec![("content-type".into(), "text/html; charset=utf-8".into())];
    ctx.store.enqueue(Arc::new(rec));
    ctx.store.flush();

    let Res::Flow(d) = c
        .call(Req::GetFlow(pb::GetFlowRequest { id: 7, parts: 4 }))
        .await
    else {
        panic!()
    };
    let body = d.response_body.unwrap();
    assert_eq!(body.kind, pb::BodyKind::Html as i32);
    assert_eq!(body.html_title, "Docs");
    assert_eq!(body.html.len(), 2);
    assert_eq!(body.html[0].kind, pb::HtmlBlockKind::HtmlHeading as i32);
    assert_eq!(body.html[0].level, 1);
    let link = &body.html[1].spans[1];
    assert_eq!(link.text, "this");
    assert_eq!(link.href, "/x");
    assert_eq!(link.style & 8, 8);
    assert_eq!(body.html[1].spans[2].text, " & that");
    // The script never reaches the preview, but the source keeps it.
    assert!(!body
        .html
        .iter()
        .any(|b| b.spans.iter().any(|s| s.text.contains("boom"))));
    assert!(body.source.iter().any(|l| l
        .tokens
        .iter()
        .any(|t| t.kind == pb::SourceTokenKind::SrcTag as i32 && t.text == "h1")));
    assert_eq!(body.text, page);
}

#[tokio::test]
async fn hosts_list_and_exact_host_focus() {
    let (mut c, ctx, _d) = setup().await;
    ctx.store.enqueue(record(1, "api.example.com", 200, "{}"));
    ctx.store.enqueue(record(2, "api.example.com", 500, "{}"));
    ctx.store.enqueue(record(3, "example.com", 200, "{}"));
    ctx.store.enqueue(record(4, "pay.example.com", 200, "{}"));
    ctx.store.flush();

    let Res::Hosts(h) = c.call(Req::Hosts(pb::HostsRequest::default())).await else {
        panic!()
    };
    assert_eq!(h.hosts.len(), 3);
    // Newest activity first.
    assert_eq!(h.hosts[0].host, "pay.example.com");
    assert!(
        h.hosts[0].pinned,
        "the fake control reports this host pinned"
    );
    let api = h
        .hosts
        .iter()
        .find(|x| x.host == "api.example.com")
        .unwrap();
    assert_eq!((api.flows, api.errors), (2, 1));
    assert!(api.decrypted);

    // Focus is an exact match: "example.com" does not pull in "api.example.com".
    let search = |hosts: Vec<String>| {
        Req::Search(pb::SearchRequest {
            filter: Some(pb::Filter {
                hosts,
                ..Default::default()
            }),
            ..Default::default()
        })
    };
    let Res::Search(r) = c.call(search(vec!["example.com".into()])).await else {
        panic!()
    };
    assert_eq!(r.total, 1);
    assert_eq!(r.items[0].host, "example.com");
    let Res::Search(r) = c
        .call(search(vec![
            "API.example.com".into(),
            "pay.example.com".into(),
        ]))
        .await
    else {
        panic!()
    };
    assert_eq!(r.total, 3);
}

#[tokio::test]
async fn ssl_rules_and_map_local_over_the_api() {
    let (mut c, ctx, _d) = setup().await;
    let Res::Rules(r) = c.call(Req::Rules(pb::RulesRequest {})).await else {
        panic!()
    };
    assert_eq!(r.ssl.unwrap().mode, pb::SslMode::SslAll as i32);
    assert!(r.map_local.is_empty());
    assert_eq!(r.pinned_hosts, ["pay.example.com"]);

    // Turn one host off without touching the others.
    let Res::Rules(r) = c
        .call(Req::SslHost(pb::SslHostRequest {
            host: "Api.Example.com".into(),
            enable: false,
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(r.ssl.unwrap().exclude, ["api.example.com"]);
    assert!(!ctx.rules.decrypts("api.example.com", 443));
    assert!(ctx.rules.decrypts("cdn.example.com", 443));

    // Switch to a list.
    let Res::Rules(r) = c
        .call(Req::SetSsl(pb::SetSslRequest {
            ssl: Some(pb::SslSettings {
                mode: pb::SslMode::SslListed as i32,
                hosts: vec!["*.example.com".into()],
                exclude: vec![],
            }),
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(r.ssl.unwrap().hosts, ["*.example.com"]);
    assert!(ctx.rules.decrypts("a.example.com", 443));
    assert!(!ctx.rules.decrypts("other.org", 443));

    // Bad pattern is refused, with the rules left as they were.
    let Res::Error(e) = c
        .call(Req::SetSsl(pb::SetSslRequest {
            ssl: Some(pb::SslSettings {
                mode: pb::SslMode::SslListed as i32,
                hosts: vec!["https://x/".into()],
                exclude: vec![],
            }),
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(e.code, "invalid_rule");
    assert!(ctx.rules.decrypts("a.example.com", 443));

    // Map Local: create, list, replace by id, remove.
    let rule = pb::MapLocalRule {
        enabled: true,
        host: "api.example.com".into(),
        path: "/v1/me".into(),
        file: "/tmp/me.json".into(),
        ..Default::default()
    };
    let Res::Rules(r) = c
        .call(Req::PutMapLocal(pb::PutMapLocalRequest {
            rule: Some(rule),
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(r.map_local.len(), 1);
    let created = r.map_local[0].clone();
    assert!(!created.id.is_empty());
    assert_eq!(created.status, 200);
    assert_eq!(ctx.rules.map_local().len(), 1);

    let Res::Rules(r) = c
        .call(Req::RemoveRule(pb::RemoveRuleRequest { id: created.id }))
        .await
    else {
        panic!()
    };
    assert!(r.map_local.is_empty());

    let Res::Error(e) = c
        .call(Req::PutMapLocal(pb::PutMapLocalRequest {
            rule: Some(pb::MapLocalRule {
                host: "".into(),
                path: "/x".into(),
                file: "/f".into(),
                ..Default::default()
            }),
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(e.code, "invalid_rule");
}

#[tokio::test]
async fn capture_and_status() {
    let (mut c, _ctx, _d) = setup().await;
    let Res::Capture(s) = c
        .call(Req::Capture(pb::CaptureRequest {
            action: pb::capture_request::Action::Stop as i32,
        }))
        .await
    else {
        panic!()
    };
    assert!(!s.recording);
    let Res::Capture(s) = c
        .call(Req::Capture(pb::CaptureRequest {
            action: pb::capture_request::Action::Start as i32,
        }))
        .await
    else {
        panic!()
    };
    assert!(s.recording);

    let Res::Status(st) = c.call(Req::Status(pb::StatusRequest {})).await else {
        panic!()
    };
    assert_eq!(st.listen, "127.0.0.1:8899");
    assert_eq!(st.pinned_hosts, ["pay.example.com"]);
    assert!(st.recording);

    let Res::CaInfo(ca) = c.call(Req::CaInfo(pb::CaInfoRequest {})).await else {
        panic!()
    };
    assert_eq!(ca.sha256, "abc");
}

#[tokio::test]
async fn clear_notifies_subscribers() {
    let (mut c, ctx, _d) = setup().await;
    c.call(Req::Subscribe(pb::SubscribeRequest::default()))
        .await;
    ctx.hub.cleared();
    let m = c.recv().await;
    let Some(Res::Batch(b)) = m.body else {
        panic!()
    };
    assert!(b.cleared);
}

// ------------------------------------------------------- rules, breakpoints, approvals

fn mock_rule(host: &str) -> pb::Rule {
    pb::Rule {
        id: String::new(),
        name: "down".into(),
        enabled: true,
        matcher: Some(pb::RuleMatch {
            host: host.into(),
            path: "/v1/*".into(),
            ..Default::default()
        }),
        action: Some(pb::rule::Action::Mock(pb::MockAction {
            status: 503,
            headers: vec![],
            body: "down".into(),
            delay_ms: 0,
        })),
    }
}

#[tokio::test]
async fn rules_roundtrip_toggle_and_validate() {
    let (mut c, ctx, _d) = setup().await;
    let Res::Rules(r) = c
        .call(Req::PutRule(pb::PutRuleRequest {
            rule: Some(mock_rule("API.example.com")),
        }))
        .await
    else {
        panic!("rules response")
    };
    assert_eq!(r.rules.len(), 1);
    let rule = &r.rules[0];
    assert_eq!(rule.matcher.as_ref().unwrap().host, "api.example.com");
    assert!(!rule.id.is_empty());
    assert!(matches!(rule.action, Some(pb::rule::Action::Mock(ref m)) if m.status == 503));

    let Res::Rules(r) = c
        .call(Req::SetRuleEnabled(pb::SetRuleEnabledRequest {
            id: rule.id.clone(),
            enabled: false,
        }))
        .await
    else {
        panic!()
    };
    assert!(!r.rules[0].enabled);
    assert!(!ctx.rules.rules()[0].enabled);

    // Invalid rules are refused with a reason.
    let mut bad = mock_rule("x.test");
    if let Some(pb::rule::Action::Mock(m)) = &mut bad.action {
        m.status = 42;
    }
    let Res::Error(e) = c
        .call(Req::PutRule(pb::PutRuleRequest { rule: Some(bad) }))
        .await
    else {
        panic!("expected an error")
    };
    assert_eq!(e.code, "invalid_rule");
    assert!(e.message.contains("status"), "{}", e.message);

    let Res::Rules(r) = c
        .call(Req::RemoveRule(pb::RemoveRuleRequest {
            id: rule.id.clone(),
        }))
        .await
    else {
        panic!()
    };
    assert!(r.rules.is_empty());
}

#[tokio::test]
async fn watchers_see_breakpoints_and_can_resume_them() {
    let (mut c, ctx, _d) = setup().await;
    let Res::Paused(p) = c.call(Req::Watch(pb::WatchRequest {})).await else {
        panic!()
    };
    assert!(p.items.is_empty());

    let rules = ctx.rules.clone();
    let waiter = tokio::spawn(async move {
        let flow = flow::FlowSummary {
            id: 11,
            host: "api.example.com".into(),
            ..Default::default()
        };
        let held = flow::Held {
            method: "GET".into(),
            path_and_query: "/v1/me".into(),
            body: Bytes::from_static(b"hi"),
            ..Default::default()
        };
        rules
            .pauses()
            .hold(
                &flow,
                "r1".into(),
                flow::Phase::Request,
                Duration::from_secs(10),
                held,
            )
            .await
    });

    // The pause is pushed with id 0.
    let pushed = loop {
        let m = c.recv().await;
        if let Some(Res::PauseNotice(n)) = m.body {
            break n;
        }
    };
    let held = pushed.paused.expect("paused message");
    assert_eq!(held.flow_id, 11);
    assert_eq!(held.path, "/v1/me");
    assert_eq!(held.body, b"hi");

    let Res::Paused(list) = c.call(Req::Paused(pb::ListPausedRequest {})).await else {
        panic!()
    };
    assert_eq!(list.items.len(), 1);

    let Res::Paused(list) = c
        .call(Req::Resume(pb::ResumeRequest {
            flow_id: 11,
            path: Some("/v1/other".into()),
            body: Some(b"changed".to_vec()),
            ..Default::default()
        }))
        .await
    else {
        panic!()
    };
    assert!(list.items.is_empty());
    match waiter.await.unwrap() {
        flow::Verdict::Continue(e) => {
            assert_eq!(e.path_and_query.as_deref(), Some("/v1/other"));
            assert_eq!(&e.body.unwrap()[..], b"changed");
        }
        flow::Verdict::Abort(_) => panic!("not aborted"),
    }

    // Releasing is announced too.
    let released = loop {
        let m = c.recv().await;
        if let Some(Res::PauseNotice(n)) = m.body {
            if n.released_flow_id != 0 {
                break n;
            }
        }
    };
    assert_eq!(released.released_flow_id, 11);
    assert_eq!(released.how, "edited");

    let Res::Error(e) = c
        .call(Req::Resume(pb::ResumeRequest {
            flow_id: 11,
            ..Default::default()
        }))
        .await
    else {
        panic!("nothing is held any more")
    };
    assert_eq!(e.code, "not_found");
}

#[tokio::test]
async fn approvals_are_pushed_and_decided() {
    let (mut c, ctx, _d) = setup().await;
    c.call(Req::Watch(pb::WatchRequest {})).await;
    let approvals = ctx.approvals.clone();
    let asker = tokio::spawn(async move {
        approvals
            .ask(
                "create_mock",
                "Mock 503".into(),
                "{}".into(),
                Duration::from_secs(10),
            )
            .await
    });
    let requested = loop {
        let m = c.recv().await;
        if let Some(Res::ApprovalNotice(n)) = m.body {
            if let Some(a) = n.requested {
                break a;
            }
        }
    };
    assert_eq!(requested.tool, "create_mock");
    let Res::Approvals(list) = c.call(Req::Approvals(pb::ListApprovalsRequest {})).await else {
        panic!()
    };
    assert_eq!(list.pending.len(), 1);
    c.call(Req::DecideApproval(pb::DecideApprovalRequest {
        id: requested.id.clone(),
        allow: true,
    }))
    .await;
    assert_eq!(asker.await.unwrap(), proxy_api::Decision::Allowed);
}

#[tokio::test]
async fn replay_edits_a_stored_flow_and_mcp_settings_round_trip() {
    let (mut c, ctx, _d) = setup().await;
    ctx.store.enqueue(record(5, "api.example.com", 200, "{}"));
    ctx.store.flush();

    let Res::Replay(r) = c
        .call(Req::Replay(pb::ReplayRequest {
            id: 5,
            ..Default::default()
        }))
        .await
    else {
        panic!("replay response")
    };
    assert_eq!((r.flow_id, r.status), (99, 200));

    let Res::Error(e) = c
        .call(Req::Replay(pb::ReplayRequest {
            id: 5,
            url: Some("https://fail.example.com/".into()),
            ..Default::default()
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(e.code, "replay");
    assert_eq!(e.message, "refused");

    let Res::Error(e) = c
        .call(Req::Replay(pb::ReplayRequest {
            id: 404,
            ..Default::default()
        }))
        .await
    else {
        panic!()
    };
    assert!(e.message.contains("no flow"), "{}", e.message);

    let Res::McpInfo(i) = c
        .call(Req::SetMcpWrites(pb::SetMcpWritesRequest {
            writes: pb::McpWrites::McpAuto as i32,
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(i.writes, pb::McpWrites::McpAuto as i32);
    assert!(!i.enabled, "no MCP endpoint in this context");
    assert_eq!(ctx.rules.mcp_writes(), rules::McpWrites::Auto);
}

#[tokio::test]
async fn sdk_calls_are_unavailable_without_a_listener() {
    let (mut c, _ctx, _dir) = setup().await;
    match c.call(Req::SdkStatus(pb::SdkStatusRequest {})).await {
        Res::SdkStatus(s) => assert!(!s.available),
        other => panic!("{other:?}"),
    }
    match c
        .call(Req::DevicesSubscribe(pb::DevicesSubscribeRequest {}))
        .await
    {
        Res::Error(e) => assert_eq!(e.code, "unavailable"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn devices_pairing_flow_over_the_api() {
    let (mut c, ctx, _dir) = setup_with(true).await;
    // Off until enabled.
    match c.call(Req::SdkStatus(pb::SdkStatusRequest {})).await {
        Res::SdkStatus(s) => assert!(s.available && !s.enabled),
        other => panic!("{other:?}"),
    }
    let port = match c
        .call(Req::SdkSetEnabled(pb::SdkSetEnabledRequest {
            enabled: true,
        }))
        .await
    {
        Res::SdkStatus(s) => {
            assert!(s.enabled);
            assert_eq!(s.fingerprint.len(), 64);
            assert!(
                s.checks.iter().any(|c| c.id == "listener" && c.level == "ok"),
                "{:?}",
                s.checks
            );
            assert!(!s.health.is_empty() && s.health != "off" && s.health != "problem", "{}", s.health);
            s.listening[0]
                .rsplit(':')
                .next()
                .unwrap()
                .parse::<u16>()
                .unwrap()
        }
        other => panic!("{other:?}"),
    };
    match c
        .call(Req::DevicesSubscribe(pb::DevicesSubscribeRequest {}))
        .await
    {
        Res::Devices(d) => assert!(d.enabled && d.devices.is_empty() && d.pending.is_empty()),
        other => panic!("{other:?}"),
    }

    // A device asks to pair; the prompt is pushed to the subscriber.
    let mut dev = sdk::fake_client::Client::connect(port).await.unwrap();
    dev.hello("inst-1", "com.demo", &[]).await;
    let device = tokio::spawn(async move {
        let (code, res) = dev.pair_wait(|_| {}).await;
        (code, res, dev)
    });
    let prompt = loop {
        let m = c.recv().await;
        if let Some(Res::Devices(d)) = m.body {
            assert_eq!(m.id, 0, "pushed, not an answer");
            if let Some(p) = d.pending.into_iter().next() {
                break p;
            }
        }
    };
    assert_eq!(prompt.app_id, "com.demo");
    assert_eq!(prompt.code.len(), 6);
    match c
        .call(Req::DeviceAction(pb::DeviceActionRequest {
            kind: pb::device_action_request::Kind::Allow as i32,
            pending_id: prompt.id,
            ..Default::default()
        }))
        .await
    {
        Res::Ok(_) => {}
        other => panic!("{other:?}"),
    }
    let (code, res, _dev) = device.await.unwrap();
    assert_eq!(
        code, prompt.code,
        "the device computed the code the desktop showed"
    );
    assert!(res.allowed);

    // The device list now has it; push config and revoke.
    let cfg = pb::DeviceConfig {
        capture_enabled: false,
        redaction: pb::Redaction::RedactStrict as i32,
        body_cap_bytes: 2048,
        eager_body_bytes: 1024,
        ..Default::default()
    };
    match c
        .call(Req::SetDeviceConfig(pb::SetDeviceConfigRequest {
            device_id: "inst-1".into(),
            app_id: "com.demo".into(),
            config: Some(cfg),
        }))
        .await
    {
        Res::Ok(_) => {}
        other => panic!("{other:?}"),
    }
    let sdk = ctx.sdk.as_ref().unwrap();
    let snap = sdk.snapshot();
    let d = snap
        .devices
        .iter()
        .find(|d| d.facts.device_id == "inst-1")
        .unwrap();
    assert!(!d.config.capture_enabled);
    assert_eq!(d.config.body_cap_bytes, 2048);
    match c
        .call(Req::DeviceAction(pb::DeviceActionRequest {
            kind: pb::device_action_request::Kind::Revoke as i32,
            device_id: "inst-1".into(),
            app_id: "com.demo".into(),
            ..Default::default()
        }))
        .await
    {
        Res::Ok(_) => {}
        other => panic!("{other:?}"),
    }
    // Unknown prompt ids are an error, not a crash.
    match c
        .call(Req::DeviceAction(pb::DeviceActionRequest {
            pending_id: 999,
            ..Default::default()
        }))
        .await
    {
        Res::Error(e) => assert_eq!(e.code, "not_found"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn sdk_settings_round_trip_and_show_in_the_checks() {
    let (mut c, _ctx, _dir) = setup_with(true).await;
    let set = |lan, auto| {
        Req::SdkSetSettings(pb::SdkSetSettingsRequest { enabled: true, lan, auto_accept_local: auto })
    };
    match c.call(set(false, true)).await {
        Res::SdkStatus(s) => {
            assert!(s.enabled && !s.lan && s.auto_accept_local);
            assert!(s.listening.iter().all(|a| a.starts_with("127.0.0.1")), "{:?}", s.listening);
            assert!(s.checks.iter().any(|c| c.id == "auto-accept" && c.level == "ok"));
            assert!(s.checks.iter().any(|c| c.id == "wifi" && c.level == "info"));
        }
        other => panic!("{other:?}"),
    }
    match c.call(set(false, false)).await {
        Res::SdkStatus(s) => assert!(s.checks.iter().any(|c| c.id == "auto-accept" && c.level == "info")),
        other => panic!("{other:?}"),
    }
    // Turning it off is "off", not a problem.
    match c
        .call(Req::SdkSetSettings(pb::SdkSetSettingsRequest { enabled: false, lan: false, auto_accept_local: false }))
        .await
    {
        Res::SdkStatus(s) => assert_eq!(s.health, "off"),
        other => panic!("{other:?}"),
    }
}
