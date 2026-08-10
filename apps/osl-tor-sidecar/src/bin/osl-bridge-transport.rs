//! Managed pluggable transport shipped with OSL's sidecar package.
//!
//! This transport deliberately implements a small, auditable baseline: Arti
//! reaches an unlisted bridge through a managed SOCKS endpoint instead of a
//! public relay. The executable obeys Tor's managed-transport v1 protocol, so
//! it can be replaced by a stronger protocol without changing the sidecar's
//! bridge/config/status contract.

use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const TRANSPORT: &str = "oslbridge";

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("build transport runtime");
    if let Err(error) = runtime.block_on(run()) {
        let _ = writeln!(io::stderr(), "managed bridge transport failed: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    if std::env::var("TOR_PT_MANAGED_TRANSPORT_VER").is_err()
        || std::env::var("TOR_PT_CLIENT_TRANSPORTS")
            .ok()
            .is_none_or(|names| !names.split(',').any(|name| name == TRANSPORT))
    {
        return Err("managed transport environment is missing oslbridge".to_string());
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("listener: {error}"))?;
    let addr = listener.local_addr().map_err(|error| error.to_string())?;
    println!("VERSION 1");
    println!("CMETHOD {TRANSPORT} socks5 {addr}");
    println!("CMETHODS DONE");
    io::stdout().flush().map_err(|error| error.to_string())?;

    loop {
        let (stream, _) = listener.accept().await.map_err(|error| error.to_string())?;
        tokio::spawn(async move {
            let _ = serve(stream).await;
        });
    }
}

async fn serve(mut client: TcpStream) -> Result<(), String> {
    let version = client.read_u8().await.map_err(|error| error.to_string())?;
    let count = client.read_u8().await.map_err(|error| error.to_string())? as usize;
    let mut methods = vec![0u8; count];
    client
        .read_exact(&mut methods)
        .await
        .map_err(|error| error.to_string())?;
    if version != 5 || !methods.contains(&0) {
        client
            .write_all(&[5, 0xff])
            .await
            .map_err(|error| error.to_string())?;
        return Err("SOCKS client did not offer no-auth".to_string());
    }
    client
        .write_all(&[5, 0])
        .await
        .map_err(|error| error.to_string())?;
    let mut header = [0u8; 4];
    client
        .read_exact(&mut header)
        .await
        .map_err(|error| error.to_string())?;
    if header[..3] != [5, 1, 0] {
        return Err("only SOCKS5 CONNECT is supported".to_string());
    }
    let host = match header[3] {
        1 => {
            let mut octets = [0u8; 4];
            client
                .read_exact(&mut octets)
                .await
                .map_err(|error| error.to_string())?;
            IpAddr::V4(Ipv4Addr::from(octets)).to_string()
        }
        3 => {
            let length = client.read_u8().await.map_err(|error| error.to_string())? as usize;
            let mut bytes = vec![0u8; length];
            client
                .read_exact(&mut bytes)
                .await
                .map_err(|error| error.to_string())?;
            String::from_utf8(bytes).map_err(|_| "SOCKS hostname is not UTF-8".to_string())?
        }
        4 => {
            let mut octets = [0u8; 16];
            client
                .read_exact(&mut octets)
                .await
                .map_err(|error| error.to_string())?;
            IpAddr::V6(Ipv6Addr::from(octets)).to_string()
        }
        _ => return Err("unsupported SOCKS address type".to_string()),
    };
    let port = client.read_u16().await.map_err(|error| error.to_string())?;
    let mut upstream = match TcpStream::connect((host.as_str(), port)).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = client.write_all(&[5, 4, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            return Err(format!("bridge connect failed: {error}"));
        }
    };
    let bound = upstream
        .local_addr()
        .unwrap_or_else(|_| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0));
    let mut reply = vec![5, 0, 0];
    match bound.ip() {
        IpAddr::V4(ip) => {
            reply.push(1);
            reply.extend_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            reply.push(4);
            reply.extend_from_slice(&ip.octets());
        }
    }
    reply.extend_from_slice(&bound.port().to_be_bytes());
    client
        .write_all(&reply)
        .await
        .map_err(|error| error.to_string())?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
