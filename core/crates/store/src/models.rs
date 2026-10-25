use serde_json::Value;
fn quote(s: &str, lang: &str) -> String {
    let s = serde_json::to_string(s).unwrap();
    if lang == "kotlin" || lang == "dart" {
        s.replace('$', "\\$")
    } else {
        s
    }
}
fn swift_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
fn ty(v: &Value, name: &str, lang: &str, out: &mut String, depth: usize) -> String {
    if depth > 8 {
        return match lang {
            "typescript" => "unknown",
            "dart" => "dynamic",
            "kotlin" => "JsonElement",
            _ => "JSONValue",
        }
        .into();
    }
    match v {
        Value::Object(m) if m.len() <= 200 => {
            emit(name, m, lang, out, depth + 1);
            name.into()
        }
        Value::Array(a) => {
            let t = if let Some(first) = a.first().filter(|first| {
                a.iter()
                    .all(|x| std::mem::discriminant(x) == std::mem::discriminant(*first))
            }) {
                ty(first, &format!("{name}Item"), lang, out, depth + 1)
            } else {
                match lang {
                    "typescript" => "unknown",
                    "dart" => "dynamic",
                    "kotlin" => "JsonElement",
                    _ => "JSONValue",
                }
                .into()
            };
            match lang {
                "typescript" => format!("Array<{t}>"),
                "dart" | "kotlin" => format!("List<{t}>"),
                _ => format!("[{t}]"),
            }
        }
        Value::String(_) => match lang {
            "typescript" => "string",
            _ => "String",
        }
        .into(),
        Value::Bool(_) => match lang {
            "typescript" => "boolean",
            "dart" => "bool",
            "kotlin" => "Boolean",
            _ => "Bool",
        }
        .into(),
        Value::Number(n) => match lang {
            "typescript" => "number",
            "dart" => {
                if n.is_i64() {
                    "int"
                } else {
                    "double"
                }
            }
            "kotlin" => {
                if n.is_i64() {
                    "Long"
                } else {
                    "Double"
                }
            }
            _ => {
                if n.is_i64() {
                    "Int64"
                } else {
                    "Double"
                }
            }
        }
        .into(),
        _ => match lang {
            "typescript" => "unknown",
            "dart" => "dynamic",
            "kotlin" => "JsonElement",
            _ => "JSONValue",
        }
        .into(),
    }
}
fn dart_read(v: &Value, t: &str, e: &str) -> String {
    match v {
        Value::Object(_) if t != "dynamic" => {
            format!("{t}.fromJson(Map<String, dynamic>.from({e} as Map))")
        }
        Value::Array(a) if t.starts_with("List<") => {
            let inner = &t[5..t.len() - 1];
            let item = a
                .first()
                .map(|v| dart_read(v, inner, "item"))
                .unwrap_or("item".into());
            format!("({e} as List).map((item) => {item}).toList()")
        }
        Value::Number(n) => format!(
            "({e} as num).{}()",
            if n.is_i64() { "toInt" } else { "toDouble" }
        ),
        Value::String(_) => format!("{e} as String"),
        Value::Bool(_) => format!("{e} as bool"),
        _ => e.into(),
    }
}
fn emit(
    name: &str,
    m: &serde_json::Map<String, Value>,
    lang: &str,
    out: &mut String,
    depth: usize,
) {
    let fields: Vec<_> = m
        .iter()
        .enumerate()
        .map(|(i, (key, v))| {
            let t = ty(v, &format!("{name}Field{i}"), lang, out, depth);
            (key, format!("field{i}"), t)
        })
        .collect();
    match lang {
        "typescript" => {
            out.push_str(&format!("export interface {name} {{\n"));
            for (key, _, t) in fields {
                out.push_str(&format!("  {}?: {t};\n", quote(key, lang)));
            }
            out.push_str("}\n\n");
        }
        "kotlin" => {
            if fields.is_empty() {
                out.push_str(&format!("@Serializable\nclass {name}\n\n"));
                return;
            }
            out.push_str(&format!("@Serializable\ndata class {name}(\n"));
            for (key, f, t) in fields {
                out.push_str(&format!(
                    "  @SerialName({}) val {f}: {t}? = null,\n",
                    quote(key, lang)
                ));
            }
            out.push_str(")\n\n");
        }
        "swift" => {
            out.push_str(&format!("struct {name}: Codable {{\n"));
            for (_, f, t) in &fields {
                out.push_str(&format!("  var {f}: {t}?\n"));
            }
            if !fields.is_empty() {
                out.push_str("  enum CodingKeys: String, CodingKey {\n");
                for (key, f, _) in fields {
                    out.push_str(&format!("    case {f} = {}\n", swift_quote(key)));
                }
                out.push_str("  }\n");
            }
            out.push_str("}\n\n");
        }
        _ => {
            out.push_str(&format!("class {name} {{\n"));
            for (key, f, t) in &fields {
                out.push_str(&format!(
                    "  // JSON key: {}\n  final {t}{} {f};\n",
                    quote(key, lang),
                    if t == "dynamic" { "" } else { "?" }
                ));
            }
            if fields.is_empty() {
                out.push_str(&format!("  {name}();\n"));
            } else {
                out.push_str(&format!(
                    "  {name}({{{}}});\n",
                    fields
                        .iter()
                        .map(|(_, f, _)| format!("this.{f}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            out.push_str(&format!(
                "  factory {name}.fromJson(Map<String, dynamic> json) => {name}(\n"
            ));
            for (i, (key, v)) in m.iter().enumerate() {
                let (_, f, t) = &fields[i];
                let e = format!("json[{}]", quote(key, lang));
                out.push_str(&format!(
                    "    {f}: {e} == null ? null : {},\n",
                    dart_read(v, t, &e)
                ));
            }
            out.push_str("  );\n");
            out.push_str("}\n\n");
        }
    }
}
pub fn models(lang: &str, entries: &[Value]) -> (String, &'static str) {
    let (mut out,ext)=match lang {
        "typescript"=>(String::from("// Inferred from observed JSON; all fields optional.\n"),"ts"),
        "kotlin"=>(String::from("import kotlinx.serialization.*\nimport kotlinx.serialization.json.JsonElement\n\n"),"kt"),
        "swift"=>(String::from("import Foundation\nindirect enum JSONValue: Codable {\n  case string(String), number(Double), bool(Bool), array([JSONValue]), object([String: JSONValue]), null\n  init(from decoder: Decoder) throws {\n    let c = try decoder.singleValueContainer()\n    if c.decodeNil() { self = .null }\n    else if let v = try? c.decode(Bool.self) { self = .bool(v) }\n    else if let v = try? c.decode(Double.self) { self = .number(v) }\n    else if let v = try? c.decode(String.self) { self = .string(v) }\n    else if let v = try? c.decode([JSONValue].self) { self = .array(v) }\n    else { self = .object(try c.decode([String: JSONValue].self)) }\n  }\n  func encode(to encoder: Encoder) throws {\n    var c = encoder.singleValueContainer()\n    switch self {\n    case .null: try c.encodeNil()\n    case .string(let v): try c.encode(v)\n    case .number(let v): try c.encode(v)\n    case .bool(let v): try c.encode(v)\n    case .array(let v): try c.encode(v)\n    case .object(let v): try c.encode(v)\n    }\n  }\n}\n\n"),"swift"),
        _=>(String::from("// Inferred models. JSON keys are documented on each field.\n"),"dart"),
    };
    for (i, e) in entries.iter().enumerate() {
        for (side, body) in [
            ("Request", &e["request"]["postData"]),
            ("Response", &e["response"]["content"]),
        ] {
            if let Some(text) = body["text"].as_str().filter(|s| s.len() <= 256 * 1024) {
                if let Ok(Value::Object(m)) = serde_json::from_str::<Value>(text) {
                    emit(&format!("Flow{}{side}", i + 1), &m, lang, &mut out, 0);
                }
            }
        }
    }
    (out, ext)
}
