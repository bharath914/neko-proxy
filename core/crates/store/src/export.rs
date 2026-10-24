//! All generated artifacts begin with the same redacted HAR representation.
use flow::FlowRecord;
use serde_json::{json, Map, Value};
use std::sync::Arc;
const LIMIT: usize = 64 * 1024 * 1024;
pub const EXPORT_KINDS: &[&str] = &[
    "har",
    "postman",
    "k6",
    "wiremock",
    "msw",
    "mockoon",
    "openapi",
    "dart",
    "kotlin",
    "swift",
    "typescript",
    "dio",
];
pub struct Artifact {
    pub data: Vec<u8>,
    pub extension: &'static str,
    pub count: u32,
}
fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn q(v: &str) -> String {
    serde_json::to_string(v).unwrap()
}
fn headers(v: &Value) -> Value {
    let mut out = Map::new();
    for h in v.as_array().into_iter().flatten() {
        let name = s(h, "name");
        if [
            "content-length",
            "content-encoding",
            "transfer-encoding",
            "connection",
        ]
        .iter()
        .any(|n| name.eq_ignore_ascii_case(n))
        {
            continue;
        }
        out.insert(name.into(), h["value"].clone());
    }
    Value::Object(out)
}
fn uuid(n: usize) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn schema(v: &Value, depth: usize) -> Value {
    if depth > 8 {
        return json!({});
    }
    match v {
        Value::Object(m) => {
            json!({"type":"object","properties":m.iter().take(200).map(|(k,v)| (k.clone(), schema(v,depth+1))).collect::<Map<_,_>>() })
        }
        Value::Array(a) => {
            json!({"type":"array","items":a.first().map(|v|schema(v,depth+1)).unwrap_or(json!({}))})
        }
        Value::Bool(_) => json!({"type":"boolean"}),
        Value::Number(n) => json!({"type":if n.is_i64()||n.is_u64(){"integer"}else{"number"}}),
        Value::Null => json!({"nullable":true}),
        _ => json!({"type":"string"}),
    }
}
pub fn export(kind: &str, records: &[Arc<FlowRecord>], version: &str) -> Result<Artifact, String> {
    let kind = if kind.is_empty() { "har" } else { kind };
    if !EXPORT_KINDS.contains(&kind) {
        return Err(format!("unknown export kind {kind}"));
    }
    if records.len() > 10_000 {
        return Err("export limited to 10,000 flows".into());
    }
    let total = records
        .iter()
        .try_fold(0usize, |sum, r| {
            sum.checked_add(r.req_body.len())?
                .checked_add(r.res_body.len())
        })
        .ok_or("export size overflow")?;
    if total > LIMIT / 2 {
        return Err("export source bodies exceed 32 MiB; select fewer flows".into());
    }
    let mut har = crate::to_har(records, true, version);
    // Redact token-shaped URL/path/header values too, not only query/body secrets.
    har = serde_json::from_str(&decode::redact::redact_json_text(&har.to_string()))
        .map_err(|e| e.to_string())?;
    let entries = har["log"]["entries"].as_array().ok_or("missing entries")?;
    let mut text = String::new();
    let mut extension = "json";
    let value = match kind {
        "har" => {
            extension = "har";
            har.clone()
        }
        "postman" => {
            json!({"info":{"name":"Neko captured traffic","schema":"https://schema.getpostman.com/json/collection/v2.1.0/collection.json"},"item":entries.iter().map(|e| {
            let r=&e["request"]; json!({"name":format!("{} {}",s(r,"method"),s(r,"url")),"request":{"method":r["method"],"url":r["url"],"header":headers(&r["headers"]).as_object().unwrap().iter().map(|(k,v)|json!({"key":k,"value":v})).collect::<Vec<_>>(),"body":{"mode":"raw","raw":s(&r["postData"],"text")}}})
        }).collect::<Vec<_>>() })
        }
        "wiremock" => json!({"mappings":entries.iter().filter_map(|e| {
            let r=&e["request"]; let res=&e["response"];
            let uri: http::Uri=s(r,"url").parse().ok()?;
            Some(json!({"request":{"method":r["method"],"url":uri.path_and_query().map(|p|p.as_str()).unwrap_or("/")},"response":{"status":res["status"].as_u64().filter(|n|*n>=100).unwrap_or(502),"headers":headers(&res["headers"]),"body":s(&res["content"],"text")}}))
        }).collect::<Vec<_>>() }),
        "mockoon" => {
            let routes:Vec<_>=entries.iter().enumerate().filter_map(|(i,e)| {
                let r=&e["request"]; let res=&e["response"]; let uri:http::Uri=s(r,"url").parse().ok()?;
                Some(json!({"uuid":uuid(i*2+2),"type":"http","documentation":"Captured by Neko; redacted","method":s(r,"method").to_ascii_lowercase(),"endpoint":uri.path().trim_start_matches('/'),"responseMode":null,"streamingMode":null,"streamingInterval":0,"responses":[{"uuid":uuid(i*2+3),"rules":[],"rulesOperator":"OR","statusCode":res["status"].as_u64().filter(|n|*n>=100).unwrap_or(502),"label":"Captured response","headers":res["headers"].as_array().into_iter().flatten().filter(|h|!s(h,"name").eq_ignore_ascii_case("content-length")&&!s(h,"name").eq_ignore_ascii_case("content-encoding")).map(|h|json!({"key":h["name"],"value":h["value"]})).collect::<Vec<_>>(),"body":s(&res["content"],"text"),"latency":0,"bodyType":"INLINE","filePath":"","databucketID":"","sendFileAsBody":false,"disableTemplating":true,"fallbackTo404":false,"default":true,"crudKey":"id","callbacks":[]}]}))
            }).collect();
            json!({"uuid":uuid(1),"lastMigration":33,"name":"Neko traffic","port":3000,"hostname":"127.0.0.1","endpointPrefix":"","latency":0,"folders":[],"rootChildren":routes.iter().map(|r|json!({"type":"route","uuid":r["uuid"]})).collect::<Vec<_>>(),"routes":routes,"proxyMode":false,"proxyRemovePrefix":false,"proxyHost":"","proxyReqHeaders":[],"proxyResHeaders":[],"tlsOptions":{"enabled":false,"type":"CERT","pfxPath":"","certPath":"","keyPath":"","caPath":"","passphrase":""},"cors":true,"headers":[],"data":[],"callbacks":[]})
        }
        "openapi" => {
            let mut paths = Map::new();
            let mut servers = std::collections::BTreeSet::new();
            for e in entries {
                let r = &e["request"];
                let res = &e["response"];
                let Ok(uri) = s(r, "url").parse::<http::Uri>() else {
                    continue;
                };
                let method = s(r, "method").to_ascii_lowercase();
                if ![
                    "get", "post", "put", "patch", "delete", "head", "options", "trace",
                ]
                .contains(&method.as_str())
                {
                    continue;
                }
                if let (Some(scheme), Some(host)) = (uri.scheme_str(), uri.authority()) {
                    servers.insert(format!("{scheme}://{host}"));
                }
                let path = paths.entry(uri.path().to_string()).or_insert(json!({}));
                if path.get(&method).is_none() {
                    path[&method] = json!({"responses":{},"parameters":[]});
                }
                let op = &mut path[&method];
                let request_text = s(&r["postData"], "text");
                if !request_text.is_empty() {
                    let mime = s(&r["postData"], "mimeType")
                        .split(';')
                        .next()
                        .unwrap_or("");
                    let example = serde_json::from_str::<Value>(request_text)
                        .unwrap_or(Value::String(request_text.into()));
                    op["requestBody"] = json!({"required":false,"content":{if mime.is_empty(){"application/octet-stream"}else{mime}:{"schema":schema(&example,0),"example":example}}});
                }

                let ct = s(&res["content"], "mimeType")
                    .split(';')
                    .next()
                    .unwrap_or("");
                let example = serde_json::from_str::<Value>(s(&res["content"], "text"))
                    .unwrap_or(Value::String(s(&res["content"], "text").into()));
                let status = res["status"]
                    .as_u64()
                    .filter(|n| *n >= 100)
                    .map(|n| n.to_string())
                    .unwrap_or("default".into());
                op["responses"][status] = json!({"description":"Observed response","content":{if ct.is_empty(){"application/octet-stream"}else{ct}:{"schema":schema(&example,0),"example":example}}});
                let mut seen = std::collections::HashSet::new();
                op["parameters"]=json!(r["queryString"].as_array().into_iter().flatten().filter(|p|seen.insert(s(p,"name").to_owned())).map(|p|json!({"name":p["name"],"in":"query","required":false,"schema":{"type":"string"}})).collect::<Vec<_>>());
            }
            json!({"openapi":"3.0.3","info":{"title":"Observed Neko API","version":"0.1.0"},"servers":servers.into_iter().map(|url|json!({"url":url})).collect::<Vec<_>>(),"paths":paths})
        }
        "k6" => {
            extension = "js";
            let requests:Vec<_>=entries.iter().map(|e|json!({"method":e["request"]["method"],"url":e["request"]["url"],"body":s(&e["request"]["postData"],"text"),"headers":headers(&e["request"]["headers"])})).collect();
            text=format!("import http from 'k6/http';\nconst requests = {};\nexport default function () {{\n  for (const r of requests) http.request(r.method, r.url, r.body || null, {{headers:r.headers}});\n}}\n",json!(requests));
            Value::Null
        }
        "msw" => {
            extension = "js";
            text = "import { http, HttpResponse } from 'msw/http';\nexport const handlers = [\n"
                .into();
            for e in entries {
                let r = &e["request"];
                let res = &e["response"];
                let method = s(r, "method").to_ascii_lowercase();
                if !["get", "post", "put", "patch", "delete", "head", "options"]
                    .contains(&method.as_str())
                {
                    continue;
                }
                let url = s(r, "url").split('?').next().unwrap_or("");
                let body = if ["HEAD"].contains(&s(r, "method"))
                    || [204, 205, 304].contains(&res["status"].as_u64().unwrap_or(0))
                {
                    "null".into()
                } else {
                    q(s(&res["content"], "text"))
                };
                text.push_str(&format!("  http.{method}({}, () => new HttpResponse({body}, {{ status: {}, headers: {} }})),\n",q(url),res["status"].as_u64().filter(|n|*n>=200).unwrap_or(502),headers(&res["headers"])));
            }
            text.push_str("];\n");
            Value::Null
        }
        "dio" => {
            extension = "dart";
            text="import 'package:dio/dio.dart';\nFuture<void> replayTraffic() async {\n  final dio = Dio();\n".into();
            for e in entries {
                let r = &e["request"];
                let quote = |s: &str| q(s).replace('$', "\\$");
                text.push_str(&format!("  await dio.request({}, data: {}, options: Options(method: {}, headers: {}));\n",quote(s(r,"url")),quote(s(&r["postData"],"text")),quote(s(r,"method")),headers(&r["headers"]).to_string().replace('$',"\\$")));
            }
            text.push_str("}\n");
            Value::Null
        }
        "dart" | "kotlin" | "swift" | "typescript" => {
            let (source, ext) = super::models::models(kind, entries);
            text = source;
            extension = ext;
            Value::Null
        }
        _ => unreachable!(),
    };
    let data = if text.is_empty() {
        serde_json::to_vec_pretty(&value).map_err(|e| e.to_string())?
    } else {
        text.into_bytes()
    };
    if data.len() > LIMIT {
        return Err("generated artifact exceeds 64 MiB".into());
    }
    Ok(Artifact {
        data,
        extension,
        count: entries.len() as u32,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Vec<Arc<FlowRecord>> {
        vec![Arc::new(FlowRecord {
            id: 1,
            scheme: "https".into(),
            host: "api.example.com".into(),
            port: 443,
            method: "POST".into(),
            path: "/items".into(),
            query: "api_key=never-export-this".into(),
            status: 200,
            req_headers: vec![
                ("Authorization".into(), "Bearer never-export-this".into()),
                ("content-type".into(), "application/json".into()),
            ],
            req_body: bytes::Bytes::from_static(br#"{"password":"never-export-this","ok":true}"#),
            res_headers: vec![("content-type".into(), "application/json".into())],
            res_body: bytes::Bytes::from_static(
                br#"{"id":1,"name":"A","nested":{"enabled":true},"tags":["x"]}"#,
            ),
            req_size: 50,
            res_size: 60,
            ..Default::default()
        })]
    }
    #[test]
    fn every_artifact_redacts_and_has_structure() {
        for kind in EXPORT_KINDS {
            let a = export(kind, &sample(), "test").unwrap();
            let text = std::str::from_utf8(&a.data).unwrap();
            assert!(!text.contains("never-export-this"), "{kind}: {text}");
            assert!(!text.is_empty());
            if ["har", "postman", "wiremock", "mockoon", "openapi"].contains(kind) {
                serde_json::from_slice::<Value>(&a.data).unwrap();
            }
            if kind == &"mockoon" {
                let v: Value = serde_json::from_slice(&a.data).unwrap();
                assert_eq!(v["routes"][0]["responses"][0]["disableTemplating"], true);
            }
        }
    }
    #[test]
    fn binary_is_omitted_and_unknown_kind_rejected() {
        let mut r = (*sample()[0]).clone();
        r.res_body = bytes::Bytes::from_static(b"\0token-secret");
        r.res_headers.clear();
        let a = export("har", &[Arc::new(r)], "test").unwrap();
        let text = String::from_utf8(a.data).unwrap();
        assert!(text.contains("binary body omitted"));
        assert!(!text.contains("token-secret"));
        assert!(export("bogus", &[], "").is_err());
    }
    #[test]
    fn generated_fixtures() {
        if let Ok(dir) = std::env::var("NEKO_CODEGEN_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            for kind in EXPORT_KINDS {
                let a = export(kind, &sample(), "test").unwrap();
                std::fs::write(format!("{dir}/{kind}.{}", a.extension), a.data).unwrap();
            }
        }
    }
}
