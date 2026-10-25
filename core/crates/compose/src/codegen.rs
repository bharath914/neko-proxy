//! Code snippets for a built request.
//!
//! Each generator writes a complete example for one language or tool: the
//! method, the URL, every header and the body exactly as built. A form-data body
//! uses the language's own multipart support where that is easy, so the library
//! picks the boundary (and the generated `Content-Type` header is left out).

use crate::Built;

const LANGUAGES: &[(&str, &str)] = &[
    ("curl", "cURL"),
    ("fetch", "JavaScript fetch"),
    ("axios", "JavaScript axios"),
    ("python", "Python requests"),
    ("urlsession", "Swift URLSession"),
    ("okhttp", "Kotlin OkHttp"),
    ("httpie", "HTTPie"),
    ("go", "Go net/http"),
    ("php", "PHP cURL"),
    ("ruby", "Ruby Net::HTTP"),
    ("csharp", "C# HttpClient"),
    ("java", "Java HttpClient"),
    ("powershell", "PowerShell Invoke-RestMethod"),
    ("wget", "Wget"),
];

/// `(id, label)` of every language, in menu order.
pub fn code_languages() -> &'static [(&'static str, &'static str)] {
    LANGUAGES
}

pub fn generate_code(lang: &str, b: &Built) -> Result<String, String> {
    Ok(match lang {
        "curl" => curl(b),
        "fetch" => fetch(b),
        "axios" => axios(b),
        "python" => python(b),
        "urlsession" => urlsession(b),
        "okhttp" => okhttp(b),
        "httpie" => httpie(b),
        "go" => go(b),
        "php" => php(b),
        "ruby" => ruby(b),
        "csharp" => csharp(b),
        "java" => java(b),
        "powershell" => powershell(b),
        "wget" => wget(b),
        _ => return Err(format!("no generator for {lang}")),
    })
}

// ---- shared helpers ----

fn is_multipart(b: &Built) -> bool {
    !b.form.is_empty()
}

/// The headers to write. A multipart body written with a library's own
/// multipart support leaves `Content-Type` out, since the boundary is the
/// library's to choose.
fn headers_for(b: &Built, drop_content_type: bool) -> Vec<(String, String)> {
    b.headers
        .iter()
        .filter(|(k, _)| !(drop_content_type && k.eq_ignore_ascii_case("content-type")))
        .cloned()
        .collect()
}

/// For a map that cannot repeat a name: join repeats with a comma.
fn merged(headers: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, v) in headers {
        match out.iter_mut().find(|(ek, _)| ek.eq_ignore_ascii_case(k)) {
            Some(e) => {
                e.1.push_str(", ");
                e.1.push_str(v);
            }
            None => out.push((k.clone(), v.clone())),
        }
    }
    out
}

fn body_text(b: &Built) -> String {
    String::from_utf8_lossy(&b.body).into_owned()
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn title_case(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut c = lower.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        None => String::new(),
    }
}

/// A double-quoted literal for the C family: escapes `\`, `"`, `\n`, `\r`,
/// `\t` and the other control characters through `ctrl`, and `$` when the
/// language interpolates it.
fn dq(s: &str, ctrl: fn(u32) -> String, dollar: bool) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '$' if dollar => o.push_str("\\$"),
            c if (c as u32) < 0x20 || c == '\u{7f}' => o.push_str(&ctrl(c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn ctrl_u4(c: u32) -> String {
    format!("\\u{c:04x}")
}

fn ctrl_x2(c: u32) -> String {
    format!("\\x{c:02x}")
}

fn ctrl_octal(c: u32) -> String {
    format!("\\{c:03o}")
}

fn ctrl_swift(c: u32) -> String {
    format!("\\u{{{c:x}}}")
}

/// Single quotes where only `\` and `'` are special (PHP, Ruby).
fn sq_backslash(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// A POSIX shell word.
fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// A PowerShell single-quoted string. PowerShell also treats the curly quotes
/// as quote characters, so they are doubled too.
fn ps(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('\'');
    for c in s.chars() {
        o.push(c);
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}') {
            o.push(c);
        }
    }
    o.push('\'');
    o
}

fn js_str(s: &str) -> String {
    dq(s, ctrl_u4, false)
}

/// A JS string for a body: a multi-line text reads better as a template
/// literal, as long as it holds nothing a template would change.
fn js_text(s: &str) -> String {
    let plain = s.contains('\n')
        && !s
            .chars()
            .any(|c| c == '\r' || (c.is_control() && c != '\n' && c != '\t'));
    if !plain {
        return js_str(s);
    }
    let mut o = String::from("`");
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => o.push_str("\\\\"),
            '`' => o.push_str("\\`"),
            '$' if it.peek() == Some(&'{') => o.push_str("\\$"),
            c => o.push(c),
        }
    }
    o.push('`');
    o
}

fn py_str(s: &str) -> String {
    dq(s, ctrl_x2, false)
}

/// A Python string; a multi-line text is written one line per literal so it
/// stays readable and exact.
fn py_text(s: &str, indent: &str) -> String {
    let lines: Vec<&str> = s.split_inclusive('\n').collect();
    if lines.len() < 2 {
        return py_str(s);
    }
    let mut o = String::from("(\n");
    for l in lines {
        o.push_str(&format!("{indent}    {}\n", py_str(l)));
    }
    o.push_str(indent);
    o.push(')');
    o
}

// ---- cURL ----

