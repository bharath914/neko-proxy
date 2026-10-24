//! Everything that turns captured bytes into something a person can read.
//! The UI never parses: it asks for a [`BodyView`] and renders it.

mod codec;
mod curl;
mod html;
mod json;
mod kind;
pub mod redact;
mod view;
mod sse;
pub use sse::{parse_sse, SseEvent};

pub use codec::{decode_content, Decoded, DEFAULT_DECODE_LIMIT};
pub use curl::curl_command;
pub use html::{
    html_view, looks_like_html, Block as HtmlBlock, BlockKind as HtmlBlockKind, HtmlView,
    SourceKind, SourceLine, SourceToken, Span as HtmlSpan,
};
pub use json::{json_outline, JsonLine, JsonToken, JsonTokenKind};
pub use kind::{classify, parse_form, sniff_image, Kind};
pub use view::{build_view, BodyView, ViewOptions};
