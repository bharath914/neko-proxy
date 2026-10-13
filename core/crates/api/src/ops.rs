//! Operations shared by the client API and the MCP server, so both behave
//! the same way.

use crate::{ApiContext, ReplayCall};
use flow::flags;

/// What may change when a stored flow is sent again.
#[derive(Clone, Debug, Default)]
pub struct ReplayEdits {
    pub method: Option<String>,
    pub url: Option<String>,
    /// Replaces every header.
    pub headers: Option<Vec<(String, String)>>,
    pub body: Option<bytes::Bytes>,
}

pub struct Replayed {
    pub flow_id: u64,
    pub status: u16,
}

/// Send a stored flow again, with edits, as a new flow.
pub async fn replay_flow(
    ctx: &ApiContext,
    id: u64,
    edits: ReplayEdits,
) -> Result<Replayed, String> {
    let store = ctx.store.clone();
    let rec = tokio::task::spawn_blocking(move || store.get(id))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no flow {id}"))?;
    if rec.flags & (flags::TUNNEL | flags::PINNED) != 0 {
        return Err("this flow is an encrypted tunnel; there is no request to send".into());
    }
    let body_edited = edits.body.is_some();
    let mut headers = edits.headers.unwrap_or_else(|| rec.req_headers.clone());
    let body = edits.body.unwrap_or_else(|| rec.req_body.clone());
    if body_edited {
        // The stored headers describe the old bytes.
        headers.retain(|(n, _)| {
            !n.eq_ignore_ascii_case("content-length") && !n.eq_ignore_ascii_case("content-encoding")
        });
    }
    if rec.flags & flags::REQ_TRUNCATED != 0 && !body_edited {
        return Err("the captured request body was cut at the capture limit; edit the body or replay a smaller request".into());
    }
    let out = ctx
        .control
        .replay(ReplayCall {
            method: edits.method.unwrap_or_else(|| rec.method.clone()),
            url: edits.url.unwrap_or_else(|| rec.url()),
            headers,
            body,
        })
        .await?;
    Ok(Replayed {
        flow_id: out.0,
        status: out.1,
    })
}