fn curl(b: &Built) -> String {
    if !is_multipart(b) {
        return crate::to_curl(b);
    }
    // curl builds the multipart body, boundary and Content-Type itself.
    let mut parts = vec!["curl".to_string()];
    if b.method != "POST" {
        parts.push(format!("-X {}", sh(&b.method)));
    }
    parts.push(sh(&b.url));
    for (k, v) in headers_for(b, true) {
        parts.push(format!("-H {}", sh(&format!("{k}: {v}"))));
    }
    for f in &b.form {
        parts.push(if f.file {
            if f.value.contains([';', ',', '"']) {
                let quoted = f.value.replace('\\', "\\\\").replace('"', "\\\"");
                format!("-F {}", sh(&format!("{}=@\"{quoted}\"", f.name)))
            } else {
                format!("-F {}", sh(&format!("{}=@{}", f.name, f.value)))
            }
        } else if f.value.starts_with(['@', '<', '"']) || f.value.contains(';') {
            format!("--form-string {}", sh(&format!("{}={}", f.name, f.value)))
        } else {
            format!("-F {}", sh(&format!("{}={}", f.name, f.value)))
        });
    }
    parts.join(" \\\n  ")
}

// ---- JavaScript ----

/// `FormData` set-up shared by fetch and axios, and whether it needs `openAsBlob`.
fn js_form(b: &Built) -> (bool, String) {
    let mut s = String::from("const form = new FormData();\n");
    let mut files = false;
    for f in &b.form {
        if f.file {
            files = true;
            s.push_str(&format!(
                "form.append({}, await openAsBlob({}), {});\n",
                js_str(&f.name),
                js_str(&f.value),
                js_str(base_name(&f.value))
            ));
        } else {
            s.push_str(&format!(
                "form.append({}, {});\n",
                js_str(&f.name),
                js_str(&f.value)
            ));
        }
    }
    (files, s)
}

fn js_headers(headers: &[(String, String)]) -> String {
    let mut s = String::from("  headers: {\n");
    for (k, v) in merged(headers) {
        s.push_str(&format!("    {}: {},\n", js_str(&k), js_str(&v)));
    }
    s.push_str("  },\n");
    s
}

fn fetch(b: &Built) -> String {
    let mp = is_multipart(b);
    let headers = headers_for(b, mp);
    let mut s = String::new();
    let mut form = String::new();
    if mp {
        let (files, f) = js_form(b);
        if files {
            s.push_str("import { openAsBlob } from \"node:fs\";\n\n");
        }
        form = f;
    }
    s.push_str(&form);
    if mp {
        s.push('\n');
    }
    s.push_str(&format!(
        "const response = await fetch({}, {{\n  method: {},\n",
        js_str(&b.url),
        js_str(&b.method)
    ));
    if !headers.is_empty() {
        s.push_str(&js_headers(&headers));
    }
    if mp {
        s.push_str("  body: form,\n");
    } else if !b.body.is_empty() {
        s.push_str(&format!("  body: {},\n", js_text(&body_text(b))));
    }
    s.push_str("});\n\nconsole.log(response.status);\nconsole.log(await response.text());\n");
    s
}

fn axios(b: &Built) -> String {
    let mp = is_multipart(b);
    let headers = headers_for(b, mp);
    let mut s = String::from("import axios from \"axios\";\n");
    let mut form = String::new();
    if mp {
        let (files, f) = js_form(b);
        if files {
            s.push_str("import { openAsBlob } from \"node:fs\";\n");
        }
        form = f;
    }
    s.push('\n');
    if mp {
        s.push_str(&form);
        s.push('\n');
    }
    s.push_str(&format!(
        "const response = await axios.request({{\n  method: {},\n  url: {},\n",
        js_str(&b.method.to_ascii_lowercase()),
        js_str(&b.url)
    ));
    if !headers.is_empty() {
        s.push_str(&js_headers(&headers));
    }
    if mp {
        s.push_str("  data: form,\n");
    } else if !b.body.is_empty() {
        s.push_str(&format!("  data: {},\n", js_text(&body_text(b))));
    }
    s.push_str("});\n\nconsole.log(response.status);\nconsole.log(response.data);\n");
    s
}

// ---- Python ----

fn python(b: &Built) -> String {
    let mp = is_multipart(b);
    let headers = merged(&headers_for(b, mp));
    let mut s = String::from("import requests\n\n");
    s.push_str(&format!("url = {}\n\n", py_str(&b.url)));
    if !headers.is_empty() {
        s.push_str("headers = {\n");
        for (k, v) in &headers {
            s.push_str(&format!("    {}: {},\n", py_str(k), py_str(v)));
        }
        s.push_str("}\n\n");
    }
    let mut args = String::new();
    if !headers.is_empty() {
        args.push_str(", headers=headers");
    }
    if mp {
        s.push_str("files = [\n");
        for f in &b.form {
            if f.file {
                s.push_str(&format!(
                    "    ({}, open({}, \"rb\")),\n",
                    py_str(&f.name),
                    py_str(&f.value)
                ));
            } else {
                s.push_str(&format!(
                    "    ({}, (None, {})),\n",
                    py_str(&f.name),
                    py_str(&f.value)
                ));
            }
        }
        s.push_str("]\n\n");
        args.push_str(", files=files");
    } else if !b.body.is_empty() {
        let text = body_text(b);
        s.push_str(&format!("payload = {}\n\n", py_text(&text, "")));
        if text.is_ascii() {
            args.push_str(", data=payload");
        } else {
            args.push_str(", data=payload.encode(\"utf-8\")");
        }
    }
    s.push_str(&format!(
        "response = requests.request({}, url{args})\n\nprint(response.status_code)\nprint(response.text)\n",
        py_str(&b.method)
    ));
    s
}

