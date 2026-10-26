use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use base64::prelude::*;
use serde::Deserialize;
use crate::models::SSTPServer;

const DEFAULT_CACHE_TTL_SECS: u64 = 4 * 3600; // 4 hours
const REQUEST_TIMEOUT_SECS: u64 = 6;

#[derive(Debug, Deserialize)]
struct MirrorList {
    #[serde(default)]
    mirrors: Vec<String>,
}

pub fn get_executable_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.to_path_buf();
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn get_cache_file() -> PathBuf {
    get_executable_dir().join("servers.json")
}

pub fn is_cache_valid(ttl_secs: u64) -> bool {
    let file = get_cache_file();
    if !file.exists() {
        return false;
    }
    if let Ok(metadata) = fs::metadata(&file) {
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                return elapsed.as_secs() < ttl_secs;
            }
        }
    }
    false
}

pub fn load_from_cache() -> Option<Vec<SSTPServer>> {
    let file = get_cache_file();
    if !file.exists() {
        return None;
    }
    if let Ok(content) = fs::read_to_string(file) {
        if let Ok(servers) = serde_json::from_str::<Vec<SSTPServer>>(&content) {
            if !servers.is_empty() {
                return Some(servers);
            }
        }
    }
    None
}

pub fn save_to_cache(servers: &[SSTPServer]) {
    let file = get_cache_file();
    if let Ok(content) = serde_json::to_string_pretty(servers) {
        let _ = fs::write(file, content);
    }
}

/// Tier 1: Try to fetch pre-built, lightweight JSON (~30 KB) directly from GitHub CDN
fn fetch_servers_json() -> Option<Vec<SSTPServer>> {
    let endpoints = [
        "https://cdn.jsdelivr.net/gh/haliyari/zephyr@data/servers.json",
        "https://fastly.jsdelivr.net/gh/haliyari/zephyr@data/servers.json",
        "https://raw.githubusercontent.com/haliyari/zephyr/data/servers.json",
    ];

    for url in endpoints {
        let resp = ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 Zephyr/0.1.0")
            .set("Accept", "application/json")
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .call();

        if let Ok(r) = resp {
            if let Ok(body) = r.into_string() {
                if let Ok(servers) = serde_json::from_str::<Vec<SSTPServer>>(&body) {
                    if !servers.is_empty() {
                        return Some(servers);
                    }
                }
            }
        }
    }
    None
}

/// Tier 2: Try to fetch live mirror endpoints from GitHub CDN
fn fetch_mirrors_list() -> Vec<String> {
    let endpoints = [
        "https://cdn.jsdelivr.net/gh/haliyari/zephyr@data/mirrors.json",
        "https://fastly.jsdelivr.net/gh/haliyari/zephyr@data/mirrors.json",
        "https://raw.githubusercontent.com/haliyari/zephyr/data/mirrors.json",
    ];

    for url in endpoints {
        let resp = ureq::get(url)
            .set("User-Agent", "Mozilla/5.0 Zephyr/0.1.0")
            .set("Accept", "application/json")
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .call();

        if let Ok(r) = resp {
            if let Ok(body) = r.into_string() {
                if let Ok(data) = serde_json::from_str::<MirrorList>(&body) {
                    if !data.mirrors.is_empty() {
                        return data.mirrors;
                    }
                }
            }
        }
    }
    Vec::new()
}

pub fn fetch_csv_data(mirrors: &[String]) -> Option<String> {
    let mut urls = Vec::new();

    // Prioritize mirror direct IP endpoints (bypasses DNS and SNI filtering)
    for m in mirrors {
        let base = m.trim().trim_end_matches('/');
        urls.push(format!("{}/api/iphone/", base));
    }

    // Direct official endpoints as fallback
    urls.push("http://www.vpngate.net/api/iphone/".to_string());
    urls.push("http://vpngate.net/api/iphone/".to_string());
    urls.push("https://www.vpngate.net/api/iphone/".to_string());
    urls.push("https://vpngate.net/api/iphone/".to_string());

    for url in urls {
        let resp = ureq::get(&url)
            .set("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36")
            .set("Accept", "*/*")
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .call();

        if let Ok(r) = resp {
            if let Ok(body) = r.into_string() {
                if body.contains("*vpn_servers") {
                    return Some(body);
                }
            }
        }
    }
    None
}

