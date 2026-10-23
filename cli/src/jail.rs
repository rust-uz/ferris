// ==============================================================================
// FERRIS JAIL — Linux Landlock LSM Security Sandbox
// Restricts filesystem access to project directories only
// Enforces PR_SET_NO_NEW_PRIVS to prevent privilege escalation
// ==============================================================================

use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use landlock::{
    AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
    RulesetError,
};

pub struct JailConfig {
    pub allow_cwd_write: bool,
    pub extra_write_paths: Vec<PathBuf>,
    pub extra_read_paths: Vec<PathBuf>,
}

impl Default for JailConfig {
    fn default() -> Self {
        Self {
            allow_cwd_write: true,
            extra_write_paths: Vec::new(),
            extra_read_paths: Vec::new(),
        }
    }
}

/// Linux tizimlarida Landlock va NoNewPrivs orqali qamal o'rnatish
#[cfg(target_os = "linux")]
pub fn apply_sandbox(config: &JailConfig) -> Result<(), String> {
    // 1. Yangi imtiyozlar olishni taqiqlash (NoNewPrivs)
    let res = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if res != 0 {
        return Err("PR_SET_NO_NEW_PRIVS o'rnatib bo'lmadi".to_string());
    }

    let read_access = AccessFs::ReadFile
        | AccessFs::ReadDir
        | AccessFs::Execute;

    let write_access = read_access
        | AccessFs::WriteFile
        | AccessFs::RemoveFile
        | AccessFs::RemoveDir
        | AccessFs::MakeChar
        | AccessFs::MakeDir
        | AccessFs::MakeReg
        | AccessFs::MakeSock
        | AccessFs::MakeFifo
        | AccessFs::MakeSym;

    let ruleset = match Ruleset::default()
        .handle_access(read_access | write_access) {
            Ok(r) => r,
            Err(_) => {
                eprintln!("⚠️  [Ferris Jail] Linux yadrosi Landlock LSM ni qo'llab-quvvatlamaydi (Kernel < 5.13). Faqat NoNewPrivs faollashtirildi.");
                return Ok(());
            }
        };

    let created_ruleset = match ruleset.create() {
        Ok(cr) => cr,
        Err(_) => {
            eprintln!("⚠️  [Ferris Jail] Landlock qoidalari to'plami yaratilmadi. Standart himoya rejimi faol.");
            return Ok(());
        }
    };

    let mut rules: Vec<PathBeneath<PathFd>> = Vec::new();

    // Tizim kutubxonalari va standart fayllarga faqat O'QISH ruxsatini berish
    let system_read_paths = [
        "/usr",
        "/lib",
        "/lib64",
        "/etc/ssl",
        "/etc/pki",
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/nsswitch.conf",
        "/etc/ld.so.cache",
        "/proc",
        "/dev/urandom",
        "/dev/null",
        "/dev/zero",
    ];

    for path_str in &system_read_paths {
        let p = Path::new(path_str);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                rules.push(PathBeneath::new(fd, read_access));
            }
        }
    }

    // Foydalanuvchi toolchain yo'llari (Node, Python, Bun, Cargo, Mise)
    if let Ok(home) = std::env::var("HOME") {
        let home_p = PathBuf::from(home);
        let user_tool_paths = [
            home_p.join(".local"),
            home_p.join(".cargo"),
            home_p.join(".rustup"),
            home_p.join(".config").join("mise"),
            home_p.join(".nvm"),
            home_p.join(".bun"),
            home_p.join(".asdf"),
        ];
        for p in &user_tool_paths {
            if p.exists() {
                if let Ok(fd) = PathFd::new(p) {
                    rules.push(PathBeneath::new(fd, read_access));
                }
            }
        }
    }

    // Qo'shimcha o'qish yo'llari
    for p in &config.extra_read_paths {
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                rules.push(PathBeneath::new(fd, read_access));
            }
        }
    }

    // Joriy ishchi papka (CWD)
    if let Ok(cwd) = std::env::current_dir() {
        let cwd_access = if config.allow_cwd_write {
            write_access
        } else {
            read_access
        };

        if let Ok(fd) = PathFd::new(&cwd) {
            rules.push(PathBeneath::new(fd, cwd_access));
        }
    }

    // Vaqtincha /tmp papkasi
    let tmp = Path::new("/tmp");
    if tmp.exists() {
        if let Ok(fd) = PathFd::new(tmp) {
            rules.push(PathBeneath::new(fd, write_access));
        }
    }

    // Maxsus yozish ruxsatnomalari
    for p in &config.extra_write_paths {
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                rules.push(PathBeneath::new(fd, write_access));
            }
        }
    }

    // Barcha qoidalarni birgalikda qo'llash
    let populated_ruleset = match created_ruleset.add_rules(rules.into_iter().map(Ok::<_, RulesetError>)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("⚠️  [Ferris Jail] Qoidalarni yuklashda ogohlantirish: {:?}", e);
            return Ok(());
        }
    };

    // Qamal qoidalarini joriy jarayonga tatbiq etish
    match populated_ruleset.restrict_self() {
        Ok(_status) => {
            println!("🛡️  [Ferris Jail] Yadroviy qamal (Landlock LSM) faollashtirildi.");
        }
        Err(e) => {
            eprintln!("⚠️  [Ferris Jail] Qamalni cheklashda ogohlantirish: {:?}", e);
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_sandbox(_config: &JailConfig) -> Result<(), String> {
    // Linux bo'lmagan tizimlarda (macOS, Windows) xotiradagi sirlar yetarli
    Ok(())
}
