//! Merging the SDK's view of a call with the proxy's view of the same call.
//!
//! The matching rule is a pure function over two candidates. The [`Merger`]
//! keeps recent unmatched candidates from each side and links the best pair
//! when the second half of a pair finishes. Records are never rewritten:
//! the link is `merged_with` on both, so un-merging is just removing it.

use flow::{FlowEvent, FlowId, FlowRecord, FlowSink, Source};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use store::Store;

/// How far apart two starts can be and still be one call.
pub const WINDOW_MS: i64 = 500;
const KEEP: usize = 512;

/// Everything the rule looks at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub id: FlowId,
    pub method: String,
    /// See [`normalize_url`].
    pub url: String,
    /// Start time in core time (device clocks already corrected).
    pub started_ms: i64,
    /// Hash of the request body, if it was captured whole and unencoded.
    pub body: Option<u64>,
    /// SDK side: the device. Proxy side: the device the client address maps
    /// to, if that could be determined.
    pub device: Option<String>,
}

/// Scheme, host, port, path and sorted query, lower-cased where case does
/// not matter. Default ports are dropped.
pub fn normalize_url(scheme: &str, host: &str, port: u16, path: &str, query: &str) -> String {
    let scheme = scheme.to_ascii_lowercase();
    let default = match scheme.as_str() {
        "https" | "wss" => 443,
        "http" | "ws" => 80,
        _ => 0,
    };
    let mut pairs: Vec<&str> = query.split('&').filter(|p| !p.is_empty()).collect();
    pairs.sort_unstable();
    let mut s = format!("{scheme}://{}", host.to_ascii_lowercase());
    if port != 0 && port != default {
        s.push(':');
        s.push_str(&port.to_string());
    }
    s.push_str(if path.is_empty() { "/" } else { path });
    if !pairs.is_empty() {
        s.push('?');
        s.push_str(&pairs.join("&"));
    }
    s
}

/// Would these two records be the same call? `Some(distance)` if so, where
/// a smaller distance is a better match.
pub fn score(sdk: &Candidate, proxy: &Candidate) -> Option<i64> {
    if !sdk.method.eq_ignore_ascii_case(&proxy.method) || sdk.url != proxy.url {
        return None;
    }
    let dt = (sdk.started_ms - proxy.started_ms).abs();
    if dt > WINDOW_MS {
        return None;
    }
    // The device must be known and equal: a proxied connection that cannot be
    // attributed is never merged.
    match (&sdk.device, &proxy.device) {
        (Some(a), Some(b)) if a == b => {}
        _ => return None,
    }
    if let (Some(a), Some(b)) = (sdk.body, proxy.body) {
        if a != b {
            return None;
        }
    }
    Some(dt)
}

/// The best partner for `new` among `pool`, consuming nothing.
pub fn best_match<'a>(new: &Candidate, pool: &'a [Candidate], new_is_sdk: bool) -> Option<&'a Candidate> {
    pool.iter()
        .filter_map(|c| {
            let s = if new_is_sdk { score(new, c) } else { score(c, new) };
            s.map(|d| (d, c))
        })
        .min_by_key(|(d, c)| (*d, c.id))
        .map(|(_, c)| c)
}

/// Body hash for matching, or None when the body cannot be compared fairly
/// (content-encoded, truncated, or absent).
pub fn body_hash(rec: &FlowRecord) -> Option<u64> {
    let enc = rec.request_header("content-encoding");
    if rec.req_body.is_empty()
        || !(enc.is_empty() || enc.eq_ignore_ascii_case("identity"))
        || rec.flags & flow::flags::REQ_TRUNCATED != 0
    {
        return None;
    }
    let h = blake3::hash(&rec.req_body);
    Some(u64::from_le_bytes(h.as_bytes()[..8].try_into().unwrap()))
}

pub fn candidate(rec: &FlowRecord, device: Option<String>) -> Candidate {
    Candidate {
        id: rec.id,
        method: rec.method.clone(),
        url: normalize_url(&rec.scheme, &rec.host, rec.port, &rec.path, &rec.query),
        started_ms: rec.started_at_ms,
        body: body_hash(rec),
        device,
    }
}

/// Maps a client address to the one device behind it, if exactly one is.
pub trait DeviceLookup: Send + Sync + 'static {
    fn device_for_ip(&self, ip: &str) -> Option<String>;
}

#[derive(Default)]
struct Pools {
    sdk: VecDeque<Candidate>,
    proxy: VecDeque<Candidate>,
}

pub struct Merger {
    pools: Mutex<Pools>,
    lookup: Arc<dyn DeviceLookup>,
    sink: Arc<dyn FlowSink>,
    store: Store,
}

impl Merger {
    pub fn new(lookup: Arc<dyn DeviceLookup>, sink: Arc<dyn FlowSink>, store: Store) -> Merger {
        Merger {
            pools: Mutex::new(Pools::default()),
            lookup,
            sink,
            store,
        }
    }

