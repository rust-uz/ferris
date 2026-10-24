# 🦀 Ferris CLI (`ferris`) v0.3.0

[![Crates.io](https://img.shields.io/badge/crates.io-v0.3.0-orange.svg)](https://crates.io)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green.svg)](https://ferris.uz)
[![Made with Rust](https://img.shields.io/badge/made%20with-🦀%20in%20Uzbekistan-red.svg)](https://ferris.uz)

Universal **DevSecOps Platform, Landlock LSM Enclave, Zero-Disk Secret Vault & Pre-Commit Secret Guardian**.

```text
       Developer / CI / Git Pre-Commit Hook
                       │
                       ▼
                 ferris v0.3.0
                       │
       ┌───────────────┴───────────────┐
       ▼                               ▼
  [Core DevSecOps]              [Community Hub]
  • ferris scan                 • ferris hub feed
  • ferris vault                • ferris hub post
  • ferris run                  • ferris hub chat
  • ferris doc                  • ferris hub dm
  • ferris auth                 • ferris hub group
```

---

## 🚀 Installation

### One-line installer (Linux & macOS):
```bash
curl -fsSL https://ferris.uz/install.sh | bash
```

### Windows (PowerShell):
```powershell
irm https://ferris.uz/install.ps1 | iex
```

### From Crates.io via Cargo:
```bash
cargo install ferris-cli
```

---

## ⚡ Core DevSecOps Capabilities

### 🛡️ 1. `ferris scan` (Pre-Commit Secret Guardian)
Prevent leaks of API keys, private keys, database connection strings, and high-entropy secret tokens:

```bash
# Scan working tree (respects .gitignore)
ferris scan

# Scan only git staged files
ferris scan --staged

# Install automated git pre-commit hook into .git/hooks/pre-commit
ferris scan --install-hook
```

### 🔐 2. `ferris vault` (Zero-Disk Secrets Store)
ChaCha20-Poly1305 AEAD encrypted secret storage:

```bash
# Seal .env into .env.ferris and securely wipe cleartext file:
ferris vault seal

# Inspect keys inside vault:
ferris vault inspect
ferris vault inspect --reveal

# Restore cleartext .env if needed:
ferris vault unseal
```

### 🚀 3. `ferris run` (Landlock Enclave & RAM-only secrets)
Launch processes with zero-disk environment variables and read-only source files:

```bash
# Run application in hardened sandbox:
ferris run --allow-write ./logs -- python3 bot.py

# Complete network isolation (Zero-Trust namespace):
ferris run -n -- python3 worker.py
```

### 🔑 4. `ferris auth` (Terminal SSO & Sessions)
```bash
ferris auth login
ferris auth status
ferris auth logout
```

---

## 📡 Community Hub (`ferris hub`)

```bash
# Community discussion feed
ferris hub feed

# Real-time WebSocket developer chat
ferris hub chat

# Direct messages
ferris hub dm list
ferris hub dm send iskurama "Hello Rustacean!"

# Topics and communities
ferris hub topic list
ferris hub group list
```

*(Note: legacy aliases `ferris chat`, `ferris feed`, `ferris post` remain backwards-compatible).*

---

## 🌐 Multi-Language (i18n)

Default is English (`en`). Uzbek (`uz`) is auto-detected from system locale or selected with `--lang`:
```bash
ferris --lang en scan
ferris --lang uz scan
```

---

## 📄 License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).

Made with 🦀 in Uzbekistan by **iskurama** & the Ferris.uz Community.
Website: [https://ferris.uz](https://ferris.uz)
