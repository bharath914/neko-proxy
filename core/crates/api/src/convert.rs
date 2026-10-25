//! Core types to wire types.

use crate::pb;
use decode::{BodyView, HtmlBlockKind, JsonTokenKind, Kind, SourceKind};
use flow::{FlowState, FlowSummary, Timings};

pub fn summary(s: &FlowSummary) -> pb::FlowSummary {
    {
        pb::FlowSummary {
            id: s.id,
            started_at_ms: s.started_at_ms,
            duration_ms: s.duration_ms,
            scheme: s.scheme.clone(),
            host: s.host.clone(),
            port: s.port as u32,
            method: s.method.clone(),
            path: s.path.clone(),
            status: s.status as u32,
            protocol: s.protocol.clone(),
            app: s.app.clone(),
            req_size: s.req_size,
            res_size: s.res_size,
            state: match s.state {
                FlowState::Pending => pb::FlowState::Pending,
                FlowState::Receiving => pb::FlowState::Receiving,
                FlowState::Complete => pb::FlowState::Complete,
                FlowState::Failed => pb::FlowState::Failed,
            } as i32,
            flags: s.flags,
            issue_labels: decode::issue_labels(s.flags),
            content_type: s.content_type.clone(),
            error: s.error.clone(),
            source: match s.source {
                flow::Source::Proxy => pb::FlowSource::Proxy,
                flow::Source::Sdk => pb::FlowSource::Sdk,
                flow::Source::Merged => pb::FlowSource::Merged,
            } as i32,
            merged_with: s.merged_with,
            device_id: s.device_id.clone(),
            app_id: s.app_id.clone(),
            name: s.name.clone(),
            tag: s.tag.clone(),
            owner: s.owner.clone(),
            screen: s.screen.clone(),
            route_template: s.route_template.clone(),
            client_stack: s.client_stack.clone(),
        }
    }
}

pub fn timings(t: &Timings) -> pb::Timings {
    {
        let ms = |us: u64| us as f64 / 1000.0;
        pb::Timings {
            dns_ms: ms(t.dns_us),
            connect_ms: ms(t.connect_us),
            tls_ms: ms(t.tls_us),
            send_ms: ms(t.send_us),
            wait_ms: ms(t.wait_us),
            receive_ms: ms(t.receive_us),
            total_ms: ms(t.total_us),
            reused_connection: t.reused,
        }
    }
}

