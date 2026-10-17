use bytes::Bytes;
use flow::{flags, FlowRecord, FlowState, Timings};
use proxy_store::{Kind, Query, Store};
use std::sync::Arc;

fn rec(id: u64, host: &str, path: &str, status: u16, ms: u64, body: &str) -> Arc<FlowRecord> {
    Arc::new(FlowRecord {
        id,
        started_at_ms: 1_700_000_000_000 + id as i64,
        scheme: "https".into(),
        host: host.into(),
        port: 443,
        method: if id.is_multiple_of(2) { "POST" } else { "GET" }.into(),
        path: path.into(),
        query: String::new(),
        status,
        protocol: "h2".into(),
        state: FlowState::Complete,
        req_headers: vec![("Accept".into(), "application/json".into())],
        res_headers: vec![("Content-Type".into(), "application/json".into())],
        res_size: body.len() as u64,
        res_body: Bytes::from(body.to_string()),
        timings: Timings {
            total_us: ms * 1000,
            ..Default::default()
        },
        ..Default::default()
    })
}

#[test]
fn roundtrip_search_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.enqueue(rec(
        1,
        "api.example.com",
        "/v1/me",
        200,
        80,
        r#"{"name":"Asha Rao","plan":"pro"}"#,
    ));
    store.enqueue(rec(
        2,
        "auth.example.com",
        "/oauth/token",
        401,
        140,
        r#"{"error":"invalid_grant"}"#,
    ));
    store.enqueue(rec(3, "cdn.example.net", "/img/a.webp", 200, 2400, "x"));
    // Visible immediately, before the writer commits.
    assert!(store.get(1).unwrap().is_some());
    store.flush();
    assert_eq!(store.count().unwrap(), 3);
    assert_eq!(store.last_id().unwrap(), 3);

    let all = store.search(&Query::default(), 0, 100).unwrap();
    assert_eq!(all.total, 3);
    assert_eq!(
        all.items.iter().map(|s| s.id).collect::<Vec<_>>(),
        [1, 2, 3]
    );

    // Text search reaches host, path and body text.
    let q = |t: &str| Query {
        text: t.into(),
        ..Default::default()
    };
    assert_eq!(store.search(&q("oauth"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("asha"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("example.com"), 0, 10).unwrap().total, 2);
    assert_eq!(store.search(&q("invalid_grant"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("401"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("method:post"), 0, 10).unwrap().total, 1);
    assert_eq!(
        store
            .search(&q("status:2xx host:example.net"), 0, 10)
            .unwrap()
            .total,
        1
    );
    assert_eq!(
        store.search(&q("nothing-like-this"), 0, 10).unwrap().total,
        0
    );

    // Chips.
    let k = |kind| Query {
        kind,
        ..Default::default()
    };
    assert_eq!(store.search(&k(Kind::Errors), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&k(Kind::Slow), 0, 10).unwrap().items[0].id, 3);

    // Paging.
    let page = store.search(&Query::default(), 1, 1).unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.items[0].id, 2);

    // Bodies come back intact from blobs.
    let f = store.get(2).unwrap().unwrap();
    assert_eq!(&f.res_body[..], br#"{"error":"invalid_grant"}"#);
    assert_eq!(f.req_headers[0].0, "Accept");
    assert_eq!(f.timings.total_us, 140_000);

    assert_eq!(store.clear().unwrap(), 3);
    assert_eq!(store.count().unwrap(), 0);
    assert!(store.get(1).unwrap().is_none());
    store.close();
}

#[test]
fn survives_reopen_and_flags_are_searchable() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = Store::open(dir.path()).unwrap();
        let mut r = (*rec(7, "pay.example.com", "/session", 0, 0, "")).clone();
        r.flags = flags::PINNED | flags::TUNNEL;
        r.state = FlowState::Failed;
        r.error = "client rejected certificate".into();
        store.enqueue(Arc::new(r));
        store.close();
    }
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(store.last_id().unwrap(), 7);
    let pinned = store
        .search(
            &Query {
                kind: Kind::Pinned,
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(pinned.total, 1);
    assert_eq!(pinned.items[0].error, "client rejected certificate");
    store.close();
}

#[test]
fn identical_bodies_share_one_blob() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for i in 1..=5 {
        store.enqueue(rec(i, "a.test", "/same", 200, 10, "same body"));
    }
    store.flush();
    let mut files = 0;
    for shard in std::fs::read_dir(dir.path().join("blobs"))
        .unwrap()
        .flatten()
    {
        files += std::fs::read_dir(shard.path()).unwrap().count();
    }
    assert_eq!(files, 1);
    store.close();
}

#[test]
fn har_has_expected_shape_and_redacts() {
    let mut r = (*rec(
        1,
        "api.example.com",
        "/v1/me",
        200,
        80,
        r#"{"password":"hunter2","name":"a"}"#,
    ))
    .clone();
    r.query = "access_token=abc&page=1".into();
    r.req_headers
        .push(("Authorization".into(), "Bearer abc".into()));
    r.res_headers
        .push(("Set-Cookie".into(), "sid=secret; Path=/".into()));
    r.timings = Timings {
        dns_us: 1000,
        connect_us: 2000,
        tls_us: 3000,
        send_us: 0,
        wait_us: 40_000,
        receive_us: 5000,
        total_us: 51_000,
        reused: false,
    };
    let recs = vec![Arc::new(r)];

    let har = proxy_store::to_har(&recs, true, "0.1.0");
    let e = &har["log"]["entries"][0];
    assert_eq!(har["log"]["version"], "1.2");
    assert_eq!(e["request"]["method"], "GET");
    assert_eq!(e["response"]["status"], 200);
    assert_eq!(e["timings"]["ssl"], 3.0);
    assert_eq!(e["timings"]["connect"], 5.0);
    let text = e["response"]["content"]["text"].as_str().unwrap();
    assert!(text.contains("[redacted]") && !text.contains("hunter2"));
    let auth = e["request"]["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["name"] == "Authorization")
        .unwrap();
    assert_eq!(auth["value"], "[redacted]");
    assert!(!e["request"]["url"].as_str().unwrap().contains("abc"));
    assert_eq!(e["response"]["cookies"][0]["value"], "[redacted]");

    let raw = proxy_store::to_har(&recs, false, "0.1.0");
    let auth = raw["log"]["entries"][0]["request"]["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["name"] == "Authorization")
        .unwrap();
    assert_eq!(auth["value"], "Bearer abc");
}

#[test]
fn tunnels_are_not_exported() {
    let mut r = (*rec(1, "pay.example.com", "", 0, 0, "")).clone();
    r.flags = flags::TUNNEL;
    let har = proxy_store::to_har(&[Arc::new(r)], true, "0.1.0");
    assert_eq!(har["log"]["entries"].as_array().unwrap().len(), 0);
}

fn sdk_rec(id: u64, device: &str, name: &str, body: &str) -> Arc<FlowRecord> {
    let mut r = (*rec(id, "api.shop.test", "/v1/cart/7/coupon", 200, 50, body)).clone();
    r.source = flow::Source::Sdk;
    r.app = "com.shop.app".into();
    r.sdk = Some(Arc::new(flow::SdkInfo {
        device_id: device.into(),
        app_id: "com.shop.app".into(),
        call_id: format!("call-{id}"),
        name: name.into(),
        owner: "payments".into(),
        screen: "CartActivity".into(),
        route_template: "/v1/cart/{id}/coupon".into(),
        client_stack: "okhttp".into(),
        device_name: "Pixel 8".into(),
        app_version: "1.2.3".into(),
        timings: Some(flow::SdkTimings {
            dns: 3,
            ..Default::default()
        }),
        res_body: flow::BodyState::Lazy,
        ..Default::default()
    }));
    Arc::new(r)
}

#[test]
fn sdk_fields_roundtrip_search_and_lazy_body() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.enqueue(sdk_rec(1, "dev-a", "Apply coupon", ""));
    store.enqueue(rec(2, "other.test", "/x", 200, 5, "{}"));
    store.flush();

    let got = store.get(1).unwrap().unwrap();
    assert_eq!(got.source, flow::Source::Sdk);
    let sdk = got.sdk.as_ref().unwrap();
    assert_eq!(sdk.name, "Apply coupon");
    assert_eq!(sdk.route_template, "/v1/cart/{id}/coupon");
    assert_eq!(sdk.app_version, "1.2.3");
    assert_eq!(sdk.timings.unwrap().dns, 3);
    assert_eq!(sdk.res_body, flow::BodyState::Lazy);
    assert!(store.get(2).unwrap().unwrap().sdk.is_none());

    let q = |t: &str| Query {
        text: t.into(),
        ..Default::default()
    };
    // Plain words reach SDK fields; operators filter on them.
    assert_eq!(store.search(&q("coupon"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("payments"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("CartActivity"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("name:apply"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("source:sdk"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("source:proxy"), 0, 10).unwrap().total, 1);
    assert_eq!(store.search(&q("device:pixel"), 0, 10).unwrap().total, 1);
    let by_device = Query {
        devices: vec!["dev-a".into()],
        ..Default::default()
    };
    assert_eq!(store.search(&by_device, 0, 10).unwrap().total, 1);
    let rows = store.devices().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].flows, 1);

    // A lazy body arriving later is stored and becomes searchable.
    store.update_body(
        1,
        proxy_store::BodyPart::Response,
        Bytes::from_static(br#"{"discount":"SPRING25"}"#),
        flow::BodyState::Present,
        false,
    );
    store.flush();
    let got = store.get(1).unwrap().unwrap();
    assert_eq!(&got.res_body[..], br#"{"discount":"SPRING25"}"#);
    assert_eq!(got.sdk.as_ref().unwrap().res_body, flow::BodyState::Present);
    assert_eq!(store.search(&q("SPRING25"), 0, 10).unwrap().total, 1);
}

#[test]
fn merged_pair_shows_one_row_and_unmerges() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    store.enqueue(sdk_rec(1, "dev-a", "Apply coupon", "{}"));
    store.enqueue(rec(2, "api.shop.test", "/v1/cart/7/coupon", 200, 50, "{}"));
    store.flush();
    let all = |s: &Store| s.search(&Query::default(), 0, 10).unwrap();
    assert_eq!(all(&store).total, 2);

    store.set_merge(1, 2);
    store.flush();
    let page = all(&store);
    assert_eq!(page.total, 1, "the wire half is hidden");
    assert_eq!(page.items[0].id, 1);
    assert_eq!(page.items[0].source, flow::Source::Merged);
    assert_eq!(page.items[0].merged_with, 2);
    assert_eq!(store.get(2).unwrap().unwrap().merged_with, 1);
    let with_hidden = Query {
        include_hidden: true,
        ..Default::default()
    };
    assert_eq!(store.search(&with_hidden, 0, 10).unwrap().total, 2);

    // Unmerging restores both rows.
    store.set_merge(1, 0);
    store.flush();
    let page = all(&store);
    assert_eq!(page.total, 2);
    assert_eq!(store.get(1).unwrap().unwrap().source, flow::Source::Sdk);
}

#[test]
fn ten_thousand_mixed_flows_search_and_filter_fast() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    for i in 1..=10_000u64 {
        let mut r = (*rec(
            i,
            &format!("api{}.example.com", i % 40),
            "/v1/items",
            200,
            20,
            r#"{"ok":true}"#,
        ))
        .clone();
        if i % 3 == 0 {
            r.source = flow::Source::Sdk;
            r.sdk = Some(Arc::new(flow::SdkInfo {
                device_id: format!("dev-{}", i % 4),
                app_id: "com.demo".into(),
                call_id: format!("c{i}"),
                name: if i % 6 == 0 {
                    "Apply coupon".into()
                } else {
                    "List items".into()
                },
                screen: "CartActivity".into(),
                route_template: "/v1/items/{id}".into(),
                ..Default::default()
            }));
        }
        store.enqueue(Arc::new(r));
    }
    // A merged pair in the middle.
    store.set_merge(3, 4);
    store.flush();

    let time = |q: Query| {
        let t = std::time::Instant::now();
        let p = store.search(&q, 0, 200).unwrap();
        (t.elapsed(), p.total)
    };
    let q = |text: &str| Query {
        text: text.into(),
        ..Default::default()
    };
    for (label, query) in [
        ("all", Query::default()),
        ("text", q("coupon")),
        ("source", q("source:sdk")),
        ("name", q("name:coupon")),
        (
            "device chip",
            Query {
                devices: vec!["dev-1".into()],
                ..Default::default()
            },
        ),
        ("host + source", q("api7.example.com source:proxy")),
    ] {
        let (d, total) = time(query);
        eprintln!("{label}: {total} rows in {d:?}");
        assert!(
            d < std::time::Duration::from_millis(100),
            "{label} took {d:?}"
        );
    }
    assert_eq!(time(q("source:sdk")).1, 3333);
}