    /// A flow finished. Call for both SDK and proxy flows; tunnels and
    /// replays are ignored.
    pub fn observe(&self, rec: &Arc<FlowRecord>) {
        let is_sdk = match rec.source {
            Source::Sdk => true,
            Source::Proxy => {
                if rec.flags & (flow::flags::TUNNEL | flow::flags::REPLAYED) != 0 || rec.method == "CONNECT" {
                    return;
                }
                false
            }
            Source::Merged => return,
        };
        let device = if is_sdk {
            rec.sdk.as_ref().map(|i| i.device_id.clone())
        } else {
            self.lookup.device_for_ip(&rec.client_ip)
        };
        let cand = candidate(rec, device);
        let pair = {
            let mut guard = self.pools.lock().unwrap();
            let p = &mut *guard;
            let (mine, other) = if is_sdk {
                (&mut p.sdk, &mut p.proxy)
            } else {
                (&mut p.proxy, &mut p.sdk)
            };
            let snapshot: Vec<Candidate> = other.iter().cloned().collect();
            match best_match(&cand, &snapshot, is_sdk).map(|c| c.id) {
                Some(partner) => {
                    other.retain(|c| c.id != partner);
                    Some(if is_sdk { (cand.id, partner) } else { (partner, cand.id) })
                }
                None => {
                    mine.push_back(cand);
                    while mine.len() > KEEP {
                        mine.pop_front();
                    }
                    None
                }
            }
        };
        if let Some((sdk_id, proxy_id)) = pair {
            self.link(sdk_id, proxy_id, if is_sdk { Some(rec) } else { None });
        }
    }

    fn link(&self, sdk_id: FlowId, proxy_id: FlowId, sdk_rec: Option<&Arc<FlowRecord>>) {
        self.store.set_merge(sdk_id, proxy_id);
        self.sink.publish(FlowEvent::Removed(proxy_id));
        // Re-announce the SDK row as merged so live subscribers update it.
        let rec = match sdk_rec {
            Some(r) => Some(r.clone()),
            None => self.store.get(sdk_id).ok().flatten(),
        };
        if let Some(r) = rec {
            let mut s = r.summary();
            s.source = Source::Merged;
            s.merged_with = proxy_id;
            self.sink.publish(FlowEvent::Updated(s));
        }
    }

