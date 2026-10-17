//! A minimal client for the running core's API socket, for the CLI commands
//! that change live state (`proxyd sdk`, `proxyd devices`).

use anyhow::{anyhow, bail, Context, Result};
use api::pb::{self, client_message::Body as Req, server_message::Body as Res};
use futures_util::{SinkExt, StreamExt};
use prost::Message as _;
use std::path::Path;
use std::time::Duration;
use tokio::net::UnixStream;
use tokio_tungstenite::tungstenite::Message;

pub struct Client {
    ws: tokio_tungstenite::WebSocketStream<UnixStream>,
    next: u64,
}

impl Client {
    pub async fn connect(socket: &Path) -> Result<Client> {
        let stream = UnixStream::connect(socket).await.with_context(|| {
            format!(
                "could not reach the core at {}. Is `proxyd run` (or the desktop app) running?",
                socket.display()
            )
        })?;
        let (ws, _) = tokio_tungstenite::client_async("ws://localhost/", stream)
            .await
            .context("API handshake failed")?;
        Ok(Client { ws, next: 0 })
    }

    pub async fn call(&mut self, body: Req) -> Result<Res> {
        self.next += 1;
        let id = self.next;
        let msg = pb::ClientMessage {
            id,
            body: Some(body),
        };
        self.ws.send(Message::binary(msg.encode_to_vec())).await?;
        loop {
            let frame = tokio::time::timeout(Duration::from_secs(15), self.ws.next())
                .await
                .map_err(|_| anyhow!("the core did not answer"))?
                .ok_or_else(|| anyhow!("the core closed the connection"))??;
            let Message::Binary(b) = frame else { continue };
            let m = pb::ServerMessage::decode(&b[..])?;
            if m.id != id {
                continue;
            }
            return match m.body {
                Some(Res::Error(e)) => bail!("{}: {}", e.code, e.message),
                Some(other) => Ok(other),
                None => bail!("empty answer"),
            };
        }
    }
}
