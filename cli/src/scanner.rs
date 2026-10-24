// ==============================================================================
// FERRIS SCANNER — Pre-Commit Secret Guardian Engine
// Scans files and git staged changes for exposed API keys, private keys, and tokens.
// High-precision regex rules + Shannon entropy calculation.
// Exit code 1 on secrets found (blocks git commits).
// ==============================================================================

use crate::i18n::Language;
use colored::*;
use ignore::WalkBuilder;
use regex::Regex;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub struct ScanFinding {
    pub file_path: PathBuf,
    pub line_number: usize,
    pub rule_name: &'static str,
    pub masked_secret: String,
    pub snippet: String,
}

#[derive(Debug, Default, Clone)]
pub struct ScanReport {
    pub files_scanned: usize,
    pub findings: Vec<ScanFinding>,
    pub duration_ms: u128,
}

impl ScanReport {
    pub fn has_secrets(&self) -> bool {
        !self.findings.is_empty()
    }
}

pub struct SecretRule {
    pub name: &'static str,
    pub regex: Regex,
}

pub struct ScannerEngine {
    rules: Vec<SecretRule>,
    assignment_regex: Regex,
}

impl ScannerEngine {
    pub fn new() -> Self {
        let rules = vec![
            SecretRule {
                name: "OpenAI / Anthropic API Key",
                regex: Regex::new(r"(?i)\b(?:sk-(?:proj-|ant-)?(?:api03-|admin-)?[a-zA-Z0-9_\-]{20,})\b").unwrap(),
            },
            SecretRule {
                name: "AWS Access Key ID",
                regex: Regex::new(r"\b(?:AKIA|ASIA|AROA)[A-Z0-9]{16}\b").unwrap(),
            },
            SecretRule {
                name: "AWS Secret Access Key",
                regex: Regex::new(r#"(?i)\b(?:aws_secret_access_key|aws_secret_key)\s*[:=]\s*['"]?([A-Za-z0-9/+=]{40})['"]?\b"#).unwrap(),
            },
            SecretRule {
                name: "GitHub Access Token",
                regex: Regex::new(r"\b(?:(?:ghp|gho|ghu|ghs|ghr)_[a-zA-Z0-9]{36,}|github_pat_[a-zA-Z0-9_]{22,})\b").unwrap(),
            },
            SecretRule {
                name: "Stripe Secret Key",
                regex: Regex::new(r"\b(?:sk|rk)_(?:live|test)_[0-9a-zA-Z]{24,}\b").unwrap(),
            },
            SecretRule {
                name: "Telegram Bot Token",
                regex: Regex::new(r"\b[0-9]{8,10}:[a-zA-Z0-9_-]{35}\b").unwrap(),
            },
            SecretRule {
                name: "Private Cryptographic Key",
                regex: Regex::new(r"-----BEGIN (?:[A-Z0-9_\-]+ )?PRIVATE KEY(?: BLOCK)?-----").unwrap(),
            },
            SecretRule {
                name: "Database Connection String",
                regex: Regex::new(r#"\b(?:postgres|postgresql|mysql|mongodb|redis|rediss|amqp|amqps)://[^:\s/]*:[^@\s/]+@[a-zA-Z0-9_.\-]+(?::\d+)?/[^\s"'>]*\b"#).unwrap(),
            },
        ];

        let assignment_regex = Regex::new(
            r#"(?i)\b(?:api[_-]?key|secret|password|token|access[_-]?token|client[_-]?secret)\s*[:=]\s*(?:"([^"]{16,})"|'([^']{16,})'|([a-zA-Z0-9_\-]{16,}))"#
        ).unwrap();

        Self {
            rules,
            assignment_regex,
        }
    }

    pub fn scan_text(&self, path: &Path, content: &str) -> Vec<ScanFinding> {
        let mut findings = Vec::new();

        for (idx, line) in content.lines().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim();

            // Ignore common false positives: placeholder strings
            let lower = trimmed.to_lowercase();
            if lower.contains("your_api_key")
                || lower.contains("your_secret")
                || lower.contains("change_me")
                || lower.contains("insert_here")
                || lower.contains("dummy_key_here")
            {
                continue;
            }

            // Check standard rules
            for rule in &self.rules {
                if let Some(caps) = rule.regex.captures(trimmed) {
                    let matched_str = caps
                        .get(1)
                        .or_else(|| caps.get(0))
                        .map(|m| m.as_str())
                        .unwrap_or(trimmed);

                    findings.push(ScanFinding {
                        file_path: path.to_path_buf(),
                        line_number: line_num,
                        rule_name: rule.name,
                        masked_secret: mask_secret(matched_str),
                        snippet: trimmed.to_string(),
                    });
                    break;
                }
            }

            // Check high-entropy assignment pattern
            if let Some(caps) = self.assignment_regex.captures(trimmed) {
                let (val, is_quoted) = if let Some(m) = caps.get(1).or_else(|| caps.get(2)) {
                    (m.as_str(), true)
                } else if let Some(m) = caps.get(3) {
                    (m.as_str(), false)
                } else {
                    ("", false)
                };

                let is_code_declaration = trimmed.starts_with("let ")
                    || trimmed.starts_with("const ")
                    || trimmed.starts_with("var ");

                if !val.is_empty() && (is_quoted || !is_code_declaration) {
                    let entropy = shannon_entropy(val);
                    let is_hex = val.chars().all(|c| c.is_ascii_hexdigit());
                    let threshold = if is_hex { 3.0 } else { 3.6 };
                    if entropy >= threshold && !findings.iter().any(|f| f.line_number == line_num) {
                        findings.push(ScanFinding {
                            file_path: path.to_path_buf(),
                            line_number: line_num,
                            rule_name: "High-Entropy Secret Token",
                            masked_secret: mask_secret(val),
                            snippet: trimmed.to_string(),
                        });
                    }
                }
            }
        }

        findings
    }
}

impl Default for ScannerEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub fn mask_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    let len = chars.len();
    if len <= 8 {
        "********".to_string()
    } else if len <= 16 {
        let prefix: String = chars[..3].iter().collect();
        let suffix: String = chars[len - 2..].iter().collect();
        format!("{}****{}", prefix, suffix)
    } else {
        let prefix: String = chars[..4].iter().collect();
        let suffix: String = chars[len - 4..].iter().collect();
        format!("{}****...****{}", prefix, suffix)
    }
}

