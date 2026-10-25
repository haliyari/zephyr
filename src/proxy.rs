use std::net::SocketAddr;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use crate::stack::UserspaceStackHandle;

pub struct ProxyMetrics {
    pub bytes_sent: AtomicU64,
    pub bytes_recv: AtomicU64,
    pub active_conns: AtomicI64,
}

impl ProxyMetrics {
    pub fn new() -> Self {
        Self {
            bytes_sent: AtomicU64::new(0),
            bytes_recv: AtomicU64::new(0),
            active_conns: AtomicI64::new(0),
        }
    }
}

pub async fn run_dual_proxy(
    listen_addr: &str,
    stack: UserspaceStackHandle,
    metrics: Arc<ProxyMetrics>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> Result<(), String> {
    let listener = TcpListener::bind(listen_addr)
        .await
        .map_err(|e| format!("Failed to bind proxy listener on {}: {}", listen_addr, e))?;

    loop {
        tokio::select! {
            _ = shutdown_rx.recv() => {
                break;
            }
            res = listener.accept() => {
                match res {
                    Ok((client, _)) => {
                        let stack_clone = UserspaceStackHandle { cmd_tx: stack.cmd_tx.clone() };
                        let m = metrics.clone();
                        tokio::spawn(async move {
                            m.active_conns.fetch_add(1, Ordering::SeqCst);
                            let _ = handle_client(client, stack_clone, m.clone()).await;
                            m.active_conns.fetch_sub(1, Ordering::SeqCst);
                        });
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_client(
    mut client: TcpStream,
    stack: UserspaceStackHandle,
    metrics: Arc<ProxyMetrics>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut first_byte = [0u8; 1];
    client.read_exact(&mut first_byte).await?;

    if first_byte[0] == 0x05 {
        handle_socks5(client, stack, metrics).await
    } else {
        handle_http(client, first_byte[0], stack, metrics).await
    }
}

async fn handle_socks5(
    mut client: TcpStream,
    stack: UserspaceStackHandle,
    metrics: Arc<ProxyMetrics>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut nmethods = [0u8; 1];
    client.read_exact(&mut nmethods).await?;
    let mut methods = vec![0u8; nmethods[0] as usize];
    client.read_exact(&mut methods).await?;

    // No auth required response
    client.write_all(&[0x05, 0x00]).await?;

    let mut header = [0u8; 4];
    client.read_exact(&mut header).await?;
    if header[0] != 0x05 || header[1] != 0x01 {
        return Ok(());
    }

    let target_addr: SocketAddr = match header[3] {
        0x01 => {
            // IPv4
            let mut ip_bytes = [0u8; 4];
            client.read_exact(&mut ip_bytes).await?;
            let mut port_bytes = [0u8; 2];
            client.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);
            SocketAddr::from((ip_bytes, port))
        }
        0x03 => {
            // Domain
            let mut len_byte = [0u8; 1];
            client.read_exact(&mut len_byte).await?;
            let mut domain_bytes = vec![0u8; len_byte[0] as usize];
            client.read_exact(&mut domain_bytes).await?;
            let domain = String::from_utf8_lossy(&domain_bytes).to_string();
            let mut port_bytes = [0u8; 2];
            client.read_exact(&mut port_bytes).await?;
            let port = u16::from_be_bytes(port_bytes);

            let resolved = match tokio::net::lookup_host((domain.as_str(), port)).await {
                Ok(mut addrs) => addrs.next(),
                Err(_) => None,
            };

            if let Some(addr) = resolved {
                addr
            } else {
                client.write_all(&[0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
                return Ok(());
            }
        }
        _ => {
            client.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        }
    };

    let stream = match tokio::time::timeout(std::time::Duration::from_secs(12), stack.dial(target_addr)).await {
        Ok(Ok(s)) => s,
        _ => {
            client.write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;
            return Ok(());
        }
    };

    client.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).await?;

    pipe_proxy(client, stream, metrics).await
}

async fn handle_http(
    mut client: TcpStream,
    first_byte: u8,
    stack: UserspaceStackHandle,
    metrics: Arc<ProxyMetrics>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut header_buf = vec![first_byte];
    let mut b = [0u8; 1];
    while !header_buf.ends_with(b"\r\n\r\n") && header_buf.len() < 8192 {
        if client.read_exact(&mut b).await.is_err() {
            break;
        }
        header_buf.push(b[0]);
    }

    let header_str = String::from_utf8_lossy(&header_buf);
    let mut lines = header_str.lines();
    let first_line = lines.next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(());
    }

    let method = parts[0].to_uppercase();
    let target = parts[1];

    if method == "CONNECT" {
        let lookup = target;
        let dest = if let Ok(mut addrs) = tokio::net::lookup_host(lookup).await {
            if let Some(a) = addrs.next() {
                a
            } else {
                client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await?;
                return Ok(());
            }
        } else {
            client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await?;
            return Ok(());
        };

        let stream = match tokio::time::timeout(std::time::Duration::from_secs(12), stack.dial(dest)).await {
            Ok(Ok(s)) => s,
            _ => {
                client.write_all(b"HTTP/1.1 502 Bad Gateway\r\n\r\n").await?;
                return Ok(());
            }
        };

        client.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
        pipe_proxy(client, stream, metrics).await
    } else {
        // Plain HTTP proxy
        client.write_all(b"HTTP/1.1 501 Not Implemented\r\n\r\n").await?;
        Ok(())
    }
}

async fn pipe_proxy(
    mut client: TcpStream,
    mut stream: crate::stack::ProxyStream,
    metrics: Arc<ProxyMetrics>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut client_read, mut client_write) = client.split();

    let m1 = metrics.clone();
    let to_stack = async move {
        let mut buf = vec![0u8; 8192];
        loop {
            match client_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    m1.bytes_sent.fetch_add(n as u64, Ordering::Relaxed);
                    if stream.tx.send(buf[..n].to_vec()).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    let m2 = metrics.clone();
    let to_client = async move {
        while let Some(chunk) = stream.rx.recv().await {
            m2.bytes_recv.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            if client_write.write_all(&chunk).await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        _ = to_stack => {}
        _ = to_client => {}
    }

    Ok(())
}
