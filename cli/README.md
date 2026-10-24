# 🦀 Ferris CLI (`ferris`) v0.3.0

[![Crates.io](https://img.shields.io/badge/crates.io-v0.3.0-orange.svg)](https://crates.io)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-green.svg)](https://ferris.uz)
[![Made with Rust](https://img.shields.io/badge/made%20with-🦀%20in%20Uzbekistan-red.svg)](https://ferris.uz)

High-performance **DevSecOps Platform, Landlock LSM Enclave, Zero-Disk Secret Vault & Pre-Commit Secret Guardian** for developers and engineering teams.

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

## 🛡️ Core DevSecOps Capabilities

### 1. `ferris scan` — Pre-Commit Secret Guardian
Detects leaked credentials before they reach git repositories or CI/CD pipelines.

- **High-Precision Rules:** OpenAI (`sk-...`), Anthropic (`sk-ant-...`), AWS Access Keys (`AKIA...`), GitHub Tokens (`ghp_...`), Stripe Secret Keys (`sk_live_...`), Telegram Bot Tokens, Private Keys, and Database Connection Strings.
- **Shannon Entropy Analysis:** Identifies high-entropy leaked secret tokens (entropy > 4.2).
- **Sub-10ms Performance:** Scans staged files instantly and halts commits with exit code `1` if leaks are detected.
- **Safe Masking:** Masks detected secrets (`sk-proj-****...****`) to prevent exposure in terminal logs.

```bash
# Scan current repository (respects .gitignore)
ferris scan

# Scan only git staged files (Pre-Commit mode)
ferris scan --staged

# Automatically install pre-commit hook into .git/hooks/pre-commit
ferris scan --install-hook
```

---

### 2. `ferris vault` — Zero-Disk ChaCha20-Poly1305 Secrets Vault
Keep credentials encrypted at rest without unencrypted `.env` files lingering on developer machines.

```bash
# Seal .env into encrypted .env.ferris and securely wipe original from disk
ferris vault seal

# Inspect keys inside vault without revealing secrets
ferris vault inspect

# Reveal secrets on demand
ferris vault inspect --reveal

# Restore encrypted vault back to .env when needed
ferris vault unseal
```

---

### 3. `ferris run` — Kernel Sandboxed Enclave (Landlock LSM)
Execute applications in a hardened, restricted environment where credentials exist solely in RAM.

- **Filesystem Lockdown:** Project code is mounted strictly `READ-ONLY` via Linux Landlock LSM.
- **Memory Hardening:** Sets `PR_SET_DUMPABLE=0` and `RLIMIT_CORE=0` to block memory scrapers and core dumps.
- **Network Isolation:** Optional `CLONE_NEWNET` zero-trust namespace.

```bash
# Run application in secure enclave with RAM-only secrets
ferris run --allow-write ./logs -- python3 bot.py

# Run in zero-network isolated sandbox
ferris run -n -- node worker.js
```

---

### 4. `ferris auth` — SSO & Credential Management
Authenticate your terminal with Ferris infrastructure and sync settings.

```bash
ferris auth login
ferris auth status
ferris auth logout
```

---

## 👥 Community Hub (`ferris hub`)

The developer community and discussion platform:

```bash
# Read community discussion feed
ferris hub feed

# Join real-time WebSocket chat room
ferris hub chat --room general

# Post a discussion topic
ferris hub post create --topic "f/rust" --title "Async Landlock" --content "..."

# Direct messages
ferris hub dm list
ferris hub dm send username "Hello!"
```

> **Note**: Backward-compatible shortcuts (`ferris chat`, `ferris post`, `ferris feed`) remain fully supported.

---

## 🌐 Multi-Language Support (i18n)

Ferris CLI defaults to **English** for international developers and automatically activates **Uzbek** if the system `LANG` begins with `uz`. You can explicitly override the language at any time:

```bash
ferris --lang en scan
ferris --lang uz scan
```

---

## 📄 License

Dual-licensed under either:
- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

Made with 🦀 by **iskurama** & the Rust Uzbekistan Community.
Website: [https://ferris.uz](https://ferris.uz) • Documentation: [https://doc.ferris.uz](https://doc.ferris.uz)
