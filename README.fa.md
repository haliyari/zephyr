# زفیر (Zephyr)

[![Release CI](https://github.com/haliyari/zephyr/actions/workflows/release.yml/badge.svg)](https://github.com/haliyari/zephyr/actions/workflows/release.yml)
[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
[![Rust 100%](https://img.shields.io/badge/Rust-100%25-orange.svg)](#)

### اینترنت آزاد و رها مثل باد 🌬️
**[English README](README.md)** · **[راهنمای جامع فارسی](Docs/DOCS.fa.md)** · **[راهنمای ترموکس اندروید](Docs/TERMUX.fa.md)**

**زفیر (Zephyr)** یک کلاینت سبک، مستقل و تک‌فایل به زبان **راست (Rust)** برای اتصال مستقیم به سرورهای MS-SSTP با پشته شبکه کاربران (`smoltcp`) و پراکسی دوگانه محلی **SOCKS5 و HTTP CONNECT** روی آدرس `127.0.0.1:1095` است.

طراحی‌شده ویژه شبکه‌های محدود و فیلترشده با قابلیت دور زدن آلودگی DNS، اتصال آنی و **بدون نیاز به دسترسی sudo یا روت**.

---

## ویژگی‌های اصلی

- **۱۰۰٪ کد خالص راست:** بدون Go، بدون پایتون، بدون ابزارهای خارجی. یک فایل اجرایی مستقل (~2.9 مگابایت).
- **۱۰۰٪ در فضای کاربر (Userspace):** بدون نیاز به `sudo` یا روت. هیچ اینترفیس مجازی هسته (`tun`/`tap`) ساخته نمی‌شود و تنظیمات شبکه سیستم دست‌نخورده باقی می‌ماند.
- **پراکسی دوگانه محلی:** ارائه هر دو پروتکل **SOCKS5** (با رزولوشن ریموت DNS) و **HTTP CONNECT** روی پورت `1095`.
- **موتور ضد فیلترینگ و ضد آلودگی DNS:** بررسی همزمان آی‌پی مستقیم و دامنه DDNS، حذف پاسخ‌های مسموم‌شده فیلترینگ (`10.10.34.34`) و اتصال مستقیم به آی‌پی‌های سالم با حفظ TLS SNI.
- **اجرای آنی (< ۱ میلی‌ثانیه):** استفاده از کش محلی فشرده با به‌روزرسانی هوشمند ۴ ساعته.
- **قطع اتصال بدون باقیمانده:** با فشردن `Ctrl+C`، ارتباط بلافاصله خاتمه یافته و پورت ۱۰۹۵ آزاد می‌شود.
- **چندسکویی کامل:** لینوکس، اندروید (ترموکس)، مک (اپل سیلیکون و اینتل) و ویندوز.

---

## نصب سریع با یک دستور

### ترموکس اندروید (بدون روت)
```bash
curl -fsSL https://raw.githubusercontent.com/haliyari/zephyr/main/zephyr.sh -o zephyr.sh && chmod +x zephyr.sh && ./zephyr.sh install
```
*برای جزئیات بیشتر [راهنمای ترموکس](Docs/TERMUX.fa.md) را مطالعه فرمایید.*

### لینوکس و مک
```bash
curl -fsSL https://raw.githubusercontent.com/haliyari/zephyr/main/zephyr.sh -o zephyr.sh && chmod +x zephyr.sh && ./zephyr.sh install
```

---

## دانلود باینری‌های آماده

تمام فایل‌ها به صورت خودکار توسط گیت‌هاب اکشنز بیلد شده و در صفحه **[Releases](https://github.com/haliyari/zephyr/releases/latest)** در دسترس هستند:

| سیستم‌عامل | معماری پردازنده | فایل آرشیو |
|---|---|---|
| **ترموکس اندروید** | 64 بیتی (`aarch64` / `arm64-v8a`) | [`zephyr-android-arm64.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **ترموکس اندروید** | 32 بیتی (`armv7` / `armeabi-v7a`) | [`zephyr-android-armv7.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **ترموکس اندروید** | x86_64 | [`zephyr-android-x86_64.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **لینوکس (glibc)** | x86_64 (دسکتاپ و سرور) | [`zephyr-linux-x86_64.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **لینوکس (glibc)** | ARM 64-bit | [`zephyr-linux-arm64.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **لینوکس استاتیک (musl)** | x86_64 (روترها و OpenWrt) | [`zephyr-linux-x86_64-musl.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **لینوکس استاتیک (musl)** | ARM 64-bit (روترها و OpenWrt) | [`zephyr-linux-aarch64-musl.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **مک (macOS)** | اپل سیلیکون (`M1`/`M2`/`M3`/`M4`) | [`zephyr-macos-arm64.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **مک (macOS)** | اینتل (`x86_64`) | [`zephyr-macos-x86_64.tar.gz`](https://github.com/haliyari/zephyr/releases/latest) |
| **ویندوز** | x86_64 | [`zephyr-windows-x86_64.zip`](https://github.com/haliyari/zephyr/releases/latest) |

---

## دستورات و نحوه استفاده

```bash
# بررسی وضعیت در دسترس بودن سرورها
zephyr check --limit 10

# اتصال خودکار به سریع‌ترین سرور باز
zephyr connect --auto

# منوی انتخابی تعاملی
zephyr connect

# اتصال به یک سرور خاص
zephyr connect public-vpn-257.opengw.net:443
```

---

## تنظیم در برنامه‌ها

پراکسی محلی روی آدرس `127.0.0.1:1095` اجرا می‌شود:

- **تلگرام:** Settings -> Advanced -> Connection type -> SOCKS5 -> آدرس: `127.0.0.1` پورت: `1095`
- **مرورگرها:** تنظیم پراکسی SOCKS5 روی `127.0.0.1:1095`
- **تست در ترمینال:**
  ```bash
  curl -x socks5h://127.0.0.1:1095 http://httpbin.org/ip
  ```

---

## مجوز

این پروژه تحت مجوز [GNU General Public License v3.0 (GPLv3)](LICENSE) منتشر شده است.
