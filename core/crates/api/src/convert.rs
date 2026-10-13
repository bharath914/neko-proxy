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
            image: v.image,
            note: v.note,
        }
    }
}
