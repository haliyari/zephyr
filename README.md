# Zephyr

[![Release CI](https://github.com/hossein/zephyr/actions/workflows/release.yml/badge.svg)](https://github.com/hossein/zephyr/actions/workflows/release.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Rust 100%](https://img.shields.io/badge/Rust-100%25-orange.svg)](#)

### اینترنت آزاد و رها مثل باد 🌬️
**[راهنمای فارسی](README.fa.md)** · **[English Documentation](Docs/DOCS.en.md)** · **[راهنمای جامع فارسی](Docs/DOCS.fa.md)** · **[راهنمای ترموکس اندروید](Docs/TERMUX.fa.md)**

**Zephyr** is a lightweight, pure **100% Rust** userspace MS-SSTP client with an in-memory TCP/IP stack (`smoltcp`) and local dual **SOCKS5 & HTTP CONNECT** proxy on `127.0.0.1:1095`.

Built for heavily filtered networks (such as Iran) with automatic DNS poisoning bypass, instant launch caching, and **zero sudo / zero root** requirements.

---

## Highlights

- **100% Pure Rust:** Zero Go code, zero Python, zero external C libraries. Compiles into a single standalone binary (~2.9 MB).
- **100% Userspace:** No `sudo`, no `root` privileges. Does not create kernel `tun`/`tap` devices and does not alter system network configurations or NetworkManager.
- **Dual Local Proxy:** Exposes `127.0.0.1:1095` as both a SOCKS5 (with remote DNS resolution) and HTTP CONNECT proxy on the same port.
- **Iran Censorship & DNS Poisoning Bypass:** Probes both direct IPv4 addresses and DDNS hostnames concurrently, discards poisoned sinkhole responses (`10.10.34.34`), and routes directly to unblocked IPs while preserving TLS SNI.
- **Instant Launch (< 1 ms):** Local compressed cache with 4-hour automatic refresh.
- **Graceful Clean Shutdown:** Pressing `Ctrl+C` immediately closes the tunnel and frees port `1095`.
- **Cross-Platform:** Available for Linux, Android (Termux), macOS, and Windows.

---

## Download Prebuilt Binaries

Prebuilt standalone binaries are generated automatically by GitHub Actions for every release on the **[Releases](https://github.com/hossein/zephyr/releases/latest)** page:

| System / Device | Architecture | Archive |
|---|---|---|
| **Android (Termux)** | ARM 64-bit (`aarch64` / `arm64-v8a`) | [`zephyr-android-arm64.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Android (Termux)** | ARM 32-bit (`armv7` / `armeabi-v7a`) | [`zephyr-android-armv7.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Android (Termux)** | x86_64 | [`zephyr-android-x86_64.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Linux (glibc)** | x86_64 (Desktop & Server) | [`zephyr-linux-x86_64.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Linux (glibc)** | ARM 64-bit (`aarch64`) | [`zephyr-linux-arm64.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Linux (glibc)** | ARM 32-bit (`armv7`) | [`zephyr-linux-armv7.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Linux (musl static)** | x86_64 (Alpine / OpenWrt / Routers) | [`zephyr-linux-x86_64-musl.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Linux (musl static)** | ARM 64-bit (OpenWrt / Routers) | [`zephyr-linux-aarch64-musl.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Linux (musl static)** | ARM 32-bit (OpenWrt / Routers) | [`zephyr-linux-armv7-musl.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **macOS** | Apple Silicon (`M1`/`M2`/`M3`/`M4`) | [`zephyr-macos-arm64.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **macOS** | Intel (`x86_64`) | [`zephyr-macos-x86_64.tar.gz`](https://github.com/hossein/zephyr/releases/latest) |
| **Windows** | x86_64 | [`zephyr-windows-x86_64.zip`](https://github.com/hossein/zephyr/releases/latest) |

Every release includes individual `.sha256` files and a unified `SHA256SUMS.txt`.

---

## One-Line Installers

### Termux (Android)
```bash
curl -fsSL https://raw.githubusercontent.com/hossein/zephyr/main/zephyr.sh -o zephyr.sh && chmod +x zephyr.sh && ./zephyr.sh install
```
*Installs directly into `$PREFIX/bin/zephyr`. See the [Termux Guide](Docs/TERMUX.en.md) for details.*

### Linux / macOS
```bash
curl -fsSL https://raw.githubusercontent.com/hossein/zephyr/main/zephyr.sh -o zephyr.sh && chmod +x zephyr.sh && ./zephyr.sh install
```

---

## Quick Usage

```bash
# 1. Check reachability of servers from your network (Iran)
zephyr check --limit 10

# 2. Connect automatically to the best unblocked server
zephyr connect --auto

# 3. Interactive selection menu
zephyr connect

# 4. Connect to a specific country or custom endpoint
zephyr check --country JP
zephyr connect public-vpn-257.opengw.net:443
```

---

## Application Setup (`127.0.0.1:1095`)

Configure your applications to use the local proxy:

- **Telegram:** Settings -> Advanced -> Connection Type -> SOCKS5 -> `127.0.0.1:1095`
- **Firefox:** Settings -> Network -> Manual Proxy -> SOCKS Host: `127.0.0.1`, Port: `1095`
- **Curl:** `curl -x socks5h://127.0.0.1:1095 http://httpbin.org/ip`

---

## Building from Source

```bash
cargo build --release
```
The output binary will be located at `target/release/zephyr`.

---

## License

This project is licensed under the [GNU General Public License v3.0 (GPLv3)](LICENSE).