// ---- Swift ----

fn swift_str(s: &str) -> String {
    dq(s, ctrl_swift, false)
}

fn urlsession(b: &Built) -> String {
    let mut s = String::from("import Foundation\n\n");
    s.push_str(&format!(
        "var request = URLRequest(url: URL(string: {})!)\n",
        swift_str(&b.url)
    ));
    s.push_str(&format!("request.httpMethod = {}\n", swift_str(&b.method)));
    let mut seen: Vec<String> = Vec::new();
    for (k, v) in &b.headers {
        let lk = k.to_ascii_lowercase();
        let call = if seen.contains(&lk) { "addValue" } else { "setValue" };
        seen.push(lk);
        s.push_str(&format!(
            "request.{call}({}, forHTTPHeaderField: {})\n",
            swift_str(v),
            swift_str(k)
        ));
    }
    if !b.body.is_empty() {
        s.push_str(&format!(
            "request.httpBody = Data({}.utf8)\n",
            swift_str(&body_text(b))
        ));
    }
    s.push_str(
        "\ndo {\n    let (data, response) = try await URLSession.shared.data(for: request)\n    if let http = response as? HTTPURLResponse {\n        print(http.statusCode)\n    }\n    print(String(decoding: data, as: UTF8.self))\n} catch {\n    print(error)\n}\n",
    );
    s
}

// ---- Kotlin ----

fn kt_str(s: &str) -> String {
    dq(s, ctrl_u4, true)
}

fn okhttp(b: &Built) -> String {
    let mp = is_multipart(b);
    let headers = headers_for(b, mp);
    let has_body = !b.body.is_empty();
    let content_type = if mp {
        None
    } else {
        find_header(&headers, "content-type")
    };
    let mut imports: Vec<&str> = vec!["okhttp3.OkHttpClient", "okhttp3.Request"];
    if mp {
        imports.push("okhttp3.MultipartBody");
        if b.form.iter().any(|f| f.file) {
            imports.push("okhttp3.RequestBody.Companion.asRequestBody");
            imports.push("java.io.File");
        }
    } else if has_body {
        imports.push("okhttp3.RequestBody.Companion.toRequestBody");
        if content_type.is_some() {
            imports.push("okhttp3.MediaType.Companion.toMediaTypeOrNull");
        }
    } else if needs_body(&b.method) {
        imports.push("okhttp3.RequestBody.Companion.toRequestBody");
    }
    imports.sort();
    let mut s = String::new();
    for i in imports {
        s.push_str(&format!("import {i}\n"));
    }
    s.push_str("\nval client = OkHttpClient()\n\n");
    let body_var = if mp {
        s.push_str("val body = MultipartBody.Builder()\n    .setType(MultipartBody.FORM)\n");
        for f in &b.form {
            if f.file {
                s.push_str(&format!(
                    "    .addFormDataPart({}, {}, File({}).asRequestBody(null))\n",
                    kt_str(&f.name),
                    kt_str(base_name(&f.value)),
                    kt_str(&f.value)
                ));
            } else {
                s.push_str(&format!(
                    "    .addFormDataPart({}, {})\n",
                    kt_str(&f.name),
                    kt_str(&f.value)
                ));
            }
        }
        s.push_str("    .build()\n\n");
        "body"
    } else if has_body {
        let media = match content_type {
            Some(ct) => format!("{}.toMediaTypeOrNull()", kt_str(ct)),
            None => "null".to_string(),
        };
        s.push_str(&format!(
            "val body = {}.toByteArray().toRequestBody({media})\n\n",
            kt_str(&body_text(b))
        ));
        "body"
    } else if needs_body(&b.method) {
        s.push_str("val body = \"\".toRequestBody(null)\n\n");
        "body"
    } else {
        "null"
    };
    s.push_str(&format!(
        "val request = Request.Builder()\n    .url({})\n    .method({}, {body_var})\n",
        kt_str(&b.url),
        kt_str(&b.method)
    ));
    for (k, v) in &headers {
        // The body's media type already writes Content-Type.
        if content_type.is_some() && has_body && k.eq_ignore_ascii_case("content-type") {
            continue;
        }
        s.push_str(&format!("    .addHeader({}, {})\n", kt_str(k), kt_str(v)));
    }
    s.push_str("    .build()\n\nclient.newCall(request).execute().use { response ->\n    println(response.code)\n    println(response.body?.string())\n}\n");
    s
}

fn needs_body(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "PROPPATCH" | "REPORT")
}

// ---- HTTPie ----