    pub fn pending(&self) -> (usize, usize) {
        let p = self.pools.lock().unwrap();
        (p.sdk.len(), p.proxy.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: u64, method: &str, url: &str, at: i64, device: Option<&str>) -> Candidate {
        Candidate {
            id,
            method: method.into(),
            url: url.into(),
            started_ms: at,
            body: None,
            device: device.map(String::from),
        }
    }

    #[test]
    fn url_normalization() {
        assert_eq!(
            normalize_url("HTTPS", "API.Test", 443, "/a", "b=2&a=1"),
            "https://api.test/a?a=1&b=2"
        );
        assert_eq!(normalize_url("http", "x.test", 8080, "", ""), "http://x.test:8080/");
        assert_ne!(
            normalize_url("https", "x.test", 443, "/a", "q=1"),
            normalize_url("https", "x.test", 443, "/a", "q=2")
        );
    }

    #[test]
    fn exact_match() {
        let s = cand(1, "GET", "https://a.test/x", 1000, Some("d1"));
        let p = cand(2, "get", "https://a.test/x", 1000, Some("d1"));
        assert_eq!(score(&s, &p), Some(0));
    }

    #[test]
    fn close_timing_matches_and_far_timing_does_not() {
        let s = cand(1, "GET", "https://a.test/x", 1000, Some("d1"));
        assert_eq!(score(&s, &cand(2, "GET", "https://a.test/x", 1400, Some("d1"))), Some(400));
        assert_eq!(score(&s, &cand(2, "GET", "https://a.test/x", 1501, Some("d1"))), None);
        assert_eq!(score(&s, &cand(2, "GET", "https://a.test/x", 499, Some("d1"))), None);
    }

    #[test]
    fn same_url_different_bodies_do_not_merge() {
        let mut s = cand(1, "POST", "https://a.test/x", 1000, Some("d1"));
        let mut p = cand(2, "POST", "https://a.test/x", 1000, Some("d1"));
        s.body = Some(1);
        p.body = Some(2);
        assert_eq!(score(&s, &p), None);
        p.body = Some(1);
        assert_eq!(score(&s, &p), Some(0));
        // A side without a comparable body does not block the match.
        p.body = None;
        assert_eq!(score(&s, &p), Some(0));
    }

    #[test]
    fn redirects_only_merge_the_first_hop() {
        let sdk = cand(1, "GET", "https://a.test/old", 1000, Some("d1"));
        let hop1 = cand(2, "GET", "https://a.test/old", 1005, Some("d1"));
        let hop2 = cand(3, "GET", "https://a.test/new", 1030, Some("d1"));
        let pool = vec![hop1, hop2];
        assert_eq!(best_match(&sdk, &pool, true).unwrap().id, 2);
    }

    #[test]
    fn retries_merge_one_and_leave_the_rest() {
        let sdk = cand(1, "GET", "https://a.test/x", 1000, Some("d1"));
        let first = cand(2, "GET", "https://a.test/x", 1002, Some("d1"));
        let retry = cand(3, "GET", "https://a.test/x", 1250, Some("d1"));
        let pool = vec![retry.clone(), first.clone()];
        assert_eq!(best_match(&sdk, &pool, true).unwrap().id, 2, "closest start wins");
        let rest = vec![retry];
        assert_eq!(best_match(&sdk, &rest, true).unwrap().id, 3);
    }

    #[test]
    fn clock_skew_is_the_callers_job_but_a_skewed_pair_fails() {
        // The core corrects device times with the session offset before
        // building candidates; an uncorrected 2 s skew must not match.
        let s = cand(1, "GET", "https://a.test/x", 3000, Some("d1"));
        let p = cand(2, "GET", "https://a.test/x", 1000, Some("d1"));
        assert_eq!(score(&s, &p), None);
    }

    #[test]
    fn unknown_or_different_device_never_merges() {
        let s = cand(1, "GET", "https://a.test/x", 1000, Some("d1"));
        assert_eq!(score(&s, &cand(2, "GET", "https://a.test/x", 1000, None)), None);
        assert_eq!(score(&s, &cand(2, "GET", "https://a.test/x", 1000, Some("d2"))), None);
        let none = cand(1, "GET", "https://a.test/x", 1000, None);
        assert_eq!(score(&none, &cand(2, "GET", "https://a.test/x", 1000, None)), None);
    }

    #[test]
    fn different_method_or_query_does_not_merge() {
        let s = cand(1, "GET", "https://a.test/x?q=1", 1000, Some("d1"));
        assert_eq!(score(&s, &cand(2, "POST", "https://a.test/x?q=1", 1000, Some("d1"))), None);
        assert_eq!(score(&s, &cand(2, "GET", "https://a.test/x?q=2", 1000, Some("d1"))), None);
    }

    struct Fixed(Option<String>);
    impl DeviceLookup for Fixed {
        fn device_for_ip(&self, _: &str) -> Option<String> {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct Collect(Mutex<Vec<String>>);
    impl FlowSink for Collect {
        fn publish(&self, e: FlowEvent) {
            self.0.lock().unwrap().push(match e {
                FlowEvent::Removed(id) => format!("removed {id}"),
                FlowEvent::Updated(s) => format!("updated {} {:?} {}", s.id, s.source, s.merged_with),
                _ => "other".into(),
            });
        }
    }

    fn rec(id: u64, source: Source, device: &str, at: i64) -> Arc<FlowRecord> {
        Arc::new(FlowRecord {
            id,
            started_at_ms: at,
            scheme: "https".into(),
            host: "a.test".into(),
            port: 443,
            method: "GET".into(),
            path: "/x".into(),
            source,
            client_ip: "10.0.0.5".into(),
            sdk: (source == Source::Sdk).then(|| {
                Arc::new(flow::SdkInfo {
                    device_id: device.into(),
                    ..Default::default()
                })
            }),
            ..Default::default()
        })
    }

    #[test]
    fn merger_links_in_either_order_and_hides_the_wire_half() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let m = Merger::new(Arc::new(Fixed(Some("d1".into()))), sink.clone(), Store::open(dir.path()).unwrap());
        // SDK first, then proxy.
        m.observe(&rec(1, Source::Sdk, "d1", 1000));
        assert_eq!(m.pending(), (1, 0));
        m.observe(&rec(2, Source::Proxy, "", 1010));
        assert_eq!(m.pending(), (0, 0));
        // Proxy first, then SDK.
        m.observe(&rec(4, Source::Proxy, "", 5000));
        m.observe(&rec(3, Source::Sdk, "d1", 5003));
        let log = sink.0.lock().unwrap().clone();
        assert!(log.contains(&"removed 2".to_string()), "{log:?}");
        assert!(log.iter().any(|l| l.starts_with("updated 3 Merged 4")), "{log:?}");
    }

    #[test]
    fn merger_ignores_proxy_flows_it_cannot_attribute() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(Collect::default());
        let m = Merger::new(Arc::new(Fixed(None)), sink.clone(), Store::open(dir.path()).unwrap());
        m.observe(&rec(1, Source::Sdk, "d1", 1000));
        m.observe(&rec(2, Source::Proxy, "", 1000));
        assert!(sink.0.lock().unwrap().is_empty());
        assert_eq!(m.pending(), (1, 1));
    }
}