pub fn body(v: BodyView) -> pb::BodyView {
    {
        pb::BodyView {
            kind: match v.kind {
                Kind::Empty => pb::BodyKind::Empty,
                Kind::Json => pb::BodyKind::Json,
                Kind::Text => pb::BodyKind::Text,
                Kind::Html => pb::BodyKind::Html,
                Kind::Form => pb::BodyKind::Form,
                Kind::Image => pb::BodyKind::Image,
                Kind::Binary => pb::BodyKind::Binary,
            } as i32,
            content_type: v.content_type,
            content_encoding: v.content_encoding,
            wire_size: v.wire_size,
            decoded_size: v.decoded_size,
            capture_truncated: v.capture_truncated,
            preview_truncated: v.preview_truncated,
            text: v.text,
            json: v
                .json
                .into_iter()
                .map(|l| pb::JsonLine {
                    depth: l.depth,
                    fold_end: l.fold_end,
                    tokens: l
                        .tokens
                        .into_iter()
                        .map(|t| pb::JsonToken {
                            kind: match t.kind {
                                JsonTokenKind::Punct => pb::JsonTokenKind::Punct,
                                JsonTokenKind::Key => pb::JsonTokenKind::Key,
                                JsonTokenKind::String => pb::JsonTokenKind::String,
                                JsonTokenKind::Number => pb::JsonTokenKind::Number,
                                JsonTokenKind::Literal => pb::JsonTokenKind::Literal,
                            } as i32,
                            text: t.text,
                        })
                        .collect(),
                })
                .collect(),
            html: v
                .html
                .into_iter()
                .map(|b| pb::HtmlBlock {
                    kind: match b.kind {
                        HtmlBlockKind::Paragraph => pb::HtmlBlockKind::HtmlParagraph,
                        HtmlBlockKind::Heading => pb::HtmlBlockKind::HtmlHeading,
                        HtmlBlockKind::ListItem => pb::HtmlBlockKind::HtmlListItem,
                        HtmlBlockKind::Quote => pb::HtmlBlockKind::HtmlQuote,
                        HtmlBlockKind::Code => pb::HtmlBlockKind::HtmlCode,
                        HtmlBlockKind::Rule => pb::HtmlBlockKind::HtmlRule,
                        HtmlBlockKind::Image => pb::HtmlBlockKind::HtmlImage,
                    } as i32,
                    level: b.level,
                    marker: b.marker,
                    spans: b
                        .spans
                        .into_iter()
                        .map(|s| pb::HtmlSpan {
                            text: s.text,
                            style: s.style,
                            href: s.href,
                        })
                        .collect(),
                })
                .collect(),
            html_title: v.html_title,
            source: v
                .source
                .into_iter()
                .map(|l| pb::SourceLine {
                    tokens: l
                        .tokens
                        .into_iter()
                        .map(|t| pb::SourceToken {
                            kind: match t.kind {
                                SourceKind::Plain => pb::SourceTokenKind::SrcPlain,
                                SourceKind::Tag => pb::SourceTokenKind::SrcTag,
                                SourceKind::Attr => pb::SourceTokenKind::SrcAttr,
                                SourceKind::Value => pb::SourceTokenKind::SrcValue,
                                SourceKind::Comment => pb::SourceTokenKind::SrcComment,
                                SourceKind::Punct => pb::SourceTokenKind::SrcPunct,
                                SourceKind::Entity => pb::SourceTokenKind::SrcEntity,
                            } as i32,
                            text: t.text,
                        })
                        .collect(),
                })
                .collect(),
            form: v
                .form
                .into_iter()
                .map(|(name, value)| pb::FormField { name, value })
                .collect(),
            events: v
                .events
                .into_iter()
                .map(|e| pb::SseEvent {
                    event: e.event,
                    id: e.id,
                    data: e.data,
                    retry_ms: e.retry_ms,
                })
                .collect(),
            protocol: Some(pb::ProtocolView {
                title: v.protocol.title,
                note: v.protocol.note,
                fields: v
                    .protocol
                    .fields
                    .into_iter()
                    .map(|f| pb::DecodedField {
                        depth: f.depth,
                        label: f.label,
                        value: f.value,
                    })
                    .collect(),
            }),
            image: v.image,
            note: v.note,
        }
    }
}

// ---------------------------------------------------------------- rules

pub fn rule_to_pb(r: &rules::Rule) -> pb::Rule {
    use pb::rule::Action as A;
    let hdr = |e: &rules::HeaderEdit| pb::HeaderEdit {
        op: match e.op {
            rules::HeaderOp::Set => pb::header_edit::Op::Set,
            rules::HeaderOp::Add => pb::header_edit::Op::Add,
            rules::HeaderOp::Remove => pb::header_edit::Op::Remove,
        } as i32,
        name: e.name.clone(),
        value: e.value.clone(),
    };
    let action = match &r.action {
        rules::Action::Mock {
            status,
            headers,
            body,
            delay_ms,
        } => A::Mock(pb::MockAction {
            status: *status as u32,
            headers: headers
                .iter()
                .map(|(n, v)| pb::Header {
                    name: n.clone(),
                    value: v.clone(),
                })
                .collect(),
            body: body.clone(),
            delay_ms: *delay_ms,
        }),
        rules::Action::MapRemote { scheme, host, port } => A::MapRemote(pb::MapRemoteAction {
            scheme: scheme.clone(),
            host: host.clone(),
            port: *port as u32,
        }),
        rules::Action::Rewrite {
            request_headers,
            response_headers,
        } => A::Rewrite(pb::RewriteAction {
            request_headers: request_headers.iter().map(hdr).collect(),
            response_headers: response_headers.iter().map(hdr).collect(),
        }),
        rules::Action::Scenario {
            fail_one_in,
            delay_ms,
            jitter_ms,
            flaky_one_in,
        } => A::Scenario(pb::ScenarioAction {
            fail_one_in: *fail_one_in,
            delay_ms: *delay_ms,
            jitter_ms: *jitter_ms,
            flaky_one_in: *flaky_one_in,
        }),
        rules::Action::Throttle { delay_ms } => A::Throttle(pb::ThrottleAction {
            delay_ms: *delay_ms,
        }),
        rules::Action::Breakpoint { phase, timeout_s } => A::Breakpoint(pb::BreakpointAction {
            phase: match phase {
                rules::BreakpointPhase::Both => pb::breakpoint_action::Phase::Both,
                rules::BreakpointPhase::Request => pb::breakpoint_action::Phase::Request,
                rules::BreakpointPhase::Response => pb::breakpoint_action::Phase::Response,
            } as i32,
            timeout_s: *timeout_s,
        }),
    };
    pb::Rule {
        id: r.id.clone(),
        name: r.name.clone(),
        enabled: r.enabled,
        matcher: Some(pb::RuleMatch {
            scheme: r.matcher.scheme.clone(),
            host: r.matcher.host.clone(),
            port: r.matcher.port as u32,
            method: r.matcher.method.clone(),
            path: r.matcher.path.clone(),
            query: r.matcher.query.clone(),
        }),
        action: Some(action),
    }
}

