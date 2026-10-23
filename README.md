# 🦀 Ferris CLI (`ferris`)

[![Crates.io](https://img.shields.io/badge/crates.io-v0.2.0-orange.svg)](https://crates.io)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green.svg)](https://ferris.uz)

O'zbekiston Rustaceanlar va Tizimli Muhandislar Hamjamiyati rasmiy terminal vositasi (Developer Infrastructure & Incident Hub).

## 🚀 O'rnatish (Installation)

### 1-qatorli tezkor o'rnatuvchi (Linux & macOS):
```bash
curl -sSf https://ferris.uz/install.sh | sh
```

### Windows (PowerShell):
```powershell
irm https://ferris.uz/install.ps1 | iex
```

### Cargo orqali (Har qanday operatsion tizimda):
```bash
cargo install ferris-cli
```

---

## ⚡ Foydalanish (Usage)

### 🛡️ 1. Ferris Enclave & Vault (Kiberxavfsizlik va Sirlar Ombori)
Loyiha sirlarini (.env) diskda ochiq qoldirmasdan, ChaCha20-Poly1305 orqali shifrlash va Linux Landlock LSM yadroviy qamali ostida xavfsiz ishga tushirish:

```bash
# 1. .env faylni shifrlab muhrlash (ochiq nusxa xavfsiz tarzda diskdan yo'qotiladi):
ferris vault seal

# 2. Vault ichidagi kalitlar ro'yxatini ko'rish:
ferris vault inspect
ferris vault inspect --reveal

# 3. Ilovani xavfsiz qobiqda (RAM-only secrets + Read-Only kod himoyasi) ishga tushirish:
ferris run --allow-write ./logs -- python3 bot.py

# 4. Zero-Network rejimi (Tashqi internet uziladi, Localhost va Unix Soket faol):
ferris run -n -- python3 worker.py

# 5. Kerak bo'lganda vaultni qayta .env ga chiqarish:
ferris vault unseal
```

### 📡 2. Hamjamiyat va Tizim Buyruqlari
```bash
# Tizim va API holatini tekshirish
ferris ping

# Jonli jamoat chatiga ulanish (WebSocket Real-time)
ferris live

# Yangi postlar va muhokamalar tasmasini ko'rish
ferris feed

# Shaxsiy xabarlarni o'qish va yuborish (Direct Messages)
ferris dm list
ferris dm read iskurama
ferris dm send iskurama "Salom Rustacean!"

# Foydalanuvchini bloklash / blokdan chiqarish
ferris block add @spammer
ferris block list

# Foydalanish shartlari va Maxfiylik siyosati
ferris terms
ferris privacy
ferris license
```

---

## 📄 Litsenziya (License)

Ushbu loyiha qo'shaloq litsenziya asosida taqdim etiladi:
- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

Muallif: **iskurama** & Ferris.uz jamoasi.
Batafsil: [https://ferris.uz](https://ferris.uz)
