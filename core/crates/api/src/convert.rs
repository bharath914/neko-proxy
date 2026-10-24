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
            events: v.events.into_iter().map(|e| pb::SseEvent {
                event: e.event, id: e.id, data: e.data, retry_ms: e.retry_ms,
            }).collect(),
            protocol: Some(pb::ProtocolView { title: v.protocol.title, note: v.protocol.note,
                fields: v.protocol.fields.into_iter().map(|f| pb::DecodedField { depth: f.depth, label: f.label, value: f.value }).collect(),
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