pub fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut counts = std::collections::HashMap::new();
    let chars: Vec<char> = s.chars().collect();
    for &c in &chars {
        *counts.entry(c).or_insert(0usize) += 1;
    }
    let len = chars.len() as f64;
    counts.values().fold(0.0, |acc, &count| {
        let p = count as f64 / len;
        acc - p * p.log2()
    })
}

pub fn is_binary_bytes(bytes: &[u8]) -> bool {
    bytes.iter().take(1024).any(|&b| b == 0)
}

fn is_binary_file(path: &Path) -> bool {
    if let Ok(mut file) = fs::File::open(path) {
        let mut buffer = [0u8; 1024];
        if let Ok(bytes_read) = file.read(&mut buffer) {
            return is_binary_bytes(&buffer[..bytes_read]);
        }
    }
    false
}

/// Locate git directory (.git folder or worktree/submodule gitdir pointer)
pub fn find_git_dir() -> Option<PathBuf> {
    if let Ok(output) = Command::new("git").args(["rev-parse", "--git-dir"]).output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                let p = PathBuf::from(&s);
                return Some(if p.is_relative() {
                    std::env::current_dir().map(|c| c.join(p)).unwrap_or_else(|_| PathBuf::from(s))
                } else {
                    p
                });
            }
        }
    }

    if let Ok(mut curr) = std::env::current_dir() {
        loop {
            let git_candidate = curr.join(".git");
            if git_candidate.is_dir() {
                return Some(git_candidate);
            } else if git_candidate.is_file() {
                if let Ok(content) = fs::read_to_string(&git_candidate) {
                    for line in content.lines() {
                        if let Some(rest) = line.strip_prefix("gitdir:") {
                            let p = PathBuf::from(rest.trim());
                            return Some(if p.is_relative() { curr.join(p) } else { p });
                        }
                    }
                }
            }
            if !curr.pop() {
                break;
            }
        }
    }

    None
}