pub fn rule_from_pb(r: pb::Rule) -> Result<rules::Rule, String> {
    use pb::rule::Action as A;
    let hdr = |e: pb::HeaderEdit| rules::HeaderEdit {
        op: match pb::header_edit::Op::try_from(e.op).unwrap_or(pb::header_edit::Op::Set) {
            pb::header_edit::Op::Set => rules::HeaderOp::Set,
            pb::header_edit::Op::Add => rules::HeaderOp::Add,
            pb::header_edit::Op::Remove => rules::HeaderOp::Remove,
        },
        name: e.name,
        value: e.value,
    };
    let port = |p: u32| -> Result<u16, String> {
        u16::try_from(p).map_err(|_| format!("{p} is not a port"))
    };
    let m = r.matcher.unwrap_or_default();
    let action = match r.action.ok_or("a rule needs an action")? {
        A::Mock(a) => rules::Action::Mock {
            status: u16::try_from(a.status).map_err(|_| "status out of range".to_string())?,
            headers: a.headers.into_iter().map(|h| (h.name, h.value)).collect(),
            body: a.body,
            delay_ms: a.delay_ms,
        },
        A::MapRemote(a) => rules::Action::MapRemote {
            scheme: a.scheme,
            host: a.host,
            port: port(a.port)?,
        },
        A::Rewrite(a) => rules::Action::Rewrite {
            request_headers: a.request_headers.into_iter().map(hdr).collect(),
            response_headers: a.response_headers.into_iter().map(hdr).collect(),
        },
        A::Scenario(a) => rules::Action::Scenario {
            fail_one_in: a.fail_one_in,
            delay_ms: a.delay_ms,
            jitter_ms: a.jitter_ms,
            flaky_one_in: a.flaky_one_in,
        },
        A::Throttle(a) => rules::Action::Throttle {
            delay_ms: a.delay_ms,
        },
        A::Breakpoint(a) => rules::Action::Breakpoint {
            phase: match pb::breakpoint_action::Phase::try_from(a.phase)
                .unwrap_or(pb::breakpoint_action::Phase::Both)
            {
                pb::breakpoint_action::Phase::Both => rules::BreakpointPhase::Both,
                pb::breakpoint_action::Phase::Request => rules::BreakpointPhase::Request,
                pb::breakpoint_action::Phase::Response => rules::BreakpointPhase::Response,
            },
            timeout_s: a.timeout_s,
        },
    };
    Ok(rules::Rule {
        id: r.id,
        name: r.name,
        enabled: r.enabled,
        matcher: rules::Match {
            scheme: m.scheme,
            host: m.host,
            port: port(m.port)?,
            method: m.method,
            path: m.path,
            query: m.query,
        },
        action,
    })
}

