mod checker;
mod fetcher;
mod models;
mod proxy;
mod sstp;
mod stack;
mod ui;

use clap::{Args, Parser, Subcommand};
use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use checker::{check_servers_reachability, ReachabilityResult};
use fetcher::{filter_and_sort_servers, get_sstp_servers, is_cache_valid};
use models::SSTPServer;
use ui::color::*;
use ui::*;

#[derive(Parser)]
#[command(
    name = "zephyr",
    author = "Hossein Aliyari",
    version = env!("CARGO_PKG_VERSION"),
    about = "Zephyr — Userspace MS-SSTP Client & Dual SOCKS5/HTTP Proxy (100% Pure Rust)",
    long_about = "Connect to MS-SSTP VPN servers in 100% userspace without sudo or modifying OS VPN configurations."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(name = "list", alias = "ls", about = "List MS-SSTP servers")]
    List(ListArgs),

    #[command(name = "check", about = "Check server reachability (Domain & IP) from Iran")]
    Check(CheckArgs),

    #[command(name = "connect", alias = "c", about = "Connect to an MS-SSTP server in userspace")]
    Connect(ConnectArgs),

    #[command(name = "disconnect", alias = "stop", about = "Disconnect active session")]
    Disconnect,

    #[command(name = "status", alias = "info", about = "Show proxy listen status")]
    Status(StatusArgs),
}

#[derive(Args)]
struct ListArgs {
    #[arg(short, long, help = "Filter by country (e.g. JP, US, KR)")]
    country: Option<String>,

    #[arg(short, long, default_value_t = 20, help = "Max servers to display")]
    limit: usize,

    #[arg(short, long, default_value = "score", help = "Sort by: score, speed, ping, users")]
    sort: String,

    #[arg(long, help = "Test reachability from your network (Iran)")]
    check: bool,

    #[arg(short, long, help = "Force refresh server list from VPNGate")]
    refresh: bool,

    #[arg(long, help = "Output as raw JSON")]
    json: bool,
}

#[derive(Args)]
struct CheckArgs {
    #[arg(short, long, default_value_t = 15, help = "Number of servers to test")]
    limit: usize,

    #[arg(short, long, help = "Filter by country (e.g. JP, US, KR)")]
    country: Option<String>,

    #[arg(short, long, default_value_t = 2.0, help = "Per-server timeout in seconds")]
    timeout: f64,

    #[arg(long, help = "Show only reachable servers")]
    only_reachable: bool,

    #[arg(short, long, help = "Force refresh server list from VPNGate")]
    refresh: bool,
}

#[derive(Args)]
struct ConnectArgs {
    #[arg(help = "Target server index (from list) or hostname:port / IP:port")]
    target: Option<String>,

    #[arg(short, long, help = "Automatically pick lowest-latency reachable server")]
    auto: bool,

    #[arg(short, long, default_value = "127.0.0.1:1095", help = "Proxy listen address")]
    listen: String,

    #[arg(short, long, default_value_t = 443, help = "Default SSTP port")]
    port: u16,

    #[arg(short, long, help = "Filter by country (e.g. JP, US, KR)")]
    country: Option<String>,

    #[arg(short, long, help = "Force refresh server list from VPNGate")]
    refresh: bool,
}

#[derive(Args)]
struct StatusArgs {
    #[arg(short, long, default_value = "127.0.0.1:1095", help = "Proxy listen address")]
    listen: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::List(args) => handle_list(args).await,
        Commands::Check(args) => handle_check(args).await,
        Commands::Connect(args) => handle_connect(args).await,
        Commands::Disconnect => handle_disconnect(),
        Commands::Status(args) => handle_status(args).await,
    }
}

async fn handle_list(args: ListArgs) {
    print_banner();

    if args.refresh || !is_cache_valid(4 * 3600) {
        println!("{}Downloading fresh server list from VPNGate...{}", DIM, RESET);
    } else {
        println!("{}Loading server list from cache...{}", DIM, RESET);
    }

    let servers = get_sstp_servers(args.refresh);
    let mut filtered = filter_and_sort_servers(
        servers,
        args.country.as_deref(),
        None,
        None,
        &args.sort,
    );

    let mut reach_map: Option<HashMap<String, ReachabilityResult>> = None;

    if args.check {
        let check_count = if args.limit > 0 { args.limit } else { 20 };
        let candidates: Vec<SSTPServer> = filtered.iter().take(check_count).cloned().collect();
        println!(
            "{}Testing reachability of {} servers from your network (Iran)...{}",
            CYAN,
            candidates.len(),
            RESET
        );

        let results = check_servers_reachability(&candidates, Duration::from_secs_f64(2.0)).await;
        let mut map = HashMap::new();
        for r in results {
            map.insert(r.server.target(), r);
        }

        // Re-sort filtered so reachable ones appear first
        filtered.sort_by_key(|s| {
            if let Some(r) = map.get(&s.target()) {
                if r.reachable { (0, r.rtt_ms) } else { (1, 9999) }
            } else {
                (2, 9999)
            }
        });

        reach_map = Some(map);
    }

    if args.json {
        if let Ok(json) = serde_json::to_string_pretty(&filtered) {
            println!("{}", json);
        }
        return;
    }

    print_server_table(&filtered, reach_map.as_ref(), args.limit);
}

