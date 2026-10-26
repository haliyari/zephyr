#!/usr/bin/env python3
"""
Sync VPNGate Servers and Mirrors
Scrapes active mirror URLs from https://www.vpngate.net/en/sites.aspx
and fetches the server list, parsing it into a compact servers.json for Zephyr.
"""

import sys
import os
import re
import json
import base64
import urllib.request
import datetime

DEFAULT_OUT_DIR = "/tmp/vpngate_out"

def fetch_mirrors():
    urls = [
        "https://www.vpngate.net/en/sites.aspx",
        "http://www.vpngate.net/en/sites.aspx",
    ]
    headers = {
        "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    }
    mirrors = []
    for url in urls:
        try:
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req, timeout=15) as res:
                html = res.read().decode("utf-8", errors="ignore")
                matches = re.findall(r"http://\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}:\d+/", html)
                if matches:
                    mirrors = sorted(list(set(matches)))
                    print(f"[+] Found {len(mirrors)} active mirrors from {url}")
                    break
        except Exception as e:
            print(f"[-] Failed to fetch mirrors from {url}: {e}")
    return mirrors

def parse_ovpn_tcp_port(b64_data):
    try:
        decoded = base64.b64decode(b64_data.strip()).decode("utf-8", errors="ignore")
        if "proto tcp" in decoded:
            for line in decoded.splitlines():
                line = line.strip()
                if line.startswith("remote "):
                    parts = line.split()
                    if len(parts) >= 3 and parts[2].isdigit():
                        return int(parts[2])
    except Exception:
        pass
    return None

def fetch_csv(mirrors):
    candidate_urls = [
        "http://www.vpngate.net/api/iphone/",
        "https://www.vpngate.net/api/iphone/",
        "http://vpngate.net/api/iphone/",
    ]
    for m in mirrors:
        candidate_urls.append(m.rstrip("/") + "/api/iphone/")

    headers = {
        "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
        "Accept": "*/*",
    }

    for url in candidate_urls:
        try:
            req = urllib.request.Request(url, headers=headers)
            with urllib.request.urlopen(req, timeout=12) as res:
                text = res.read().decode("utf-8", errors="ignore")
                if "*vpn_servers" in text:
                    print(f"[+] Successfully fetched CSV server list from: {url}")
                    return text
        except Exception as e:
            print(f"[-] Failed to fetch CSV from {url}: {e}")
    return None

def parse_servers(csv_text):
    servers = []
    for line in csv_text.splitlines():
        line = line.strip()
        if not line or line.startswith("*") or line.startswith("#"):
            continue
        parts = line.split(",")
        if len(parts) < 15:
            continue

        port = parse_ovpn_tcp_port(parts[14])
        if port:
            hostname = parts[0]
            servers.append({
                "hostname": hostname,
                "ddns_hostname": f"{hostname}.opengw.net",
                "ip": parts[1],
                "port": port,
                "country_long": parts[5],
                "country_short": parts[6],
                "ping_ms": int(parts[3]) if parts[3].isdigit() else 0,
                "speed_bps": int(parts[4]) if parts[4].isdigit() else 0,
                "score": int(parts[2]) if parts[2].isdigit() else 0,
                "num_sessions": int(parts[7]) if parts[7].isdigit() else 0,
                "uptime_sec": int(parts[8]) if parts[8].isdigit() else 0,
                "total_users": int(parts[9]) if parts[9].isdigit() else 0,
                "operator": parts[12].replace("'s owner", "").strip() if len(parts) > 12 else "",
            })

    # Sort descending by score
    servers.sort(key=lambda s: s["score"], reverse=True)
    return servers

def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_OUT_DIR
    os.makedirs(out_dir, exist_ok=True)

    print("[*] Fetching mirror sites...")
    mirrors = fetch_mirrors()

    mirrors_data = {
        "updated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "count": len(mirrors),
        "mirrors": mirrors,
    }

    mirrors_path = os.path.join(out_dir, "mirrors.json")
    with open(mirrors_path, "w", encoding="utf-8") as f:
        json.dump(mirrors_data, f, indent=2, ensure_ascii=False)
    print(f"[+] Saved {mirrors_path}")

    print("[*] Fetching server list CSV...")
    csv_text = fetch_csv(mirrors)
    if not csv_text:
        print("[!] Error: Could not fetch CSV from any endpoint.")
        sys.exit(1)

    servers = parse_servers(csv_text)
    print(f"[+] Parsed {len(servers)} SSTP servers.")
    if len(servers) < 5:
        print("[!] Error: Too few servers parsed, aborting to prevent corrupted cache.")
        sys.exit(1)

    servers_path = os.path.join(out_dir, "servers.json")
    with open(servers_path, "w", encoding="utf-8") as f:
        json.dump(servers, f, indent=2, ensure_ascii=False)
    print(f"[+] Saved {servers_path}")

    print("[*] Sync completed successfully!")

if __name__ == "__main__":
    main()
