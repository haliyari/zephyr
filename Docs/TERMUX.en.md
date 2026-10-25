# Termux (Android) Guide - Zephyr

Run Zephyr directly on any Android phone using Termux.  
**No root required. Zero modification to Android VPN configurations.**

---

## 1. Prerequisites

1. Install **Termux** from [F-Droid](https://f-droid.org/en/packages/com.termux/) (do not use the obsolete version from Google Play Store).
2. Open Termux and ensure package repositories are up to date:
   ```bash
   pkg update -y && pkg upgrade -y
   ```

---

## 2. One-Line Installation

Run the automated installer script:
```bash
curl -fsSL https://raw.githubusercontent.com/hossein/zephyr/main/zephyr.sh -o zephyr.sh && chmod +x zephyr.sh && ./zephyr.sh install
```

The installer will:
1. Detect your device's architecture (`aarch64` / `armv7` / `x86_64`).
2. Download the prebuilt Android binary from GitHub Releases.
3. Verify its SHA-256 checksum.
4. Place `zephyr` into `$PREFIX/bin` so it is instantly available.

---

## 3. How to Connect

In Termux, run:
```bash
zephyr connect --auto
```

You will see:
```text
Connecting to MS-SSTP Server: 🇯🇵 Japan (219.100.37.217:443)...
Remote Server:  🇯🇵 Japan (219.100.37.217:443)
Assigned IP:    10.246.134.128
Local Proxy:    127.0.0.1:1095 [Dual SOCKS5 & HTTP]
```

Keep Termux running in the background.

---

## 4. Configuring Android Apps

### Option A: Telegram (Android)
1. Open Telegram on your phone.
2. Go to **Settings** -> **Data and Storage** -> **Proxy Settings**.
3. Tap **Add Proxy** -> choose **SOCKS5**.
4. Set:
   - **Server:** `127.0.0.1`
   - **Port:** `1095`
5. Save and enable the proxy. Telegram connects instantly through the SSTP tunnel!

### Option B: System-wide Routing (v2rayNG or Nekobox)
If you want all Android apps to route through this tunnel:
1. In **v2rayNG** or **Nekobox**, add a new **SOCKS** config:
   - Address: `127.0.0.1`
   - Port: `1095`
2. Start the proxy app in VPN mode. All phone traffic now flows through Zephyr!

---

## 5. Stopping the Client

Press `Ctrl + C` in Termux. The tunnel disconnects immediately and releases port `1095`.