/// Locate root working tree of the git repository
pub fn find_git_root() -> Option<PathBuf> {
    if let Ok(output) = Command::new("git").args(["rev-parse", "--show-toplevel"]).output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(PathBuf::from(s));
            }
        }
    }

    if let Ok(mut curr) = std::env::current_dir() {
        loop {
            if curr.join(".git").exists() {
                return Some(curr);
            }
            if !curr.pop() {
                break;
            }
        }
    }

    None
}

pub fn scan_directory(path: &Path, lang: Language) -> Result<ScanReport, String> {
    let start_time = Instant::now();
    let engine = ScannerEngine::new();

    println!("{}", lang.scan_start_all(&path.display().to_string()).cyan());

    let mut report = ScanReport::default();

    if path.is_file() {
        if !is_binary_file(path) {
            if let Ok(content) = fs::read_to_string(path) {
                report.files_scanned = 1;
                report.findings.extend(engine.scan_text(path, &content));
            }
        }
        report.duration_ms = start_time.elapsed().as_millis();
        return Ok(report);
    }

    let walker = WalkBuilder::new(path)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for result in walker {
        match result {
            Ok(entry) => {
                let entry_path = entry.path();
                if entry_path.is_file() {
                    if entry_path.components().any(|c| c.as_os_str() == ".git") {
                        continue;
                    }
                    if entry_path.file_name().map(|n| n == ".env.ferris").unwrap_or(false) {
                        continue;
                    }

                    if !is_binary_file(entry_path) {
                        if let Ok(content) = fs::read_to_string(entry_path) {
                            report.files_scanned += 1;
                            report.findings.extend(engine.scan_text(entry_path, &content));
                        }
                    }
                }
            }
            Err(_) => continue,
        }
    }

    report.duration_ms = start_time.elapsed().as_millis();
    Ok(report)
}

pub fn scan_staged(lang: Language) -> Result<ScanReport, String> {
    let start_time = Instant::now();
    let engine = ScannerEngine::new();

    println!("{}", lang.scan_start_staged().cyan());

    let repo_root = find_git_root().ok_or_else(|| {
        if lang.is_uz() {
            "❌ Git ombori topilmadi. Avval `git init` qiling.".to_string()
        } else {
            "❌ Not inside a git repository. Initialize with `git init` first.".to_string()
        }
    })?;

    // Fetch list of staged files via `git diff --cached --name-only -z --diff-filter=ACM`
    let output = Command::new("git")
        .current_dir(&repo_root)
        .args(["diff", "--cached", "--name-only", "-z", "--diff-filter=ACM"])
        .output()
        .map_err(|e| format!("Failed to run git diff: {}", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Git error: {}", err_str.trim()));
    }

    let stdout_bytes = output.stdout;
    let staged_files: Vec<String> = stdout_bytes
        .split(|&b| b == 0)
        .filter_map(|slice| {
            if slice.is_empty() {
                None
            } else {
                Some(String::from_utf8_lossy(slice).trim().to_string())
            }
        })
        .collect();

    if staged_files.is_empty() {
        println!("{}", lang.scan_no_staged_files().yellow());
        return Ok(ScanReport {
            files_scanned: 0,
            findings: Vec::new(),
            duration_ms: start_time.elapsed().as_millis(),
        });
    }

    let mut report = ScanReport::default();

    for file_str in staged_files {
        if file_str.ends_with(".env.ferris") {
            continue;
        }

        let p = Path::new(&file_str);

        // Extract staged content directly from the git index to avoid reading unstaged disk edits
        let show_output = Command::new("git")
            .current_dir(&repo_root)
            .args(["show", &format!(":{}", file_str)])
            .output();

        match show_output {
            Ok(out) if out.status.success() => {
                let bytes = out.stdout;
                if is_binary_bytes(&bytes) {
                    continue;
                }
                if let Ok(content) = String::from_utf8(bytes) {
                    report.files_scanned += 1;
                    report.findings.extend(engine.scan_text(p, &content));
                }
            }
            _ => {
                // Fallback to disk if git show was unable to read the blob
                let disk_path = repo_root.join(p);
                if disk_path.is_file() && !is_binary_file(&disk_path) {
                    if let Ok(content) = fs::read_to_string(&disk_path) {
                        report.files_scanned += 1;
                        report.findings.extend(engine.scan_text(p, &content));
                    }
                }
            }
        }
    }

    report.duration_ms = start_time.elapsed().as_millis();
    Ok(report)
}

pub fn install_git_hook(lang: Language) -> Result<PathBuf, String> {
    let git_dir = find_git_dir().ok_or_else(|| lang.hook_not_git_repo().to_string())?;

    let hooks_dir = git_dir.join("hooks");
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir).map_err(|e| format!("Failed to create .git/hooks: {}", e))?;
    }

    let hook_path = hooks_dir.join("pre-commit");

    let script_content = r#"#!/usr/bin/env sh
