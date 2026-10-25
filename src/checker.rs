use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use crate::models::SSTPServer;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ReachabilityResult {
    pub server: SSTPServer,
    pub reachable: bool,
    pub mode: String,
    pub preferred_endpoint: String,
    pub sni: String,
    pub port: u16,
    pub rtt_ms: u64,
    pub resolved_ip: Option<String>,
}

impl ReachabilityResult {
    pub fn badge(&self) -> &str {
        if !self.reachable {
            "❌ Blocked"
        } else {
            match self.mode.as_str() {
                "IP+Domain" => "✔ IP+Domain",
                "IP only" => "✔ IP only",
                "Domain only" => "✔ Domain only",
                _ => "✔ Reachable",
            }
        }
    }
}

pub async fn check_single_server(server: SSTPServer, timeout: Duration) -> ReachabilityResult {
    let port = server.port;
    let domain = if !server.ddns_hostname.is_empty() {
        server.ddns_hostname.clone()
    } else {
        format!("{}.opengw.net", server.hostname)
    };
    let ip = server.ip.clone();

    // 1. Test direct IP connection
    let mut ip_ok = false;
    let mut ip_rtt = 9999u64;
    let ip_addr_str = format!("{}:{}", ip, port);
    if let Ok(socket_addr) = ip_addr_str.parse::<SocketAddr>() {
        let t0 = Instant::now();
        if let Ok(Ok(_)) = tokio::time::timeout(timeout, TcpStream::connect(socket_addr)).await {
            ip_rtt = t0.elapsed().as_millis() as u64;
            ip_ok = true;
        }
    }

    // 2. Test domain DNS resolution and TCP connection
    let mut dom_ok = false;
    let mut dom_rtt = 9999u64;
    let mut resolved_ip = None;

    let dom_target = format!("{}:{}", domain, port);
    let t1 = Instant::now();
    if let Ok(Ok(mut addrs)) = tokio::time::timeout(timeout, tokio::net::lookup_host(&dom_target)).await {
        if let Some(addr) = addrs.next() {
            let rip = addr.ip().to_string();
            // Detect Iranian DNS poisoning signatures (e.g. 10.10.34.34, 127.0.0.1, 0.0.0.0)
            if rip == "10.10.34.34" || rip == "10.10.34.35" || rip == "127.0.0.1" || rip == "0.0.0.0" {
                dom_ok = false;
            } else {
                resolved_ip = Some(rip);
                if let Ok(Ok(_)) = tokio::time::timeout(timeout, TcpStream::connect(addr)).await {
                    dom_rtt = t1.elapsed().as_millis() as u64;
                    dom_ok = true;
                }
            }
        }
    }

    let (mode, reachable, preferred, rtt) = if ip_ok && dom_ok {
        ("IP+Domain", true, ip, ip_rtt.min(dom_rtt))
    } else if ip_ok {
        ("IP only", true, ip, ip_rtt)
    } else if dom_ok {
        ("Domain only", true, domain.clone(), dom_rtt)
    } else {
        ("Blocked", false, ip, 9999)
    };

    ReachabilityResult {
        server,
        reachable,
        mode: mode.to_string(),
        preferred_endpoint: preferred,
        sni: domain,
        port,
        rtt_ms: rtt,
        resolved_ip,
    }
}

pub async fn check_servers_reachability(
    servers: &[SSTPServer],
    timeout: Duration,
) -> Vec<ReachabilityResult> {
    let mut tasks = Vec::new();

    for s in servers {
        let sc = s.clone();
        tasks.push(tokio::spawn(async move {
            check_single_server(sc, timeout).await
        }));
    }

    let mut results = Vec::new();
    for t in tasks {
        if let Ok(r) = t.await {
            results.push(r);
        }
    }

    results
}
