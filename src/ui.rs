use std::collections::HashMap;
use std::io::{self, Write};
use crate::checker::ReachabilityResult;
use crate::models::SSTPServer;

#[allow(dead_code)]
pub mod color {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[37m";
}

use color::*;

pub fn print_banner() {
    let banner = r#"
  ______ _____ ____  _   ___     ______  
 |__  / | ____|  _ \| | | \ \   / /  _ \ 
   / /  |  _| | |_) | |_| |\ \ / /| |_) |
  / /_  | |___|  __/|  _  | \ V / |  _ < 
 /____| |_____|_|   |_| |_|  \_/  |_| \_\
"#;
    println!("{}{}{}", CYAN, banner, RESET);
    println!(
        "{}Zephyr — Pure Userspace MS-SSTP Client & Dual SOCKS5/HTTP Proxy{}\n",
        DIM, RESET
    );
}

pub fn print_server_table(
    servers: &[SSTPServer],
    reachability: Option<&HashMap<String, ReachabilityResult>>,
    limit: usize,
) {
    let count = if limit > 0 && limit < servers.len() {
        limit
    } else {
        servers.len()
    };

    println!(
        "{}MS-SSTP Servers (showing {} of {}):{}\n",
        BOLD,
        count,
        servers.len(),
        RESET
    );

    let has_reach = reachability.is_some();

    if has_reach {
        println!(
            "{:<4} {:<19} {:<35} {:<6} {:<15} {:<8} {:<10}",
            "#", "Country", "Endpoint / Host", "Port", "Reachability", "RTT", "Speed"
        );
    } else {
        println!(
            "{:<4} {:<19} {:<35} {:<6} {:<10} {:<10} {:<10}",
            "#", "Country", "Endpoint / Host", "Port", "Ping", "Speed", "Users"
        );
    }

    println!("{}", "─".repeat(110));

    for (i, s) in servers.iter().take(count).enumerate() {
        let flag = s.flag();
        let country_disp = if s.country_long.len() > 14 {
            format!("{}{}…", flag, &s.country_long[..13])
        } else {
            format!("{} {}", flag, s.country_long)
        };

        let host_disp = if s.ddns_hostname.len() > 33 {
            format!("{}…", &s.ddns_hostname[..32])
        } else {
            s.ddns_hostname.clone()
        };

        if let Some(reach_map) = reachability {
            let (reach_badge, rtt_str) = if let Some(r) = reach_map.get(&s.target()) {
                let badge = if r.reachable {
                    format!("{}{}{}", GREEN, r.badge(), RESET)
                } else {
                    format!("{}{}{}", RED, r.badge(), RESET)
                };
                let rtt = if r.reachable {
                    format!("{} ms", r.rtt_ms)
                } else {
                    "timeout".to_string()
                };
                (badge, rtt)
            } else {
                (format!("{}? untested{}", DIM, RESET), "-".to_string())
            };

            println!(
                "{:>2}  {:<19} {:<35} {:<6} {:<31} {:<8} {:>10}",
                i + 1,
                country_disp,
                host_disp,
                s.port,
                reach_badge,
                rtt_str,
                s.speed_formatted()
            );
        } else {
            println!(
                "{:>2}  {:<19} {:<35} {:<6} {:>6} ms {:>10} {:>8}",
                i + 1,
                country_disp,
                host_disp,
                s.port,
                s.ping_ms,
                s.speed_formatted(),
                s.num_sessions
            );
        }
    }

    println!("{}", "─".repeat(110));
}

pub fn print_connection_card(server: &SSTPServer, proxy_listen: &str, assigned_ip: &str) {
    println!(
        "\n{}✔ Userspace MS-SSTP Tunnel Established!{}",
        GREEN, RESET
    );
    println!("═════════════════════════════════════════════════════════════════");
    println!(
        " Remote Server:  {} {} ({}:{})",
        server.flag(),
        server.country_long,
        server.ip,
        server.port
    );
    println!(" Direct IP:      {}:{}", server.ip, server.port);
    println!(" Assigned IP:    {}", assigned_ip);
    println!(" Local Proxy:    {} [Dual SOCKS5 & HTTP]", proxy_listen);
    println!(" Environment:    100% Userspace (No sudo, No OS VPN configs modified)");
    println!("═════════════════════════════════════════════════════════════════");
    println!("Configure your apps (Browser, Telegram, etc.):");
    println!("  • SOCKS5 Proxy:  {}", proxy_listen);
    println!("  • HTTP Proxy:    http://{}", proxy_listen);
    println!(
        "  • Curl test:     curl -x socks5h://{} http://httpbin.org/ip",
        proxy_listen
    );
    println!("─────────────────────────────────────────────────────────────────");
    println!("Streaming active... Press Ctrl+C to disconnect and close port.\n");
}

pub fn prompt_server_selection(servers: &[SSTPServer]) -> Option<SSTPServer> {
    print!(
        "\n{}Enter server number to connect (1-{}), or 'q' to cancel:{} ",
        YELLOW,
        servers.len(),
        RESET
    );
    let _ = io::stdout().flush();

    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        return None;
    }
    let choice = input.trim();
    if choice.eq_ignore_ascii_case("q") || choice.is_empty() {
        return None;
    }

    if let Ok(idx) = choice.parse::<usize>() {
        if idx >= 1 && idx <= servers.len() {
            return Some(servers[idx - 1].clone());
        }
    }

    println!("{}Invalid choice.{}", RED, RESET);
    None
}

pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