# ==============================================================================
# Ferris Pre-Commit Secret Guardian (ferris v0.3.0)
# Automatically scans staged files to prevent leaking API keys and secrets.
# ==============================================================================

PATH="$HOME/.cargo/bin:$PATH"

if command -v ferris >/dev/null 2>&1; then
    ferris scan --staged
    EXIT_CODE=$?
    if [ $EXIT_CODE -ne 0 ]; then
        echo ""
        echo "🚨 [Ferris Guardian] Commit blocked due to detected secrets!"
        echo "💡 Tip: Use 'ferris vault seal' to encrypt secrets before committing."
        exit 1
    fi
else
    echo "⚠️  [Ferris Guardian] 'ferris' CLI not found in PATH. Skipping secret scan."
fi
"#;

    fs::write(&hook_path, script_content)
        .map_err(|e| format!("Failed to write hook file: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook_path)
            .map_err(|e| e.to_string())?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)
            .map_err(|e| format!("Failed to make hook executable: {}", e))?;
    }

    println!("{}", lang.hook_installed_success(&hook_path.display().to_string()).green().bold());
    Ok(hook_path)
}

pub fn print_scan_report(report: &ScanReport, lang: Language) {
    if !report.has_secrets() {
        println!("{}", lang.scan_clean(report.files_scanned, report.duration_ms).green().bold());
        return;
    }

    eprintln!("\n{}", lang.scan_failed_summary(report.findings.len(), report.files_scanned).red().bold());
    eprintln!("{}\n", "─".repeat(70).red());

    for (idx, finding) in report.findings.iter().enumerate() {
        eprintln!(
            "  {}. {} {}",
            (idx + 1).to_string().yellow().bold(),
            finding.rule_name.white().bold(),
            format!("(Line {})", finding.line_number).dimmed()
        );
        eprintln!("     File:   {}", finding.file_path.display().to_string().cyan());
        eprintln!("     Secret: {}", finding.masked_secret.magenta().bold());
        eprintln!("     Code:   {}", finding.snippet.dimmed());
        eprintln!();
    }

    eprintln!("{}\n", "─".repeat(70).red());
    if lang.is_uz() {
        eprintln!("{}", "💡 Tavsiya: Sirlarni .env faylida saqlang va 'ferris vault seal' bilan shifrlang!".yellow());
    } else {
        eprintln!("{}", "💡 Tip: Store credentials in .env and encrypt them using 'ferris vault seal'!".yellow());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_secret() {
        assert_eq!(mask_secret("12345"), "********");
        assert_eq!(mask_secret("123456789012"), "123****12");
        let sample = format!("sk-proj-{}", "1234567890abcdef");
        assert_eq!(mask_secret(&sample), "sk-p****...****cdef");
    }

    #[test]
    fn test_mask_secret_unicode() {
        let sample = "🦀🦀🦀🦀secret🦀🦀🦀🦀";
        let masked = mask_secret(sample);
        assert!(!masked.is_empty());
    }

    #[test]
    fn test_shannon_entropy() {
        assert_eq!(shannon_entropy(""), 0.0);
        assert_eq!(shannon_entropy("aaaa"), 0.0);
        let ent = shannon_entropy("gK8#mQ2$xP9!vL5&");
        assert!(ent > 3.5);
    }

    #[test]
    fn test_detect_openai_key() {
        let engine = ScannerEngine::new();
        let key = format!("sk-proj-{}", "abc1234567890defghijk");
        let content = format!("const client = new OpenAI({{ apiKey: '{}' }});", key);
        let findings = engine.scan_text(Path::new("test.ts"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "OpenAI / Anthropic API Key");
        assert_eq!(findings[0].masked_secret, "sk-p****...****hijk");
    }

    #[test]
    fn test_detect_anthropic_key() {
        let engine = ScannerEngine::new();
        let key = format!("sk-ant-api03-{}", "abc1234567890defghijklmnopqrst");
        let content = format!("ANTHROPIC_API_KEY='{}'", key);
        let findings = engine.scan_text(Path::new("test.env"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "OpenAI / Anthropic API Key");
    }

    #[test]
    fn test_detect_aws_access_key() {
        let engine = ScannerEngine::new();
        let key = format!("AKIA{}", "IOSFODNN7EXAMPLE");
        let content = format!("AWS_ACCESS_KEY_ID={}", key);
        let findings = engine.scan_text(Path::new("test.env"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "AWS Access Key ID");
        assert_eq!(findings[0].masked_secret, "AKIA****...****MPLE");
    }

    #[test]
    fn test_detect_aws_secret_key() {
        let engine = ScannerEngine::new();
        let secret = format!("{}/K7MDENG/bPxRfiCYEXAMPLEKEY", "wJalrXUtnFEMI");
        let content = format!("aws_secret_access_key = '{}'", secret);
        let findings = engine.scan_text(Path::new("credentials"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "AWS Secret Access Key");
        assert_eq!(findings[0].masked_secret, "wJal****...****EKEY");
    }

    #[test]
    fn test_detect_stripe_secret() {
        let engine = ScannerEngine::new();
        let key = format!("sk_live_{}", "51AbcdefghijklmnopqrstuvwxyZ");
        let content = format!("STRIPE_KEY = '{}'", key);
        let findings = engine.scan_text(Path::new("settings.py"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "Stripe Secret Key");
    }

    #[test]
    fn test_detect_telegram_token() {
        let engine = ScannerEngine::new();
        let token = format!("{}:{}", "123456789", "ABCdefGHIjklMNOpqrSTUvwxYZ123456789");
        let content = format!("bot = Bot('{}')", token);
        let findings = engine.scan_text(Path::new("bot.py"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "Telegram Bot Token");
    }

    #[test]
    fn test_detect_db_uri() {
        let engine = ScannerEngine::new();
        let uri = format!("postgres://{}:{}@{}/{}", "admin", "supersecretpass", "db.example.com:5432", "production");
        let content = format!("DATABASE_URL={}", uri);
        let findings = engine.scan_text(Path::new("db.env"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "Database Connection String");
    }

    #[test]
    fn test_detect_redis_uri() {
        let engine = ScannerEngine::new();
        let uri = format!("redis://:{}{}", "supersecretpassword@", "127.0.0.1:6379/0");
        let content = format!("REDIS_URL='{}'", uri);
        let findings = engine.scan_text(Path::new("app.env"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "Database Connection String");
    }

    #[test]
    fn test_detect_private_key() {
        let engine = ScannerEngine::new();
        let key_header = format!("-----BEGIN {} PRIVATE KEY-----", "RSA");
        let content = format!("{}\nMIIEowIBAAKCAQEA0...\n-----END RSA PRIVATE KEY-----", key_header);
        let findings = engine.scan_text(Path::new("id_rsa"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "Private Cryptographic Key");
    }

    #[test]
    fn test_detect_pgp_key() {
        let engine = ScannerEngine::new();
        let content = format!("-----BEGIN {} PRIVATE KEY BLOCK-----\nlV4EY1...\n-----END PGP PRIVATE KEY BLOCK-----", "PGP");
        let findings = engine.scan_text(Path::new("secring.asc"), &content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "Private Cryptographic Key");
    }

    #[test]
    fn test_detect_unquoted_env_assignment() {
        let engine = ScannerEngine::new();
        let content = "API_KEY=9f8a7b6c5d4e3f2a1b0c9d8e7f6a5b4c";
        let findings = engine.scan_text(Path::new(".env"), content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule_name, "High-Entropy Secret Token");
    }

    #[test]
    fn test_clean_file_no_false_positives() {
        let engine = ScannerEngine::new();
        let content = r#"
            // Standard Rust application code
            fn main() {
                println!("Hello world!");
                let placeholder = "your_api_key_here";
                let count = 42;
                let token = config.token.as_deref().unwrap_or("");
                let secret_setting = default_setting.clone();
            }
        "#;
        let findings = engine.scan_text(Path::new("main.rs"), content);
        assert!(findings.is_empty());
    }
}