fn httpie_key(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if matches!(c, '\\' | '=' | '@' | ':' | ';') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

fn httpie(b: &Built) -> String {
    let mp = is_multipart(b);
    let mut parts = vec!["http".to_string(), "--ignore-stdin".to_string()];
    if mp {
        parts.push("--multipart".into());
    }
    parts.push(b.method.clone());
    parts.push(sh(&b.url));
    for (k, v) in headers_for(b, mp) {
        // `Name:` would remove the header; `Name;` sends it empty.
        let item = if v.is_empty() {
            format!("{};", httpie_key(&k))
        } else {
            format!("{}:{v}", httpie_key(&k))
        };
        parts.push(sh(&item));
    }
    if mp {
        for f in &b.form {
            let sep = if f.file { '@' } else { '=' };
            parts.push(sh(&format!("{}{sep}{}", httpie_key(&f.name), f.value)));
        }
    } else if !b.body.is_empty() {
        parts.push(format!("--raw {}", sh(&body_text(b))));
    }
    parts.join(" \\\n  ")
}

// ---- Go ----

fn go_str(s: &str) -> String {
    dq(s, ctrl_u4, false)
}

fn go(b: &Built) -> String {
    let has_body = !b.body.is_empty();
    let mut s = String::from("package main\n\nimport (\n\t\"fmt\"\n\t\"io\"\n\t\"net/http\"\n");
    if has_body {
        s.push_str("\t\"strings\"\n");
    }
    s.push_str(")\n\nfunc main() {\n");
    s.push_str(&format!("\turl := {}\n", go_str(&b.url)));
    s.push_str(&format!("\tmethod := {}\n", go_str(&b.method)));
    if has_body {
        s.push_str(&format!(
            "\tpayload := strings.NewReader({})\n",
            go_str(&body_text(b))
        ));
    }
    s.push_str("\n\tclient := &http.Client{}\n");
    s.push_str(&format!(
        "\treq, err := http.NewRequest(method, url, {})\n",
        if has_body { "payload" } else { "nil" }
    ));
    s.push_str("\tif err != nil {\n\t\tfmt.Println(err)\n\t\treturn\n\t}\n");
    for (k, v) in &b.headers {
        if k.eq_ignore_ascii_case("host") {
            // net/http ignores a Host entry in the header map.
            s.push_str(&format!("\treq.Host = {}\n", go_str(v)));
        } else {
            s.push_str(&format!(
                "\treq.Header.Add({}, {})\n",
                go_str(k),
                go_str(v)
            ));
        }
    }
    s.push_str(
        "\n\tres, err := client.Do(req)\n\tif err != nil {\n\t\tfmt.Println(err)\n\t\treturn\n\t}\n\tdefer res.Body.Close()\n\n\tbody, err := io.ReadAll(res.Body)\n\tif err != nil {\n\t\tfmt.Println(err)\n\t\treturn\n\t}\n\tfmt.Println(res.Status)\n\tfmt.Println(string(body))\n}\n",
    );
    s
}

// ---- PHP ----

fn php(b: &Built) -> String {
    let names_unique = {
        let mut seen: Vec<&str> = Vec::new();
        b.form.iter().all(|f| {
            let fresh = !seen.contains(&f.name.as_str());
            seen.push(&f.name);
            fresh
        })
    };
    // Repeated names cannot sit in a PHP array; send the built bytes instead.
    let mp = is_multipart(b) && names_unique;
    let headers = headers_for(b, mp);
    let mut s = String::from("<?php\n\n$curl = curl_init();\n\ncurl_setopt_array($curl, [\n");
    s.push_str(&format!("    CURLOPT_URL => {},\n", sq_backslash(&b.url)));
    s.push_str("    CURLOPT_RETURNTRANSFER => true,\n");
    s.push_str(&format!(
        "    CURLOPT_CUSTOMREQUEST => {},\n",
        sq_backslash(&b.method)
    ));
    if b.method == "HEAD" {
        s.push_str("    CURLOPT_NOBODY => true,\n");
    }
    if !headers.is_empty() {
        s.push_str("    CURLOPT_HTTPHEADER => [\n");
        for (k, v) in &headers {
            s.push_str(&format!("        {},\n", sq_backslash(&format!("{k}: {v}"))));
        }
        s.push_str("    ],\n");
    }
    if mp {
        s.push_str("    CURLOPT_POSTFIELDS => [\n");
        for f in &b.form {
            if f.file {
                s.push_str(&format!(
                    "        {} => new CURLFile({}),\n",
                    sq_backslash(&f.name),
                    sq_backslash(&f.value)
                ));
            } else {
                s.push_str(&format!(
                    "        {} => {},\n",
                    sq_backslash(&f.name),
                    sq_backslash(&f.value)
                ));
            }
        }
        s.push_str("    ],\n");
    } else if !b.body.is_empty() {
        s.push_str(&format!(
            "    CURLOPT_POSTFIELDS => {},\n",
            sq_backslash(&body_text(b))
        ));
    }
    s.push_str("]);\n\n$response = curl_exec($curl);\n$error = curl_error($curl);\n\nif ($error) {\n    echo 'cURL error: ' . $error;\n} else {\n    echo $response;\n}\n");
    s
}

// ---- Ruby ----

fn ruby(b: &Built) -> String {
    let mp = is_multipart(b);
    let headers = headers_for(b, mp);
    let mut s = String::from("require 'net/http'\nrequire 'uri'\n\n");
    s.push_str(&format!("uri = URI.parse({})\n", sq_backslash(&b.url)));
    s.push_str("http = Net::HTTP.new(uri.host, uri.port)\nhttp.use_ssl = uri.scheme == 'https'\n\n");
    let class = match b.method.as_str() {
        "GET" | "HEAD" | "POST" | "PUT" | "DELETE" | "OPTIONS" | "TRACE" | "PATCH" => {
            Some(title_case(&b.method))
        }
        _ => None,
    };
    match class {
        Some(c) => s.push_str(&format!("request = Net::HTTP::{c}.new(uri.request_uri)\n")),
        None => s.push_str(&format!(
            "request = Net::HTTPGenericRequest.new({}, {}, true, uri.request_uri)\n",
            sq_backslash(&b.method),
            !b.body.is_empty() || mp
        )),
    }
    let mut seen: Vec<String> = Vec::new();
    for (k, v) in &headers {
        let lk = k.to_ascii_lowercase();
        if seen.contains(&lk) {
            s.push_str(&format!(
                "request.add_field({}, {})\n",
                sq_backslash(k),
                sq_backslash(v)
            ));
        } else {
            s.push_str(&format!(
                "request[{}] = {}\n",
                sq_backslash(k),
                sq_backslash(v)
            ));
        }
        seen.push(lk);
    }
    if mp {
        s.push_str("request.set_form([\n");
        for f in &b.form {
            if f.file {
                s.push_str(&format!(
                    "  [{}, File.open({})],\n",
                    sq_backslash(&f.name),
                    sq_backslash(&f.value)
                ));
            } else {
                s.push_str(&format!(
                    "  [{}, {}],\n",
                    sq_backslash(&f.name),
                    sq_backslash(&f.value)
                ));
            }
        }
        s.push_str("], 'multipart/form-data')\n");
    } else if !b.body.is_empty() {
        s.push_str(&format!("request.body = {}\n", sq_backslash(&body_text(b))));
    }
    s.push_str("\nresponse = http.request(request)\nputs response.code\nputs response.body\n");
    s
}

// ---- C# ----

fn cs_str(s: &str) -> String {
    dq(s, ctrl_u4, false)
}

/// Headers that belong to the content, not the request, in `HttpClient`.
fn is_content_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("content-") || matches!(n.as_str(), "allow" | "expires" | "last-modified")
}