pub fn paused(p: &rules::PausedFlow) -> pb::PausedFlow {
    pb::PausedFlow {
        flow_id: p.flow_id,
        rule_id: p.rule_id.clone(),
        phase: match p.phase {
            flow::Phase::Request => pb::MessagePhase::PhaseRequest,
            flow::Phase::Response => pb::MessagePhase::PhaseResponse,
        } as i32,
        summary: Some(summary(&p.summary)),
        method: p.held.method.clone(),
        path: p.held.path_and_query.clone(),
        status: p.held.status as u32,
        headers: p
            .held
            .headers
            .iter()
            .map(|(n, v)| pb::Header {
                name: n.clone(),
                value: v.clone(),
            })
            .collect(),
        body: p.held.body.to_vec(),
        since_ms: p.since_ms,
        timeout_s: p.timeout_s as u32,
    }
}

pub fn approval(a: &crate::Approval) -> pb::Approval {
    pb::Approval {
        id: a.id.clone(),
        tool: a.tool.clone(),
        title: a.title.clone(),
        detail: a.detail.clone(),
        requested_ms: a.requested_ms,
    }
}

pub fn mcp_writes_to_pb(w: rules::McpWrites) -> pb::McpWrites {
    match w {
        rules::McpWrites::Ask => pb::McpWrites::McpAsk,
        rules::McpWrites::Auto => pb::McpWrites::McpAuto,
        rules::McpWrites::Off => pb::McpWrites::McpOff,
    }
}

pub fn mcp_writes_from_pb(w: i32) -> rules::McpWrites {
    match pb::McpWrites::try_from(w).unwrap_or(pb::McpWrites::McpAsk) {
        pb::McpWrites::McpAsk => rules::McpWrites::Ask,
        pb::McpWrites::McpAuto => rules::McpWrites::Auto,
        pb::McpWrites::McpOff => rules::McpWrites::Off,
    }
}

// ---------------------------------------------------------------- composer

fn kv_to_pb(rows: &[compose::KeyValue]) -> Vec<pb::KeyValue> {
    rows.iter()
        .map(|r| pb::KeyValue {
            name: r.name.clone(),
            value: r.value.clone(),
            disabled: r.disabled,
            description: r.description.clone(),
            secret: r.secret,
            file: r.file,
        })
        .collect()
}

pub fn kv_to_pb_pub(rows: &[compose::KeyValue]) -> Vec<pb::KeyValue> {
    kv_to_pb(rows)
}

pub fn cookie_to_pb(c: &compose::Cookie) -> pb::Cookie {
    pb::Cookie {
        name: c.name.clone(),
        value: c.value.clone(),
        domain: c.domain.clone(),
        path: c.path.clone(),
        expires: c.expires.clone(),
        max_age: c.max_age.clone(),
        secure: c.secure,
        http_only: c.http_only,
        same_site: c.same_site.clone(),
    }
}

pub fn kv_from_pb(rows: Vec<pb::KeyValue>) -> Vec<compose::KeyValue> {
    rows.into_iter()
        .map(|r| compose::KeyValue {
            name: r.name,
            value: r.value,
            disabled: r.disabled,
            description: r.description,
            secret: r.secret,
            file: r.file,
        })
        .collect()
}

fn body_to_pb(k: compose::BodyKind) -> pb::ComposeBody {
    use compose::BodyKind as K;
    match k {
        K::None => pb::ComposeBody::None,
        K::Json => pb::ComposeBody::Json,
        K::Text => pb::ComposeBody::Text,
        K::Xml => pb::ComposeBody::Xml,
        K::Form => pb::ComposeBody::Form,
        K::Html => pb::ComposeBody::Html,
        K::Javascript => pb::ComposeBody::Javascript,
        K::FormData => pb::ComposeBody::FormData,
        K::Graphql => pb::ComposeBody::Graphql,
        K::Binary => pb::ComposeBody::Binary,
    }
}

fn body_from_pb(k: i32) -> compose::BodyKind {
    use compose::BodyKind as K;
    match pb::ComposeBody::try_from(k).unwrap_or(pb::ComposeBody::None) {
        pb::ComposeBody::None => K::None,
        pb::ComposeBody::Json => K::Json,
        pb::ComposeBody::Text => K::Text,
        pb::ComposeBody::Xml => K::Xml,
        pb::ComposeBody::Form => K::Form,
        pb::ComposeBody::Html => K::Html,
        pb::ComposeBody::Javascript => K::Javascript,
        pb::ComposeBody::FormData => K::FormData,
        pb::ComposeBody::Graphql => K::Graphql,
        pb::ComposeBody::Binary => K::Binary,
    }
}

