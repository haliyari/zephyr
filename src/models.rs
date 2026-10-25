use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SSTPServer {
    pub hostname: String,
    pub ddns_hostname: String,
    pub ip: String,
    pub port: u16,
    pub country_long: String,
    pub country_short: String,
    pub ping_ms: u32,
    pub speed_bps: u64,
    pub score: u64,
    pub num_sessions: u32,
    pub uptime_sec: u64,
    pub total_users: u64,
    pub operator: String,
}

impl SSTPServer {
    pub fn target(&self) -> String {
        format!("{}:{}", self.endpoint(), self.port)
    }

    pub fn endpoint(&self) -> &str {
        if !self.ddns_hostname.is_empty() {
            &self.ddns_hostname
        } else {
            &self.ip
        }
    }

    pub fn flag(&self) -> String {
        country_to_flag(&self.country_short)
    }

    pub fn speed_formatted(&self) -> String {
        let mbps = (self.speed_bps as f64) / 1_000_000.0;
        format!("{:.1} Mbps", mbps)
    }
}

pub fn country_to_flag(code: &str) -> String {
    let code = code.trim().to_uppercase();
    if code.len() != 2 {
        return "🌐".to_string();
    }
    let chars: Vec<char> = code.chars().collect();
    if chars[0] < 'A' || chars[0] > 'Z' || chars[1] < 'A' || chars[1] > 'Z' {
        return "🌐".to_string();
    }
    let c1 = std::char::from_u32(0x1F1E6 + (chars[0] as u32 - 'A' as u32)).unwrap_or('🌐');
    let c2 = std::char::from_u32(0x1F1E6 + (chars[1] as u32 - 'A' as u32)).unwrap_or('🌐');
    format!("{}{}", c1, c2)
}
