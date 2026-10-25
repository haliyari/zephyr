# Zephyr - Technical Documentation

A standalone, pure 100% Rust userspace MS-SSTP client with a built-in local dual SOCKS5 & HTTP CONNECT proxy (`127.0.0.1:1095`).

---

## Table of Contents

- [Overview](#overview)
- [How It Works (Architecture)](#how-it-works-architecture)
- [Features](#features)
- [Installation](#installation)
  - [Prebuilt Binaries](#prebuilt-binaries)
  - [Termux (Android) One-Line Install](#termux-android-one-line-install)
  - [Linux / macOS One-Line Install](#linux--macos-one-line-install)
  - [Building from Source](#building-from-source)
- [Command Reference](#command-reference)
- [Application Proxy Configuration](#application-proxy-configuration)
  - [Web Browsers (Firefox, Chrome)](#web-browsers-firefox-chrome)
  - [Telegram Desktop / Android](#telegram-desktop--android)
  - [Terminal & CLI (curl, git)](#terminal--cli-curl-git)
- [Censorship Circumvention & Iran Reachability](#censorship-circumvention--iran-reachability)
- [Troubleshooting & FAQ](#troubleshooting--faq)

---

## Overview

Traditional MS-SSTP clients on Linux require:
- Kernel TUN/TAP permissions (`root` / `sudo`)
- Modifying OS network routing tables and `/etc/resolv.conf`
- Heavy dependencies (such as PPP daemon, Python, or Go runtimes)

**Zephyr** eliminates all of these hurdles:
- **Zero sudo / zero root**: Runs entirely in userspace.
- **Zero OS VPN modifications**: Leaves your system network settings untouched.
- **Single static binary**: ~2.9 MB, zero runtime dependencies.
- **Built-in Proxy**: Exposes `127.0.0.1:1095` as both SOCKS5 and HTTP CONNECT.

---

## How It Works (Architecture)

```
+-----------------------------------------------------------------------+
|  Applications (Browser, Telegram, Curl, etc.)                         |
+-----------------------------------------------------------------------+
                                   |
                      SOCKS5 / HTTP CONNECT
                                   v
+-----------------------------------------------------------------------+
|  Local Proxy Listener (127.0.0.1:1095)                                |
+-----------------------------------------------------------------------+
                                   |
                         TCP Stream Intercept
                                   v
+-----------------------------------------------------------------------+
|  Userspace TCP/IP Stack (smoltcp)                                     |
|  - Manages TCP sequence numbers, windowing, packet assembly           |
|  - Encapsulates payload into raw IPv4 packets                         |
+-----------------------------------------------------------------------+
                                   |
                       PPP Framing (Proto 0x0021)
                                   v
+-----------------------------------------------------------------------+
|  MS-SSTP Engine (tokio-rustls)                                        |
|  - SSTP Duplex HTTP/1.1 Handshake                                     |
|  - PPP LCP & PAP Authentication                                       |
|  - CryptoBinding (HMAC-SHA256)                                        |
|  - PPP IPCP Address Negotiation                                       |
|  - Echo Keepalive Heartbeats                                          |
+-----------------------------------------------------------------------+
                                   |
                         TLS 1.2 / 1.3 Tunnel
                                   v
+-----------------------------------------------------------------------+
|  MS-SSTP Relay Server (Japan, US, Korea, etc.)                        |
+-----------------------------------------------------------------------+
```

---

## Installation

### Prebuilt Binaries

Prebuilt binaries are published for every release on the [GitHub Releases](https://github.com/haliyari/zephyr/releases) page:

| Platform | Target Architecture | Release Archive |
|---|---|---|
| **Android (Termux)** | ARM 64-bit (`aarch64`) | `zephyr-android-arm64.tar.gz` |
| **Android (Termux)** | ARM 32-bit (`armv7`) | `zephyr-android-armv7.tar.gz` |
| **Android (Termux)** | x86_64 | `zephyr-android-x86_64.tar.gz` |
| **Linux (glibc)** | x86_64 | `zephyr-linux-x86_64.tar.gz` |
| **Linux (glibc)** | ARM 64-bit (`aarch64`) | `zephyr-linux-arm64.tar.gz` |
| **Linux (glibc)** | ARM 32-bit (`armv7`) | `zephyr-linux-armv7.tar.gz` |
| **Linux (musl static)** | x86_64 | `zephyr-linux-x86_64-musl.tar.gz` |
| **Linux (musl static)** | ARM 64-bit (`aarch64`) | `zephyr-linux-aarch64-musl.tar.gz` |
| **Linux (musl static)** | ARM 32-bit (`armv7`) | `zephyr-linux-armv7-musl.tar.gz` |
| **macOS** | Apple Silicon (`aarch64`) | `zephyr-macos-arm64.tar.gz` |
| **macOS** | Intel (`x86_64`) | `zephyr-macos-x86_64.tar.gz` |
| **Windows** | x86_64 | `zephyr-windows-x86_64.zip` |

Every release includes matching `.sha256` files and `SHA256SUMS.txt`.

### Termux (Android) One-Line Install

Open Termux and run:
```bash
curl -fsSL https://raw.githubusercontent.com/haliyari/zephyr/main/zephyr.sh -o zephyr.sh && chmod +x zephyr.sh && ./zephyr.sh install
```

### Linux / macOS One-Line Install

```bash
curl -fsSL https://raw.githubusercontent.com/haliyari/zephyr/main/zephyr.sh -o zephyr.sh && chmod +x zephyr.sh && ./zephyr.sh install
```

### Building from Source

Ensure standard Rust (1.80+) is installed:
```bash
cargo build --release
```
The compiled binary will be placed at `target/release/zephyr`.

---

## Command Reference

### Check Server Reachability

Detects DNS poisoning and network censorship (tailored for Iranian ISPs):
```bash
zephyr check --limit 10
zephyr check --country JP
```

### Auto Connect

Picks the lowest-latency reachable server and launches the proxy:
```bash
zephyr connect --auto
```

### Interactive Connect

Displays a live table of tested servers and lets you choose by number:
```bash
zephyr connect
```

### Custom Endpoint Connect

Connect to a specific server by index, IP, or DDNS hostname:
```bash
zephyr connect 1
zephyr connect public-vpn-257.opengw.net:443
zephyr connect 219.100.37.217:443
```

---

## Application Proxy Configuration

While `zephyr` is running, point your applications to `127.0.0.1:1095`:

### Telegram Desktop
- **Settings** -> **Advanced** -> **Connection Type** -> **Use Custom Proxy**
- Select **SOCKS5**
- Hostname: `127.0.0.1`
- Port: `1095`
- Username / Password: leave blank

### Web Browsers
- **Firefox**: Settings -> Network Settings -> Manual Proxy Configuration -> SOCKS Host: `127.0.0.1`, Port: `1095`, check *Proxy DNS when using SOCKS v5*.
- **Chrome / Chromium**: Launch with flag `--proxy-server="socks5://127.0.0.1:1095"`.

### Terminal (curl)
```bash
# SOCKS5 (with remote DNS resolution)
curl -x socks5h://127.0.0.1:1095 http://httpbin.org/ip

# HTTP CONNECT
curl -x http://127.0.0.1:1095 https://api.ipify.org
```

---

## Censorship Circumvention & Iran Reachability

In heavily censored environments (such as Iran), DDNS domains (`*.opengw.net`) are frequently poisoned to return sinkhole addresses (such as `10.10.34.34`).

Zephyr's built-in reachability engine:
1. Queries both the direct IP address and the DDNS hostname in parallel.
2. Identifies spoofed/poisoned DNS responses and discards them.
3. Automatically connects directly to the server's clean IPv4 address while supplying the original DDNS hostname inside the TLS SNI extension.
4. Completes the handshake even if the DDNS domain is censored by your ISP.