fn auth_to_pb(k: compose::AuthKind) -> pb::ComposeAuth {
    use compose::AuthKind as K;
    match k {
        K::None => pb::ComposeAuth::None,
        K::Bearer => pb::ComposeAuth::Bearer,
        K::Basic => pb::ComposeAuth::Basic,
        K::Inherit => pb::ComposeAuth::Inherit,
        K::ApiKey => pb::ComposeAuth::ApiKey,
    }
}

fn auth_from_pb(k: i32) -> compose::AuthKind {
    use compose::AuthKind as K;
    match pb::ComposeAuth::try_from(k).unwrap_or(pb::ComposeAuth::None) {
        pb::ComposeAuth::None => K::None,
        pb::ComposeAuth::Bearer => K::Bearer,
        pb::ComposeAuth::Basic => K::Basic,
        pb::ComposeAuth::Inherit => K::Inherit,
        pb::ComposeAuth::ApiKey => K::ApiKey,
    }
}

fn assertion_to_pb(a: &compose::Assertion) -> pb::Assertion {
    use compose::{AssertKind as K, AssertOp as O};
    pb::Assertion {
        kind: match a.kind {
            K::Status => pb::AssertKind::Status,
            K::Header => pb::AssertKind::Header,
            K::Body => pb::AssertKind::Body,
            K::Json => pb::AssertKind::Json,
            K::Time => pb::AssertKind::Time,
            K::Size => pb::AssertKind::Size,
        } as i32,
        target: a.target.clone(),
        op: match a.op {
            O::Equals => pb::AssertOp::Equals,
            O::NotEquals => pb::AssertOp::NotEquals,
            O::Contains => pb::AssertOp::Contains,
            O::NotContains => pb::AssertOp::NotContains,
            O::Exists => pb::AssertOp::Exists,
            O::NotExists => pb::AssertOp::NotExists,
            O::LessThan => pb::AssertOp::LessThan,
            O::GreaterThan => pb::AssertOp::GreaterThan,
            O::Matches => pb::AssertOp::Matches,
        } as i32,
        expected: a.expected.clone(),
        disabled: a.disabled,
    }
}

fn assertion_from_pb(a: pb::Assertion) -> compose::Assertion {
    use compose::{AssertKind as K, AssertOp as O};
    compose::Assertion {
        kind: match pb::AssertKind::try_from(a.kind).unwrap_or(pb::AssertKind::Status) {
            pb::AssertKind::Status => K::Status,
            pb::AssertKind::Header => K::Header,
            pb::AssertKind::Body => K::Body,
            pb::AssertKind::Json => K::Json,
            pb::AssertKind::Time => K::Time,
            pb::AssertKind::Size => K::Size,
        },
        target: a.target,
        op: match pb::AssertOp::try_from(a.op).unwrap_or(pb::AssertOp::Equals) {
            pb::AssertOp::Equals => O::Equals,
            pb::AssertOp::NotEquals => O::NotEquals,
            pb::AssertOp::Contains => O::Contains,
            pb::AssertOp::NotContains => O::NotContains,
            pb::AssertOp::Exists => O::Exists,
            pb::AssertOp::NotExists => O::NotExists,
            pb::AssertOp::LessThan => O::LessThan,
            pb::AssertOp::GreaterThan => O::GreaterThan,
            pb::AssertOp::Matches => O::Matches,
        },
        expected: a.expected,
        disabled: a.disabled,
    }
}