async fn handle_check(args: CheckArgs) {
    print_banner();

    if args.refresh || !is_cache_valid(4 * 3600) {
        println!("{}Downloading fresh server list from VPNGate...{}", DIM, RESET);
    } else {
        println!("{}Loading server list from cache...{}", DIM, RESET);
    }

    let servers = get_sstp_servers(args.refresh);
    let filtered = filter_and_sort_servers(
        servers,
        args.country.as_deref(),
        None,
        None,
        "score",
    );

    let candidates: Vec<SSTPServer> = filtered.iter().take(args.limit).cloned().collect();
    println!(
        "{}Checking domain & IP reachability for {} servers from Iran...{}",
        CYAN,
        candidates.len(),
        RESET
    );

    let results = check_servers_reachability(&candidates, Duration::from_secs_f64(args.timeout)).await;
    let mut map = HashMap::new();
    let mut display_servers = Vec::new();

    for r in results {
        if !args.only_reachable || r.reachable {
            display_servers.push(r.server.clone());
        }
        map.insert(r.server.target(), r);
    }

    display_servers.sort_by_key(|s| {
        if let Some(r) = map.get(&s.target()) {
            if r.reachable { (0, r.rtt_ms) } else { (1, 9999) }
        } else {
            (2, 9999)
        }
    });

    print_server_table(&display_servers, Some(&map), args.limit);
}