pub fn parse_csv_servers(csv_text: &str) -> Vec<SSTPServer> {
    let mut servers = Vec::new();

    for line in csv_text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('*') || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 15 {
            continue;
        }

        let hostname = parts[0].to_string();
        let ip = parts[1].to_string();
        let score = parts[2].parse::<u64>().unwrap_or(0);
        let ping_ms = parts[3].parse::<u32>().unwrap_or(0);
        let speed_bps = parts[4].parse::<u64>().unwrap_or(0);
        let country_long = parts[5].to_string();
        let country_short = parts[6].to_string();
        let num_sessions = parts[7].parse::<u32>().unwrap_or(0);
        let uptime_sec = parts[8].parse::<u64>().unwrap_or(0);
        let total_users = parts[9].parse::<u64>().unwrap_or(0);
        let operator = if parts.len() > 12 {
            parts[12].replace("'s owner", "").trim().to_string()
        } else {
            String::new()
        };
        let ovpn_b64 = parts[14];

        // SoftEther runs SSTP on the same TCP port as OpenVPN TCP
        let mut port: Option<u16> = None;
        if let Ok(decoded) = BASE64_STANDARD.decode(ovpn_b64.trim()) {
            if let Ok(ovpn_text) = String::from_utf8(decoded) {
                if ovpn_text.contains("proto tcp") {
                    for ovpn_line in ovpn_text.lines() {
                        let trimmed = ovpn_line.trim();
                        if trimmed.starts_with("remote ") {
                            let rparts: Vec<&str> = trimmed.split_whitespace().collect();
                            if rparts.len() >= 3 {
                                if let Ok(p) = rparts[2].parse::<u16>() {
                                    port = Some(p);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(p) = port {
            let ddns_hostname = format!("{}.opengw.net", hostname);
            servers.push(SSTPServer {
                hostname,
                ddns_hostname,
                ip,
                port: p,
                country_long,
                country_short,
                ping_ms,
                speed_bps,
                score,
                num_sessions,
                uptime_sec,
                total_users,
                operator,
            });
        }
    }

    servers
}

pub fn get_sstp_servers(force_refresh: bool) -> Vec<SSTPServer> {
    if !force_refresh && is_cache_valid(DEFAULT_CACHE_TTL_SECS) {
        if let Some(cached) = load_from_cache() {
            return cached;
        }
    }

    // Tier 1: Try GitHub CDN pre-parsed, optimized JSON (~30 KB)
    if let Some(servers) = fetch_servers_json() {
        save_to_cache(&servers);
        return servers;
    }

    // Tier 2: Fetch mirror list and query active mirrors for CSV
    let mirrors = fetch_mirrors_list();
    if let Some(csv) = fetch_csv_data(&mirrors) {
        let servers = parse_csv_servers(&csv);
        if !servers.is_empty() {
            save_to_cache(&servers);
            return servers;
        }
    }

    // Tier 3: Direct official fallback without mirrors
    if mirrors.is_empty() {
        if let Some(csv) = fetch_csv_data(&[]) {
            let servers = parse_csv_servers(&csv);
            if !servers.is_empty() {
                save_to_cache(&servers);
                return servers;
            }
        }
    }

    // Tier 4: Fallback to expired cache if network failed
    if let Some(cached) = load_from_cache() {
        return cached;
    }

    Vec::new()
}

pub fn filter_and_sort_servers(
    mut servers: Vec<SSTPServer>,
    country: Option<&str>,
    max_ping: Option<u32>,
    min_speed: Option<u64>,
    sort_by: &str,
) -> Vec<SSTPServer> {
    if let Some(c) = country {
        let cl = c.trim().to_lowercase();
        servers.retain(|s| {
            s.country_short.to_lowercase() == cl || s.country_long.to_lowercase().contains(&cl)
        });
    }

    if let Some(mp) = max_ping {
        servers.retain(|s| s.ping_ms <= mp);
    }

    if let Some(ms) = min_speed {
        servers.retain(|s| s.speed_bps >= ms);
    }

    match sort_by {
        "speed" => servers.sort_by(|a, b| b.speed_bps.cmp(&a.speed_bps)),
        "ping" => servers.sort_by(|a, b| a.ping_ms.cmp(&b.ping_ms)),
        "users" => servers.sort_by(|a, b| b.num_sessions.cmp(&a.num_sessions)),
        _ => servers.sort_by(|a, b| b.score.cmp(&a.score)),
    }

    servers
}
