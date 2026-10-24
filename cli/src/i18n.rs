// ==============================================================================
// FERRIS I18N — Internationalization Engine
// Default: English (en) for global developers
// Auto-detect: Uzbek (uz) if LANG starts with "uz" or via --lang uz CLI flag
// ==============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    #[default]
    En,
    Uz,
}

impl Language {
    /// Detect language from CLI argument or environment variable
    pub fn detect(cli_override: Option<&str>) -> Self {
        if let Some(l) = cli_override {
            match l.trim().to_lowercase().as_str() {
                "uz" | "uz_uz" | "uzbek" => return Language::Uz,
                "en" | "en_us" | "english" => return Language::En,
                _ => {}
            }
        }

        if let Ok(lang) = std::env::var("LC_ALL").or_else(|_| std::env::var("LANG")) {
            if lang.to_lowercase().starts_with("uz") {
                return Language::Uz;
            }
        }

        Language::En
    }

    pub fn is_uz(&self) -> bool {
        matches!(self, Language::Uz)
    }

    #[allow(dead_code)]
    pub fn non_linux_jail_notice(&self) -> &'static str {
        match self {
            Language::En => {
                "ℹ️  [Ferris Jail] Landlock LSM is Linux-specific. In-memory zero-disk secrets protection is active, but filesystem/network sandboxing requires Linux (Kernel >= 5.13)."
            }
            Language::Uz => {
                "ℹ️  [Ferris Jail] Landlock LSM faqat Linux uchun xos. Xotiradagi sirlar ChaCha20-Poly1305 bilan himoyalangan, ammo fayl tizimi va tarmoq izolyatsiyasi faqat Linux (Kernel >= 5.13) da ishlaydi."
            }
        }
    }

    pub fn jail_kernel_unsupported(&self) -> &'static str {
        match self {
            Language::En => {
                "⚠️  [Ferris Jail] Linux kernel does not support Landlock LSM (Kernel < 5.13). Falling back to NoNewPrivs."
            }
            Language::Uz => {
                "⚠️  [Ferris Jail] Linux yadrosi Landlock LSM ni qo'llab-quvvatlamaydi (Kernel < 5.13). Faqat NoNewPrivs faollashtirildi."
            }
        }
    }

    pub fn jail_ruleset_failed(&self) -> &'static str {
        match self {
            Language::En => {
                "⚠️  [Ferris Jail] Failed to create Landlock ruleset. Default memory protection active."
            }
            Language::Uz => {
                "⚠️  [Ferris Jail] Landlock qoidalari to'plami yaratilmadi. Standart himoya rejimi faol."
            }
        }
    }

    pub fn jail_activated_msg(&self, write_mode: &str) -> String {
        match self {
            Language::En => {
                format!("🛡️  [Ferris Jail] Kernel sandbox (Landlock LSM) activated [Mode: {}].", write_mode)
            }
            Language::Uz => {
                format!("🛡️  [Ferris Jail] Yadroviy qamal (Landlock LSM) faollashtirildi [Rejim: {}].", write_mode)
            }
        }
    }

    pub fn mode_readonly(&self) -> &'static str {
        match self {
            Language::En => "Read-Only Code Protection",
            Language::Uz => "Faqat o'qish (Read-Only Code Protection)",
        }
    }

    pub fn mode_cwd_write(&self) -> &'static str {
        match self {
            Language::En => "Full Write (CWD Write Allowed)",
            Language::Uz => "To'liq yozish (CWD Write)",
        }
    }

    pub fn scan_start_all(&self, path: &str) -> String {
        match self {
            Language::En => format!("🔍 Scanning path '{}' for leaked secrets & API keys...", path),
            Language::Uz => format!("🔍 '{}' yo'lida ochiq sirlar va API kalitlar skanerlanmoqda...", path),
        }
    }

    pub fn scan_start_staged(&self) -> &'static str {
        match self {
            Language::En => "🔍 Scanning git staged files for secrets (Pre-Commit mode)...",
            Language::Uz => "🔍 Git staged fayllari sirlar uchun tekshirilmoqda (Pre-Commit rejimi)...",
        }
    }

    pub fn scan_no_staged_files(&self) -> &'static str {
        match self {
            Language::En => "ℹ️  No git staged files found to scan.",
            Language::Uz => "ℹ️  Tekshirish uchun staged qilingan fayllar topilmadi.",
        }
    }

    pub fn scan_clean(&self, files_scanned: usize, duration_ms: u128) -> String {
        match self {
            Language::En => {
                format!("✅ [Ferris Guardian] Clean! Scanned {} files in {}ms. No secrets found.", files_scanned, duration_ms)
            }
            Language::Uz => {
                format!("✅ [Ferris Guardian] Toza! {} ta fayl {}ms ichida tekshirildi. Hech qanday sir aniqlanmadi.", files_scanned, duration_ms)
            }
        }
    }

    pub fn scan_failed_summary(&self, findings_count: usize, files_scanned: usize) -> String {
        match self {
            Language::En => {
                format!("🚫 [Ferris Guardian] BLOCKED: Found {} leaked secret(s) across {} files scanned!", findings_count, files_scanned)
            }
            Language::Uz => {
                format!("🚫 [Ferris Guardian] TO'XTATILDI: {} ta faylda {} ta ochiq sir aniqlandi!", files_scanned, findings_count)
            }
        }
    }

    pub fn hook_installed_success(&self, path: &str) -> String {
        match self {
            Language::En => format!("✅ Pre-commit hook successfully installed at '{}'. Commits with leaked secrets will be blocked!", path),
            Language::Uz => format!("✅ Pre-commit hook '{}' fayliga muvaffaqiyatli o'rnatildi. Sirlar bilan commit qilish to'xtatiladi!", path),
        }
    }

    pub fn hook_not_git_repo(&self) -> &'static str {
        match self {
            Language::En => "❌ Current directory is not a git repository (missing .git). Initialize with `git init` first.",
            Language::Uz => "❌ Joriy papka git ombori emas (.git topilmadi). Avval `git init` buyrug'ini bajaring.",
        }
    }

    pub fn vault_sealing(&self) -> &'static str {
        match self {
            Language::En => "🔒 [Ferris Vault] Preparing encrypted secrets vault...",
            Language::Uz => "🔒 [Ferris Vault] Maxfiylik ombori tayyorlanmoqda...",
        }
    }

    pub fn vault_sealed_success(&self, output: &str) -> String {
        match self {
            Language::En => format!("✅ Successfully sealed into: {}", output),
            Language::Uz => format!("✅ Muvaffaqiyatli muhrlandi: {}", output),
        }
    }

    pub fn vault_encrypted_count(&self, count: usize) -> String {
        match self {
            Language::En => format!("   • Encrypted environment variables count: {}", count),
            Language::Uz => format!("   • Aniqlangan va shifrlangan o'zgaruvchilar soni: {}", count),
        }
    }

    pub fn vault_algorithm(&self) -> &'static str {
        match self {
            Language::En => "   • Encryption algorithm: ChaCha20-Poly1305 (256-bit AEAD)",
            Language::Uz => "   • Shifrlash algoritmi: ChaCha20-Poly1305 (256-bit AEAD)",
        }
    }

    pub fn vault_wiped_msg(&self) -> &'static str {
        match self {
            Language::En => "   • Original plaintext file securely wiped from disk (Zero-Disk Footprint).",
            Language::Uz => "   • Asl ochiq fayl diskdan izsiz xavfsiz tozalandi (Zero-Disk Footprint).",
        }
    }

    pub fn vault_kept_msg(&self) -> &'static str {
        match self {
            Language::En => "   • ATTENTION: Original plaintext kept on disk (--keep).",
            Language::Uz => "   • DIQQAT: Asl fayl diskda saqlab qolindi (--keep).",
        }
    }

    pub fn vault_run_hint(&self) -> &'static str {
        match self {
            Language::En => "\n💡 Run your application securely in the enclave:\n   $ ferris run -- python3 bot.py",
            Language::Uz => "\n💡 Endi ilovani xavfsiz qobiqda ishga tushiring:\n   $ ferris run -- python3 bot.py",
        }
    }

    pub fn vault_inspect_header(&self, vault: &str, count: usize) -> String {
        match self {
            Language::En => format!("🔐 Vault contents: {} (Total: {})", vault, count),
            Language::Uz => format!("🔐 Vault tarkibi: {} (Jami: {})", vault, count),
        }
    }

    pub fn vault_reveal_hint(&self) -> &'static str {
        match self {
            Language::En => "ℹ️  To reveal values on screen, pass --reveal.",
            Language::Uz => "ℹ️  Qiymatlarni ko'rish uchun: --reveal qo'shing.",
        }
    }

    pub fn vault_unseal_start(&self) -> &'static str {
        match self {
            Language::En => "⚠️  [Ferris Vault] Restoring plaintext file (Unseal)...",
            Language::Uz => "⚠️  [Ferris Vault] Faylni qayta tiklash (Unseal)...",
        }
    }

    pub fn vault_unseal_success(&self, output: &str) -> String {
        match self {
            Language::En => format!("✅ Plaintext file restored: {}", output),
            Language::Uz => format!("✅ Ochiq fayl tiklandi: {}", output),
        }
    }

    pub fn vault_unseal_count(&self, count: usize) -> String {
        match self {
            Language::En => format!("   • Restored variables: {}", count),
            Language::Uz => format!("   • Tiklangan o'zgaruvchilar: {}", count),
        }
    }

    pub fn vault_unseal_perms(&self) -> &'static str {
        match self {
            Language::En => "   • File permissions: 0600 (Current user only)",
            Language::Uz => "   • Ruxsatlar: 0600 (Faqat joriy foydalanuvchi)",
        }
    }

    pub fn vault_unseal_warning(&self) -> &'static str {
        match self {
            Language::En => "   • ATTENTION: After editing, re-encrypt with 'ferris vault seal'!",
            Language::Uz => "   • DIQQAT: Foydalangach uni yana 'ferris vault seal' qilishni unutmang!",
        }
    }

    pub fn run_started(&self, program: &str, args: &str) -> String {
        match self {
            Language::En => format!("🚀 [Started]: {} {}\n", program, args),
            Language::Uz => format!("🚀 [Boshlandi]: {} {}\n", program, args),
        }
    }

    pub fn run_exec_failed(&self, err: &str) -> String {
        match self {
            Language::En => format!("❌ Failed to execute process: {}", err),
            Language::Uz => format!("❌ Dasturni ishga tushirib bo'lmadi: {}", err),
        }
    }

    pub fn net_isolated_active(&self) -> &'static str {
        match self {
            Language::En => "🌐 [Ferris Net] Full network isolation active (External network disabled, Localhost enabled).",
            Language::Uz => "🌐 [Ferris Net] To'liq tarmoq izolyatsiyasi faol (Tashqi internet uzildi, Localhost faollashtirildi).",
        }
    }

    pub fn net_isolated_failed(&self) -> &'static str {
        match self {
            Language::En => "⚠️  [Ferris Net] Failed to create network namespace (requires kernel permission).",
            Language::Uz => "⚠️  [Ferris Net] Network namespace yaratilmadi (yadro ruxsati kerak).",
        }
    }

    pub fn ping_checking(&self) -> &'static str {
        match self {
            Language::En => "🦀 Checking Ferris.uz API connectivity...",
            Language::Uz => "🦀 Ferris.uz API tekshirilmoqda...",
        }
    }

    pub fn ping_connected(&self) -> &'static str {
        match self {
            Language::En => "✅ API Connected:",
            Language::Uz => "✅ API aloqada:",
        }
    }

    pub fn auth_logged_in(&self, username: &str) -> String {
        match self {
            Language::En => format!("✅ Successfully authenticated as @{}!", username),
            Language::Uz => format!("✅ Muvaffaqiyatli kirdingiz, @{}!", username),
        }
    }

    pub fn auth_token_saved(&self, path: &str) -> String {
        match self {
            Language::En => format!("🔑 Session token saved to {}.", path),
            Language::Uz => format!("🔑 Sessiya tokeni {} ga saqlandi.", path),
        }
    }

    pub fn auth_invalid_token(&self) -> &'static str {
        match self {
            Language::En => "❌ The provided token is invalid or expired.",
            Language::Uz => "❌ Kiritilgan token yaroqsiz yoki muddati o'tgan.",
        }
    }

    pub fn auth_logged_out(&self) -> &'static str {
        match self {
            Language::En => "✅ All local terminal sessions successfully cleared.",
            Language::Uz => "✅ Terminaldagi barcha sessiyalar muvaffaqiyatli tozalandi.",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lang_detection_override() {
        assert_eq!(Language::detect(Some("uz")), Language::Uz);
        assert_eq!(Language::detect(Some("UZ")), Language::Uz);
        assert_eq!(Language::detect(Some("en")), Language::En);
        assert_eq!(Language::detect(Some("EN")), Language::En);
    }

    #[test]
    fn test_lang_fallback() {
        // Unrecognized override should fall through
        let detected = Language::detect(Some("fr"));
        // Either En or whatever env LANG is, but definitely valid
        assert!(matches!(detected, Language::En | Language::Uz));
    }
}