fn csharp(b: &Built) -> String {
    let has_body = !b.body.is_empty();
    let content_headers: Vec<&(String, String)> = b
        .headers
        .iter()
        .filter(|(k, _)| is_content_header(k) && !k.eq_ignore_ascii_case("content-length"))
        .collect();
    let has_content = has_body || !content_headers.is_empty();
    let has_type = find_header(&b.headers, "content-type").is_some();
    let mut s = String::from("using System;\nusing System.Net.Http;\n");
    if has_content {
        s.push_str("using System.Net.Http.Headers;\nusing System.Text;\n");
    }
    let method = match b.method.as_str() {
        "GET" | "POST" | "PUT" | "DELETE" | "HEAD" | "OPTIONS" | "PATCH" | "TRACE" => {
            format!("HttpMethod.{}", title_case(&b.method))
        }
        m => format!("new HttpMethod({})", cs_str(m)),
    };
    s.push_str("\nusing var client = new HttpClient();\n");
    s.push_str(&format!(
        "using var request = new HttpRequestMessage({method}, {});\n",
        cs_str(&b.url)
    ));
    for (k, v) in &b.headers {
        if is_content_header(k) {
            continue;
        }
        s.push_str(&format!(
            "request.Headers.TryAddWithoutValidation({}, {});\n",
            cs_str(k),
            cs_str(v)
        ));
    }
    if has_content {
        s.push_str(&format!(
            "request.Content = new StringContent({}, Encoding.UTF8);\n",
            cs_str(&body_text(b))
        ));
        if let Some(ct) = find_header(&b.headers, "content-type") {
            s.push_str(&format!(
                "request.Content.Headers.ContentType = MediaTypeHeaderValue.Parse({});\n",
                cs_str(ct)
            ));
        } else if !has_type {
            s.push_str("request.Content.Headers.ContentType = null;\n");
        }
        for (k, v) in content_headers
            .into_iter()
            .filter(|(k, _)| !k.eq_ignore_ascii_case("content-type"))
        {
            s.push_str(&format!(
                "request.Content.Headers.TryAddWithoutValidation({}, {});\n",
                cs_str(k),
                cs_str(v)
            ));
        }
    }
    s.push_str("\nusing var response = await client.SendAsync(request);\nConsole.WriteLine((int)response.StatusCode);\nConsole.WriteLine(await response.Content.ReadAsStringAsync());\n");
    s
}

// ---- Java ----

fn java_str(s: &str) -> String {
    dq(s, ctrl_octal, false)
}

fn java(b: &Built) -> String {
    let has_body = !b.body.is_empty();
    let mut s = String::from(
        "import java.net.URI;\nimport java.net.http.HttpClient;\nimport java.net.http.HttpRequest;\nimport java.net.http.HttpResponse;\n\npublic class Main {\n    public static void main(String[] args) throws Exception {\n        HttpClient client = HttpClient.newHttpClient();\n",
    );
    s.push_str("        HttpRequest request = HttpRequest.newBuilder()\n");
    s.push_str(&format!(
        "                .uri(URI.create({}))\n",
        java_str(&b.url)
    ));
    let mut restricted: Vec<String> = Vec::new();
    for (k, v) in &b.headers {
        // HttpClient refuses to set these (see jdk.httpclient.allowRestrictedHeaders).
        if matches!(
            k.to_ascii_lowercase().as_str(),
            "connection" | "content-length" | "expect" | "host" | "upgrade"
        ) {
            restricted.push(format!("{k}: {v}").replace(['\r', '\n'], " "));
            continue;
        }
        s.push_str(&format!(
            "                .header({}, {})\n",
            java_str(k),
            java_str(v)
        ));
    }
    let publisher = if has_body {
        format!(
            "HttpRequest.BodyPublishers.ofString({})",
            java_str(&body_text(b))
        )
    } else {
        "HttpRequest.BodyPublishers.noBody()".to_string()
    };
    s.push_str(&format!(
        "                .method({}, {publisher})\n                .build();\n",
        java_str(&b.method)
    ));
    for r in restricted {
        s.push_str(&format!(
            "        // Not set, the JDK client controls it: {r}\n"
        ));
    }
    s.push_str(
        "\n        HttpResponse<String> response = client.send(request, HttpResponse.BodyHandlers.ofString());\n        System.out.println(response.statusCode());\n        System.out.println(response.body());\n    }\n}\n",
    );
    s
}

