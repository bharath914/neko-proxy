//! SOCKS5 CONNECT on the HTTP proxy port. No authentication or UDP relay.
use crate::{flowctx::AppSlot, Inner};
use std::{
    io,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

async fn reply(io: &mut TcpStream, code: u8) -> io::Result<()> {
    io.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await
}

async fn negotiate(io: &mut TcpStream) -> io::Result<(String, u16)> {
    let mut hello = [0; 2];
    io.read_exact(&mut hello).await?;
    if hello[0] != 5 || hello[1] == 0 {
        return Err(io::Error::other("invalid SOCKS greeting"));
    }
    let mut methods = [0; 255];
    io.read_exact(&mut methods[..hello[1] as usize]).await?;
    if !methods[..hello[1] as usize].contains(&0) {
        io.write_all(&[5, 255]).await?;
        return Err(io::Error::other("SOCKS authentication unsupported"));
    }
    io.write_all(&[5, 0]).await?;
    let mut req = [0; 4];
    io.read_exact(&mut req).await?;
    if req[0] != 5 || req[2] != 0 {
        reply(io, 1).await?;
        return Err(io::Error::other("invalid SOCKS request"));
    }
    if req[1] != 1 {
        reply(io, 7).await?;
        return Err(io::Error::other("only CONNECT supported"));
    }
    let host = match req[3] {
        1 => {
            let mut b = [0; 4];
            io.read_exact(&mut b).await?;
            Ipv4Addr::from(b).to_string()
        }
        4 => {
            let mut b = [0; 16];
            io.read_exact(&mut b).await?;
            Ipv6Addr::from(b).to_string()
        }
        3 => {
            let len = io.read_u8().await? as usize;
            let mut b = [0; 255];
            io.read_exact(&mut b[..len]).await?;
            let host = std::str::from_utf8(&b[..len]).map_err(io::Error::other)?;
            if host.is_empty()
                || !host
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b".-_".contains(&c))
            {
                reply(io, 8).await?;
                return Err(io::Error::other("invalid SOCKS hostname"));
            }
            host.to_ascii_lowercase()
        }
        _ => {
            reply(io, 8).await?;
            return Err(io::Error::other("unsupported address"));
        }
    };
    let port = io.read_u16().await?;
    if port == 0 {
        reply(io, 1).await?;
        return Err(io::Error::other("zero port"));
    }
    reply(io, 0).await?;
    Ok((host, port))
}

impl Inner {
    pub(crate) async fn serve_socks(self: Arc<Self>, mut stream: TcpStream, peer: SocketAddr) {
        if let Ok(Ok((host, port))) =
            tokio::time::timeout(Duration::from_secs(15), negotiate(&mut stream)).await
        {
            let app = AppSlot::lookup(
                self.apps.clone(),
                peer.ip(),
                peer.port(),
                self.cfg.listen.port(),
            );
            self.after_connect(stream, host, port, app, peer).await;
        }
    }
}
