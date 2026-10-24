// ==============================================================================
// FERRIS JAIL — Linux Landlock LSM Security Sandbox
// Restricts filesystem access: Code files are strictly READ-ONLY
// Memory Hardening: PR_SET_DUMPABLE=0 & RLIMIT_CORE=0 (Zero Memory Leaks)
// Network Jailing: CLONE_NEWNET Zero-Trust Network Namespace
// ==============================================================================

use std::path::{Path, PathBuf};
use crate::i18n::Language;

#[cfg(target_os = "linux")]
use landlock::{
    AccessFs, PathBeneath, PathFd, Ruleset, RulesetAttr, RulesetCreatedAttr,
    RulesetError,
};

pub struct JailConfig {
    pub allow_cwd_write: bool,
    pub extra_write_paths: Vec<PathBuf>,
    pub extra_read_paths: Vec<PathBuf>,
    pub isolate_network: bool,
    pub protect_memory: bool,
}

impl Default for JailConfig {
    fn default() -> Self {
        Self {
            allow_cwd_write: false, // Kod fayllari doimo faqat O'QISH huquqida!
            extra_write_paths: Vec::new(),
            extra_read_paths: Vec::new(),
            isolate_network: false,
            protect_memory: true,
        }
    }
}

/// Linux tizimlarida Landlock, NoNewPrivs, Memory Hardening va Tarmoq Qamali
#[cfg(target_os = "linux")]
pub fn apply_sandbox(config: &JailConfig, lang: Language) -> Result<(), String> {
    // 1. Yangi imtiyozlar olishni taqiqlash (NoNewPrivs)
    let res = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if res != 0 {
        return Err(if lang.is_uz() {
            "PR_SET_NO_NEW_PRIVS o'rnatib bo'lmadi".to_string()
        } else {
            "Failed to set PR_SET_NO_NEW_PRIVS".to_string()
        });
    }

    // 2. Xotira va Core Dump himoyasi (/proc/<pid>/environ va Core Dump larni bloklash)
    if config.protect_memory {
        unsafe {
            // Boshqa jarayonlar /proc/<pid>/environ va /proc/<pid>/mem ni o'qiy olmaydi
            libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);

            // Dastur qulaganda (crash) sirlar diskka yozilmasligi uchun Core Dump ni o'chirish
            let rlim = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            libc::setrlimit(libc::RLIMIT_CORE, &rlim);
        }
    }

    // 3. Tarmoq izolyatsiyasi (CLONE_NEWUSER | CLONE_NEWNET)
    if config.isolate_network {
        let res = unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNET) };
        if res == 0 {
            // Loopback (127.0.0.1 / localhost) interfeysini yoqish (IFF_UP | IFF_RUNNING)
            unsafe {
                let sock = libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0);
                if sock >= 0 {
                    let mut ifr: libc::ifreq = std::mem::zeroed();
                    let name = b"lo\0";
                    std::ptr::copy_nonoverlapping(
                        name.as_ptr() as *const libc::c_char,
                        ifr.ifr_name.as_mut_ptr(),
                        name.len(),
                    );
                    if libc::ioctl(sock, libc::SIOCGIFFLAGS, &mut ifr) >= 0 {
                        ifr.ifr_ifru.ifru_flags |= (libc::IFF_UP | libc::IFF_RUNNING) as libc::c_short;
                        libc::ioctl(sock, libc::SIOCSIFFLAGS, &ifr);
                    }
                    libc::close(sock);
                }
            }
            println!("{}", lang.net_isolated_active());
        } else {
            eprintln!("{}", lang.net_isolated_failed());
        }
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
                eprintln!("{}", lang.jail_kernel_unsupported());
                return Ok(());
            }
        };

    let created_ruleset = match ruleset.create() {
        Ok(cr) => cr,
        Err(_) => {
            eprintln!("{}", lang.jail_ruleset_failed());
            return Ok(());
        }
    };

    let mut rules: Vec<PathBeneath<PathFd>> = Vec::new();

    // Tizim kutubxonalari, tarmoq va standart konfiguratsiyalarga faqat O'QISH ruxsatini berish
    let system_read_paths = [
        "/usr",
        "/lib",
        "/lib64",
        "/bin",
        "/etc/ssl",
        "/etc/pki",
        "/etc/ca-certificates",
        "/usr/share/ca-certificates",
        "/run/systemd/resolve",
        "/etc/resolv.conf",
        "/etc/hosts",
        "/etc/nsswitch.conf",
        "/etc/gai.conf",
        "/etc/protocols",
        "/etc/services",
        "/etc/ld.so.cache",
        "/etc/localtime",
        "/proc",
        "/dev/urandom",
        "/dev/random",
    ];

    for path_str in &system_read_paths {
        let p = Path::new(path_str);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                rules.push(PathBeneath::new(fd, read_access));
            }
        }
    }

    // Tizim qurilmalari, PTY, Shared Memory va vaqtinchalik xotira (YOZISH VA O'QISH talab qilinadigan yo'llar)
    let system_write_paths = [
        "/tmp",
        "/var/tmp",
        "/dev/shm",
        "/dev/null",
        "/dev/zero",
        "/dev/full",
        "/dev/tty",
        "/dev/pts",
    ];

    for path_str in &system_write_paths {
        let p = Path::new(path_str);
        if p.exists() {
            if let Ok(fd) = PathFd::new(p) {
                rules.push(PathBeneath::new(fd, write_access));
            }
        }
    }

    // Foydalanuvchi toolchain yo'llari (Node, Python, Bun, Cargo, Mise, Pyenv, Deno)
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
            home_p.join(".pyenv"),
            home_p.join(".deno"),
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

    // Joriy ishchi papka (CWD) — default bo'yicha FAQAT O'QISH (kod o'zgartirilmasligi uchun)
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


    // Maxsus yozish ruxsatnomalari (masalan ./logs, ./data, ./uploads)
    for p in &config.extra_write_paths {
        // Agar ko'rsatilgan papka mavjud bo'lmasa, uni oldindan yaratib olish (aks holda Landlock uni o'tkazib yuboradi)
        if !p.exists() {
            let _ = std::fs::create_dir_all(p);
        }
        let canonical_p = std::fs::canonicalize(p).unwrap_or_else(|_| p.clone());
        if canonical_p.exists() {
            if let Ok(fd) = PathFd::new(&canonical_p) {
                rules.push(PathBeneath::new(fd, write_access));
            }
        }
    }

    // Barcha qoidalarni birgalikda qo'llash
    let populated_ruleset = match created_ruleset.add_rules(rules.into_iter().map(Ok::<_, RulesetError>)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{} {:?}", if lang.is_uz() { "⚠️  [Ferris Jail] Qoidalarni yuklashda ogohlantirish:" } else { "⚠️  [Ferris Jail] Warning loading ruleset:" }, e);
            return Ok(());
        }
    };

    // Qamal qoidalarini joriy jarayonga tatbiq etish
    match populated_ruleset.restrict_self() {
        Ok(_status) => {
            let write_mode = if config.allow_cwd_write {
                lang.mode_cwd_write()
            } else {
                lang.mode_readonly()
            };
            println!("{}", lang.jail_activated_msg(write_mode));
        }
        Err(e) => {
            eprintln!("{} {:?}", if lang.is_uz() { "⚠️  [Ferris Jail] Qamalni cheklashda ogohlantirish:" } else { "⚠️  [Ferris Jail] Warning restricting process:" }, e);
        }
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_sandbox(_config: &JailConfig, lang: Language) -> Result<(), String> {
    // Linux bo'lmagan tizimlarda (macOS, Windows) xotiradagi sirlar yetarli
    println!("{}", lang.non_linux_jail_notice());
    Ok(())
}