// ---- PowerShell ----

fn powershell(b: &Built) -> String {
    let has_body = !b.body.is_empty();
    let content_type = find_header(&b.headers, "content-type").map(str::to_string);
    let user_agent = find_header(&b.headers, "user-agent").map(str::to_string);
    let rest: Vec<(String, String)> = b
        .headers
        .iter()
        .filter(|(k, _)| {
            !k.eq_ignore_ascii_case("content-type") && !k.eq_ignore_ascii_case("user-agent")
        })
        .cloned()
        .collect();
    let rest = merged(&rest);
    let mut s = String::new();
    if !rest.is_empty() {
        s.push_str("$headers = @{\n");
        for (k, v) in &rest {
            s.push_str(&format!("    {} = {}\n", ps(k), ps(v)));
        }
        s.push_str("}\n\n");
    }
    let text = body_text(b);
    if has_body {
        s.push_str(&format!("$body = {}\n\n", ps(&text)));
    }
    let method = match b.method.as_str() {
        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS" | "TRACE" => {
            format!("-Method {}", title_case(&b.method))
        }
        m => format!("-CustomMethod {}", ps(m)),
    };
    s.push_str("$response = Invoke-RestMethod `\n");
    s.push_str(&format!("    -Uri {} `\n", ps(&b.url)));
    s.push_str(&format!("    {method}"));
    if !rest.is_empty() {
        s.push_str(" `\n    -Headers $headers");
    }
    if let Some(ct) = content_type {
        s.push_str(&format!(" `\n    -ContentType {}", ps(&ct)));
    }
    if let Some(ua) = user_agent {
        s.push_str(&format!(" `\n    -UserAgent {}", ps(&ua)));
    }
    if has_body {
        if text.is_ascii() {
            s.push_str(" `\n    -Body $body");
        } else {
            s.push_str(" `\n    -Body ([System.Text.Encoding]::UTF8.GetBytes($body))");
        }
    }
    s.push_str("\n\n$response\n");
    s
}

// ---- Wget ----

