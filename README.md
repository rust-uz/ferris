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