async fn handle_connect(args: ConnectArgs) {
    print_banner();

    if args.refresh || !is_cache_valid(4 * 3600) {
        println!("{}Downloading fresh server list from VPNGate...{}", DIM, RESET);
    } else {
        println!("{}Loading server list from cache...{}", DIM, RESET);
    }

    let servers = get_sstp_servers(args.refresh);
    let filtered = filter_and_sort_servers(
        servers,
        args.country.as_deref(),
        None,
        None,
        "score",
    );

    let chosen_server: SSTPServer;
    let mut preferred_endpoint: Option<String> = None;
    let mut tls_sni: Option<String> = None;

    if let Some(ref target) = args.target {
        let t = target.trim();
        if let Ok(idx) = t.parse::<usize>() {
            if idx >= 1 && idx <= filtered.len() {
                chosen_server = filtered[idx - 1].clone();
            } else {
                eprintln!(
                    "{}Error: Server index {} out of range (1-{}).{}",
                    RED,
                    idx,
                    filtered.len(),
                    RESET
                );
                return;
            }
        } else {
            let (host, p) = if t.contains(':') {
                let parts: Vec<&str> = t.split(':').collect();
                (parts[0].to_string(), parts[1].parse::<u16>().unwrap_or(args.port))
            } else {
                (t.to_string(), args.port)
            };

            chosen_server = SSTPServer {
                hostname: host.split('.').next().unwrap_or("custom").to_string(),
                ddns_hostname: host.clone(),
                ip: host,
                port: p,
                country_long: "Custom".to_string(),
                country_short: "??".to_string(),
                ping_ms: 0,
                speed_bps: 0,
                score: 0,
                num_sessions: 0,
                uptime_sec: 0,
                total_users: 0,
                operator: "Manual entry".to_string(),
            };
        }
    } else {
        let candidate_count = if args.auto { 8 } else { 15 };
        let candidates: Vec<SSTPServer> = filtered.iter().take(candidate_count).cloned().collect();

        if args.auto {
            println!(
                "{}Checking reachability of top servers from your network (Iran)...{}",
                CYAN, RESET
            );
        } else {
            println!(
                "{}Checking reachability of servers from your network (Iran)...{}",
                CYAN, RESET
            );
        }

        let results = check_servers_reachability(&candidates, Duration::from_secs_f64(2.0)).await;
        let mut map = HashMap::new();
        for r in &results {
            map.insert(r.server.target(), r.clone());
        }

        let mut ordered_servers = Vec::new();
        for r in &results {
            ordered_servers.push(r.server.clone());
        }

        if args.auto {
            let reachable: Vec<&ReachabilityResult> = results.iter().filter(|r| r.reachable).collect();
            if reachable.is_empty() {
                eprintln!(
                    "{}No reachable SSTP servers found from your connection. Try another country or pass custom target.{}",
                    RED, RESET
                );
                return;
            }
            let best = reachable[0];
            chosen_server = best.server.clone();
            preferred_endpoint = Some(best.preferred_endpoint.clone());
            tls_sni = Some(best.sni.clone());
        } else {
            print_server_table(&ordered_servers, Some(&map), 20);
            if let Some(srv) = prompt_server_selection(&ordered_servers) {
                chosen_server = srv;
            } else {
                println!("{}Connection cancelled.{}", YELLOW, RESET);
                return;
            }
        }
    }

    let server = chosen_server;
    let dial_target = preferred_endpoint.unwrap_or_else(|| server.ip.clone());
    let sni = tls_sni.unwrap_or_else(|| {
        if !server.ddns_hostname.is_empty() {
            server.ddns_hostname.clone()
        } else {
            server.ip.clone()
        }
    });

    println!(
        "\nConnecting to MS-SSTP Server: {} {} ({}:{})...",
        server.flag(),
        server.country_long,
        dial_target,
        server.port
    );
    println!("(Using 100% pure Rust userspace engine — zero sudo, zero root, zero OS VPN configs)");

    match sstp::connect_sstp(&dial_target, server.port, &sni, "vpn", "vpn").await {
        Ok(session) => {
            let assigned_ip_str = format!("{}.{}.{}.{}", session.assigned_ip[0], session.assigned_ip[1], session.assigned_ip[2], session.assigned_ip[3]);
            print_connection_card(&server, &args.listen, &assigned_ip_str);

            let shutdown_tx = session.shutdown_tx.clone();
            let stack_shutdown_rx = shutdown_tx.subscribe();
            let proxy_shutdown_rx = shutdown_tx.subscribe();

            // Spawn smoltcp userspace stack
            let stack_handle = stack::spawn_userspace_stack(
                session.assigned_ip,
                session.gateway_ip,
                session.rx_packets,
                session.tx_packets,
                stack_shutdown_rx,
            );

            // Spawn SOCKS5 / HTTP proxy
            let metrics = Arc::new(proxy::ProxyMetrics::new());
            let m_clone = metrics.clone();
            let listen_addr = args.listen.clone();

            tokio::spawn(async move {
                let _ = proxy::run_dual_proxy(&listen_addr, stack_handle, m_clone, proxy_shutdown_rx).await;
            });

            // Wait for Ctrl+C or connection abort
            let mut ticker = tokio::time::interval(Duration::from_secs(2));
            ticker.tick().await;

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("\n{}Received Ctrl+C. Shutting down MS-SSTP tunnel...{}", YELLOW, RESET);
                        let _ = shutdown_tx.send(());
                        tokio::time::sleep(Duration::from_millis(150)).await;
                        println!("{}Disconnected. Local proxy port {} closed.{}", GREEN, args.listen, RESET);
                        break;
                    }
                    _ = ticker.tick() => {
                        let sent = metrics.bytes_sent.load(std::sync::atomic::Ordering::Relaxed);
                        let recv = metrics.bytes_recv.load(std::sync::atomic::Ordering::Relaxed);
                        let active = metrics.active_conns.load(std::sync::atomic::Ordering::Relaxed);
                        let now = chrono_or_time();
                        print!(
                            "\r[{}] Active: {} conns | Sent: {} | Recv: {}    ",
                            now,
                            active,
                            format_bytes(sent),
                            format_bytes(recv),
                        );
                        let _ = std::io::stdout().flush();
                    }
                }
            }
        }
        Err(err) => {
            eprintln!("\n{}Connection failed: {}{}", RED, err, RESET);
        }
    }
}

fn handle_disconnect() {
    print_banner();
    println!("In pure userspace mode, stopping the running terminal session (Ctrl+C) closes the connection.");
}

async fn handle_status(args: StatusArgs) {
    print_banner();
    let is_open = if let Ok(addr) = args.listen.parse::<SocketAddr>() {
        tokio::net::TcpStream::connect(addr).await.is_ok()
    } else {
        false
    };

    if is_open {
        println!(
            "{}Status: RUNNING (Proxy active on {}){}",
            GREEN, args.listen, RESET
        );
    } else {
        println!(
            "{}Status: STOPPED (Port {} is closed){}",
            DIM, args.listen, RESET
        );
    }
}

fn chrono_or_time() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now();
    if let Ok(duration) = now.duration_since(SystemTime::UNIX_EPOCH) {
        let secs = duration.as_secs() % 86400;
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        "00:00:00".to_string()
    }
}