fn wget(b: &Built) -> String {
    let mut parts = vec!["wget --quiet".to_string()];
    if b.method != "GET" || !b.body.is_empty() {
        parts.push(format!("--method={}", sh(&b.method)));
    }
    for (k, v) in &b.headers {
        parts.push(format!("--header={}", sh(&format!("{k}: {v}"))));
    }
    if !b.body.is_empty() {
        parts.push(format!("--body-data={}", sh(&body_text(b))));
    }
    parts.push("--output-document=-".into());
    parts.push(sh(&b.url));
    parts.join(" \\\n  ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FormField;

    fn built() -> Built {
        Built {
            method: "POST".into(),
            url: "https://api.example.com/users?x=1".into(),
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("X-Token".into(), "abc123".into()),
            ],
            body: br#"{"name":"Ada"}"#.to_vec(),
            ..Default::default()
        }
    }

    fn gen(lang: &str, b: &Built) -> String {
        generate_code(lang, b).unwrap()
    }

    fn multipart() -> Built {
        Built {
            method: "POST".into(),
            url: "https://up.example.com/files".into(),
            headers: vec![
                (
                    "Content-Type".into(),
                    "multipart/form-data; boundary=----NekoBoundaryX".into(),
                ),
                ("X-Token".into(), "abc123".into()),
            ],
            body: b"--raw--".to_vec(),
            form: vec![
                FormField {
                    name: "title".into(),
                    value: "Hello world".into(),
                    file: false,
                },
                FormField {
                    name: "doc".into(),
                    value: "/tmp/report.pdf".into(),
                    file: true,
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn language_ids_are_unique_and_all_generate() {
        let langs = code_languages();
        assert_eq!(langs.len(), 14);
        let mut ids: Vec<&str> = langs.iter().map(|(id, _)| *id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), langs.len());
        for (id, label) in langs {
            assert!(!label.is_empty());
            assert!(generate_code(id, &built()).is_ok(), "{id}");
            assert!(generate_code(id, &multipart()).is_ok(), "{id} multipart");
            assert!(generate_code(id, &Built::default()).is_ok(), "{id} empty");
        }
    }

    #[test]
    fn unknown_language_is_an_error() {
        assert_eq!(
            generate_code("cobol", &built()).unwrap_err(),
            "no generator for cobol"
        );
    }

    #[test]
    fn every_language_has_method_url_and_headers() {
        let b = built();
        for (id, _) in code_languages() {
            let code = gen(id, &b);
            // curl leaves POST implied once there is a body.
            assert!(
                *id == "curl" || code.to_ascii_lowercase().contains("post"),
                "{id} lacks method:\n{code}"
            );
            assert!(code.contains(&b.url), "{id} lacks url:\n{code}");
            assert!(code.contains("X-Token"), "{id} lacks header:\n{code}");
            assert!(code.contains("abc123"), "{id} lacks header value:\n{code}");
            assert!(code.contains("Ada"), "{id} lacks body:\n{code}");
        }
    }

    #[test]
    fn get_without_body_has_no_body() {
        let b = Built {
            method: "GET".into(),
            url: "https://x.dev/a".into(),
            headers: vec![("Accept".into(), "*/*".into())],
            ..Default::default()
        };
        assert!(!gen("python", &b).contains("data="));
        assert!(!gen("fetch", &b).contains("body:"));
        assert!(!gen("wget", &b).contains("--method"));
        assert!(gen("go", &b).contains("http.NewRequest(method, url, nil)"));
        assert!(gen("okhttp", &b).contains(".method(\"GET\", null)"));
        assert!(gen("java", &b).contains("noBody()"));
        assert!(gen("curl", &b).contains("curl"));
    }

    fn tricky() -> Built {
        Built {
            method: "PUT".into(),
            url: "https://x.dev/".into(),
            headers: vec![("X-Q".into(), "a\"b\\c".into())],
            body: b"say \"hi\" \\ back\nline2 $x ${y} 'q'".to_vec(),
            ..Default::default()
        }
    }

    #[test]
    fn escapes_for_each_language() {
        let b = tricky();
        let curl = gen("curl", &b);
        assert!(curl.contains("'\\''q'\\''"), "{curl}");
        assert!(curl.contains("$x ${y}"), "{curl}");

        let fetch = gen("fetch", &b);
        assert!(fetch.contains("\\\\ back"), "{fetch}");
        assert!(fetch.contains("\\${y}"), "{fetch}");
        assert!(fetch.contains("\"a\\\"b\\\\c\""), "{fetch}");

        let axios = gen("axios", &b);
        assert!(axios.contains("\\${y}") && axios.contains("\\\\ back"), "{axios}");

        let py = gen("python", &b);
        assert!(py.contains("\"say \\\"hi\\\" \\\\ back\\n\""), "{py}");
        assert!(py.contains("\"line2 $x ${y} 'q'\""), "{py}");

        let swift = gen("urlsession", &b);
        assert!(swift.contains("say \\\"hi\\\" \\\\ back\\nline2 $x ${y} 'q'"), "{swift}");

        let kt = gen("okhttp", &b);
        assert!(kt.contains("\\$x \\${y}"), "{kt}");
        assert!(kt.contains("say \\\"hi\\\" \\\\ back\\nline2"), "{kt}");

        let httpie = gen("httpie", &b);
        assert!(httpie.contains("'\\''q'\\''"), "{httpie}");

        let go = gen("go", &b);
        assert!(go.contains("say \\\"hi\\\" \\\\ back\\nline2 $x ${y} 'q'"), "{go}");

        let php = gen("php", &b);
        assert!(php.contains("\\\\ back"), "{php}");
        assert!(php.contains("$x ${y} \\'q\\'"), "{php}");
        assert!(php.contains("'X-Q: a\"b\\\\c'"), "{php}");

        let ruby = gen("ruby", &b);
        assert!(ruby.contains("$x ${y} \\'q\\'"), "{ruby}");

        let cs = gen("csharp", &b);
        assert!(cs.contains("say \\\"hi\\\" \\\\ back\\nline2 $x ${y} 'q'"), "{cs}");

        let java = gen("java", &b);
        assert!(java.contains("say \\\"hi\\\" \\\\ back\\nline2 $x ${y} 'q'"), "{java}");

        let ps = gen("powershell", &b);
        assert!(ps.contains("$x ${y} ''q''"), "{ps}");
        assert!(ps.contains("-Method Put"), "{ps}");

        let wget = gen("wget", &b);
        assert!(wget.contains("'\\''q'\\''"), "{wget}");
    }

    #[test]
    fn control_characters_and_curly_quotes() {
        let b = Built {
            method: "POST".into(),
            url: "https://x.dev/".into(),
            body: "a\u{1}b\tc\r\n\u{2019}".as_bytes().to_vec(),
            ..Default::default()
        };
        assert!(gen("go", &b).contains("a\\u0001b\\tc\\r\\n"));
        assert!(gen("java", &b).contains("a\\001b\\tc\\r\\n"));
        assert!(gen("urlsession", &b).contains("a\\u{1}b"));
        assert!(gen("python", &b).contains("a\\x01b\\tc\\r\\n"));
        assert!(gen("powershell", &b).contains("\u{2019}\u{2019}"));
        // \r means no template literal.
        assert!(gen("fetch", &b).contains("body: \"a\\u0001b"));
    }

    #[test]
    fn multi_line_json_stays_readable() {
        let b = Built {
            method: "POST".into(),
            url: "https://x.dev/".into(),
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: b"{\n  \"a\": 1\n}".to_vec(),
            ..Default::default()
        };
        let py = gen("python", &b);
        assert!(py.contains("payload = (\n    \"{\\n\"\n    \"  \\\"a\\\": 1\\n\"\n    \"}\"\n)"), "{py}");
        let js = gen("fetch", &b);
        assert!(js.contains("body: `{\n  \"a\": 1\n}`"), "{js}");
    }

    #[test]
    fn multipart_curl_uses_dash_f() {
        let b = multipart();
        let c = gen("curl", &b);
        assert!(c.contains("-F 'title=Hello world'"), "{c}");
        assert!(c.contains("-F 'doc=@/tmp/report.pdf'"), "{c}");
        assert!(c.contains("-H 'X-Token: abc123'"), "{c}");
        assert!(!c.to_ascii_lowercase().contains("content-type"), "{c}");
        assert!(!c.contains("--data-raw"), "{c}");
        assert!(!c.contains("-X"), "{c}");
        let put = Built {
            method: "PUT".into(),
            ..b
        };
        assert!(gen("curl", &put).contains("-X 'PUT'"));
    }

    #[test]
    fn curl_form_values_that_curl_would_interpret() {
        let mut b = multipart();
        b.form = vec![
            FormField {
                name: "a".into(),
                value: "@notfile".into(),
                file: false,
            },
            FormField {
                name: "b".into(),
                value: "x;y".into(),
                file: false,
            },
            FormField {
                name: "f".into(),
                value: "/tmp/a;b.txt".into(),
                file: true,
            },
        ];
        let c = gen("curl", &b);
        assert!(c.contains("--form-string 'a=@notfile'"), "{c}");
        assert!(c.contains("--form-string 'b=x;y'"), "{c}");
        assert!(c.contains("-F 'f=@\"/tmp/a;b.txt\"'"), "{c}");
    }

    #[test]
    fn multipart_in_other_languages() {
        let b = multipart();
        let py = gen("python", &b);
        assert!(py.contains("(\"title\", (None, \"Hello world\"))"), "{py}");
        assert!(py.contains("(\"doc\", open(\"/tmp/report.pdf\", \"rb\"))"), "{py}");
        assert!(py.contains("files=files") && !py.contains("boundary"), "{py}");

        let fetch = gen("fetch", &b);
        assert!(fetch.contains("form.append(\"title\", \"Hello world\")"), "{fetch}");
        assert!(fetch.contains("openAsBlob(\"/tmp/report.pdf\"), \"report.pdf\""), "{fetch}");
        assert!(fetch.contains("body: form") && !fetch.contains("boundary"), "{fetch}");
        assert!(gen("axios", &b).contains("data: form"));

        let kt = gen("okhttp", &b);
        assert!(kt.contains(".addFormDataPart(\"title\", \"Hello world\")"), "{kt}");
        assert!(kt.contains("File(\"/tmp/report.pdf\").asRequestBody(null)"), "{kt}");
        assert!(!kt.contains("boundary") && !kt.contains("Content-Type"), "{kt}");

        let http = gen("httpie", &b);
        assert!(http.contains("--multipart"), "{http}");
        assert!(http.contains("'title=Hello world'") && http.contains("'doc@/tmp/report.pdf'"), "{http}");
        assert!(!http.contains("--raw"), "{http}");

        let php = gen("php", &b);
        assert!(php.contains("'doc' => new CURLFile('/tmp/report.pdf')"), "{php}");

        let ruby = gen("ruby", &b);
        assert!(ruby.contains("request.set_form(["), "{ruby}");
        assert!(ruby.contains("['doc', File.open('/tmp/report.pdf')]"), "{ruby}");

        // No multipart helper: the built bytes and boundary header go out as is.
        assert!(gen("go", &b).contains("boundary=----NekoBoundaryX"));
        assert!(gen("wget", &b).contains("--body-data='--raw--'"));
    }

    #[test]
    fn host_and_repeated_headers() {
        let b = Built {
            method: "GET".into(),
            url: "https://x.dev/".into(),
            headers: vec![
                ("Host".into(), "other.dev".into()),
                ("X-A".into(), "1".into()),
                ("x-a".into(), "2".into()),
            ],
            ..Default::default()
        };
        assert!(gen("go", &b).contains("req.Host = \"other.dev\""));
        assert!(gen("python", &b).contains("\"X-A\": \"1, 2\""));
        assert!(gen("urlsession", &b).contains("request.addValue(\"2\""));
        assert!(gen("ruby", &b).contains("request.add_field('x-a', '2')"));
        assert!(gen("java", &b).contains("Not set, the JDK client controls it: Host: other.dev"));
    }

    #[test]
    fn custom_methods_and_empty_header_values() {
        let b = Built {
            method: "PURGE".into(),
            url: "https://x.dev/".into(),
            headers: vec![("X-Empty".into(), String::new())],
            ..Default::default()
        };
        assert!(gen("httpie", &b).contains("'X-Empty;'"));
        assert!(gen("csharp", &b).contains("new HttpMethod(\"PURGE\")"));
        assert!(gen("powershell", &b).contains("-CustomMethod 'PURGE'"));
        assert!(gen("ruby", &b).contains("Net::HTTPGenericRequest.new('PURGE', false, true"));
        assert!(gen("okhttp", &b).contains(".method(\"PURGE\", null)"));
    }

    #[test]
    fn okhttp_and_csharp_content_type() {
        let b = built();
        let kt = gen("okhttp", &b);
        assert!(kt.contains("toRequestBody(\"application/json\".toMediaTypeOrNull())"), "{kt}");
        assert!(!kt.contains(".addHeader(\"Content-Type\""), "{kt}");
        let cs = gen("csharp", &b);
        assert!(cs.contains("MediaTypeHeaderValue.Parse(\"application/json\")"), "{cs}");
        assert!(!cs.contains("Headers.TryAddWithoutValidation(\"Content-Type\""), "{cs}");
        let ps = gen("powershell", &b);
        assert!(ps.contains("-ContentType 'application/json'"), "{ps}");
        assert!(ps.contains("'X-Token' = 'abc123'"), "{ps}");
    }
}