pub fn composed_to_pb(r: &compose::Request) -> pb::ComposedRequest {
    pb::ComposedRequest {
        id: r.id.clone(),
        name: r.name.clone(),
        collection: r.collection.clone(),
        method: r.method.clone(),
        url: r.url.clone(),
        params: kv_to_pb(&r.params),
        headers: kv_to_pb(&r.headers),
        body_kind: body_to_pb(r.body_kind) as i32,
        body: r.body.clone(),
        form: kv_to_pb(&r.form),
        auth: auth_to_pb(r.auth) as i32,
        auth_token: r.auth_token.clone(),
        auth_user: r.auth_user.clone(),
        auth_password: r.auth_password.clone(),
        updated_ms: r.updated_ms,
        auth_key: r.auth_key.clone(),
        auth_value: r.auth_value.clone(),
        auth_in_query: r.auth_in_query,
        auth_prefix: r.auth_prefix.clone(),
        description: r.description.clone(),
        path_vars: kv_to_pb(&r.path_vars),
        graphql_variables: r.graphql_variables.clone(),
        binary_path: r.binary_path.clone(),
        tests: r.tests.iter().map(assertion_to_pb).collect(),
    }
}

pub fn composed_from_pb(r: pb::ComposedRequest) -> compose::Request {
    compose::Request {
        body_kind: body_from_pb(r.body_kind),
        auth: auth_from_pb(r.auth),
        id: r.id,
        name: r.name,
        collection: r.collection,
        method: r.method,
        url: r.url,
        params: kv_from_pb(r.params),
        headers: kv_from_pb(r.headers),
        body: r.body,
        form: kv_from_pb(r.form),
        auth_token: r.auth_token,
        auth_user: r.auth_user,
        auth_password: r.auth_password,
        auth_key: r.auth_key,
        auth_value: r.auth_value,
        auth_in_query: r.auth_in_query,
        auth_prefix: r.auth_prefix,
        description: r.description,
        path_vars: kv_from_pb(r.path_vars),
        graphql_variables: r.graphql_variables,
        binary_path: r.binary_path,
        tests: r.tests.into_iter().map(assertion_from_pb).collect(),
        updated_ms: r.updated_ms,
    }
}

pub fn environment_from_pb(e: pb::Environment) -> compose::Environment {
    compose::Environment {
        name: e.name,
        vars: kv_from_pb(e.vars),
    }
}

fn auth_config_to_pb(a: &compose::Auth) -> pb::ComposeAuthConfig {
    pb::ComposeAuthConfig {
        kind: auth_to_pb(a.kind) as i32,
        token: a.token.clone(),
        user: a.user.clone(),
        password: a.password.clone(),
        key: a.key.clone(),
        value: a.value.clone(),
        in_query: a.in_query,
        prefix: a.prefix.clone(),
    }
}

pub fn config_from_pb(c: pb::CollectionConfig) -> compose::CollectionConfig {
    let a = c.auth.unwrap_or_default();
    compose::CollectionConfig {
        name: c.name,
        description: c.description,
        auth: compose::Auth {
            kind: auth_from_pb(a.kind),
            token: a.token,
            user: a.user,
            password: a.password,
            key: a.key,
            value: a.value,
            in_query: a.in_query,
            prefix: a.prefix,
        },
        headers: kv_from_pb(c.headers),
        vars: kv_from_pb(c.vars),
    }
}

pub fn composer_to_pb(s: &compose::State) -> pb::ComposerState {
    pb::ComposerState {
        collections: s.collections.clone(),
        requests: s.requests.iter().map(composed_to_pb).collect(),
        environments: s
            .environments
            .iter()
            .map(|e| pb::Environment {
                name: e.name.clone(),
                vars: kv_to_pb(&e.vars),
            })
            .collect(),
        active_environment: s.active_environment.clone(),
        history: s
            .history
            .iter()
            .map(|h| pb::ComposerHistoryEntry {
                request: Some(composed_to_pb(&h.request)),
                flow_id: h.flow_id,
                status: h.status as u32,
                sent_ms: h.sent_ms,
                duration_ms: h.duration_ms,
                method: h.method.clone(),
                url: h.url.clone(),
                error: h.error.clone(),
            })
            .collect(),
        saved_id: String::new(),
        globals: kv_to_pb(&s.globals),
        configs: s
            .configs
            .iter()
            .map(|c| pb::CollectionConfig {
                name: c.name.clone(),
                description: c.description.clone(),
                auth: Some(auth_config_to_pb(&c.auth)),
                headers: kv_to_pb(&c.headers),
                vars: kv_to_pb(&c.vars),
            })
            .collect(),
        notice: String::new(),
    }
}
