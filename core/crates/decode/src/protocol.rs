//! Best-effort protocol inspection. No schemas, execution, or network access.
#[derive(Clone, Debug)]
pub struct DecodedField { pub depth: u32, pub label: String, pub value: String }
#[derive(Clone, Debug, Default)]
pub struct ProtocolView { pub title: String, pub fields: Vec<DecodedField>, pub note: String }
const MAX_FIELDS: usize = 2000;
const MAX_BYTES: usize = 256 * 1024;
fn short(s: &str) -> String { s.chars().take(4096).collect() }
fn row(out: &mut Vec<DecodedField>, depth: u32, label: impl Into<String>, value: String) {
    if out.len() < MAX_FIELDS { out.push(DecodedField { depth, label: label.into(), value }); }
}
fn varint(input: &mut &[u8]) -> Result<u64, &'static str> {
    let mut n = 0;
    for i in 0..10 {
        let (&b, rest) = input.split_first().ok_or("truncated varint")?; *input = rest;
        if i == 9 && b > 1 { return Err("varint overflow"); }
        n |= ((b & 127) as u64) << (i * 7);
        if b & 128 == 0 { return Ok(n); }
    }
    Err("invalid varint")
}
fn take<'a>(b: &mut &'a [u8], n: usize) -> Result<&'a [u8], &'static str> {
    if n > b.len() { return Err("truncated field"); }
    let (v, rest) = b.split_at(n); *b = rest; Ok(v)
}
fn protobuf(mut b: &[u8], depth: u32, out: &mut Vec<DecodedField>) -> Result<(), &'static str> {
    if depth > 8 { return Err("protobuf nesting limit"); }
    while !b.is_empty() {
        if out.len() >= MAX_FIELDS { return Err("field limit reached"); }
        let key = varint(&mut b)?;
        let field = key >> 3;
        if field == 0 || field > 0x1fffffff { return Err("invalid field number"); }
        let label = format!("Field {field}");
        match key & 7 {
            0 => row(out, depth, label, format!("{} (varint)", varint(&mut b)?)),
            1 => { let v = u64::from_le_bytes(take(&mut b, 8)?.try_into().unwrap()); row(out, depth, label, format!("{v} (fixed64; double {})", f64::from_bits(v))); }
            5 => { let v = u32::from_le_bytes(take(&mut b, 4)?.try_into().unwrap()); row(out, depth, label, format!("{v} (fixed32; float {})", f32::from_bits(v))); }
            2 => {
                let n = usize::try_from(varint(&mut b)?).map_err(|_| "length overflow")?;
                let data = take(&mut b, n)?;
                if let Ok(text) = std::str::from_utf8(data) {
                    if text.chars().all(|c| !c.is_control() || c == '\n' || c == '\t') {
                        row(out, depth, label, short(text)); continue;
                    }
                }
                let start = out.len();
                row(out, depth, label, format!("{n} bytes"));
                if data.is_empty() || protobuf(data, depth + 1, out).is_err() {
                    out.truncate(start + 1);
                    out[start].value = format!("{n} bytes: {}{}", data.iter().take(64).map(|b| format!("{b:02x}")).collect::<String>(), if n > 64 {"…"} else {""});
                }
            }
            _ => return Err("unsupported protobuf wire type"),
        }
    }
    Ok(())
}

pub fn protocol_view(content_type: &str, body: &[u8]) -> ProtocolView {
    let ct = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let mut v = ProtocolView::default();
    let limited = &body[..body.len().min(MAX_BYTES)];
    if ct.starts_with("application/grpc") && !ct.starts_with("application/grpc-web") {
        v.title = "gRPC messages (schema unknown)".into();
        let mut b = limited; let mut index = 0;
        while !b.is_empty() && v.fields.len() < MAX_FIELDS {
            if b.len() < 5 { v.note = "Incomplete gRPC frame header".into(); break; }
            let compressed = b[0];
            let len = u32::from_be_bytes(b[1..5].try_into().unwrap()) as usize;
            b = &b[5..]; index += 1;
            if len > b.len() { v.note = "Incomplete or oversized gRPC frame".into(); break; }
            let msg = &b[..len]; b = &b[len..];
            row(&mut v.fields, 0, format!("Message {index}"), format!("{len} bytes"));
            if compressed == 0 {
                if let Err(e) = protobuf(msg, 1, &mut v.fields) { v.note = e.into(); }
            } else { v.note = "Compressed gRPC messages require a schema/encoding-aware decoder".into(); }
        }
    } else if ct == "application/protobuf" || ct == "application/x-protobuf" || ct == "application/vnd.google.protobuf" {
        v.title = "Protobuf fields (schema unknown)".into();
        if let Err(e) = protobuf(limited, 0, &mut v.fields) { v.note = e.into(); }
    } else if ct == "application/graphql" {
        v.title = "GraphQL document".into();
        row(&mut v.fields, 0, "query", short(&String::from_utf8_lossy(limited)));
    } else if (ct.contains("json") || ct.is_empty()) && !limited.is_empty() {
        if let Ok(doc) = serde_json::from_slice::<serde_json::Value>(limited) {
            let docs: Vec<_> = if let Some(batch) = doc.as_array() { batch.iter().take(100).collect() } else { vec![&doc] };
            for (i, doc) in docs.into_iter().enumerate() {
                if let Some(query) = doc["query"].as_str() {
                    v.title = "GraphQL request".into();
                    row(&mut v.fields, 0, format!("Operation {}", i + 1), doc["operationName"].as_str().unwrap_or("anonymous").into());
                    row(&mut v.fields, 1, "query", short(query));
                    if let Some(vars) = doc.get("variables") { row(&mut v.fields, 1, "variables", short(&serde_json::to_string_pretty(vars).unwrap_or_default())); }
                }
            }
        }
    }
    if !v.title.is_empty() && (body.len() > MAX_BYTES || v.fields.len() >= MAX_FIELDS) { v.note.push_str(" Preview limit reached."); }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn numbered_fields_and_grpc_framing() {
        let msg = [8, 150, 1, 18, 2, b'h', b'i'];
        let p = protocol_view("application/protobuf", &msg);
        assert_eq!(p.fields[0].label, "Field 1"); assert!(p.fields[0].value.starts_with("150")); assert_eq!(p.fields[1].value, "hi");
        let mut frame = vec![0,0,0,0,7]; frame.extend(msg);
        assert_eq!(protocol_view("application/grpc+proto", &frame).fields.len(), 3);
        frame.pop(); assert!(!protocol_view("application/grpc", &frame).note.is_empty());
    }
    #[test]
    fn malformed_and_bounded() {
        for n in 0..256 { let b = vec![n as u8; 200]; let p = protocol_view("application/protobuf", &b); assert!(p.fields.len() <= MAX_FIELDS); }
        assert!(!protocol_view("application/protobuf", &[8,255,255,255,255,255,255,255,255,255,255]).note.is_empty());
    }
    #[test]
    fn graphql_json_and_document() {
        let p = protocol_view("application/json", br#"{"query":"query Q { users { id } }","operationName":"Q","variables":{"n":2}}"#);
        assert_eq!(p.title, "GraphQL request"); assert_eq!(p.fields[0].value, "Q");
        assert_eq!(protocol_view("application/graphql", b"{ users { id } }").fields.len(), 1);
    }
}
