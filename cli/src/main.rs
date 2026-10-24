use clap::{Parser, Subcommand};
use colored::*;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message as WsMessage;

mod vault;
mod jail;

#[derive(Parser)]
#[command(name = "ferris")]
#[command(author = "Ferris.uz Hamjamiyati")]
#[command(version)]
#[command(about = "🦀 Ferris.uz rasmiy terminal ilovasi (CLI) — Rust Ekotizimi", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Backend API manzili (default: https://api.ferris.uz)
    #[arg(short, long, default_value = "https://api.ferris.uz")]
    api: String,
}

#[derive(Subcommand)]
enum Commands {
    /// 📡 Tizim holati va Cloudflare marshrutlashni tekshirish
    Ping,

    /// 📌 Mavzularni ko'rish (topics.ferris.uz)
    Topic {
        #[command(subcommand)]
        action: TopicCommands,
    },

    /// 👥 Jamiyatlarni ko'rish va boshqarish (groups.ferris.uz)
    Group {
        #[command(subcommand)]
        action: GroupCommands,
    },

    /// 💬 Muhokamalar va yangi postlarni ko'rish, o'qish hamda fikr bildirish
    Post {
        #[command(subcommand)]
        action: PostCommands,
    },

    /// 📰 Yangi muhokamalar tasmasini ko'rish (ferris post list ga qisqartma)
    Feed,

    /// 📖 Postni to'liq o'qish (ferris post read ga qisqartma)
    Read {
        /// Post ID yoki raqami (masalan: 1 yoki 8ae64e69)
        id: String,
    },

    /// ⚡ Jonli Real-Time Hamjamiyat Chati va Yangiliklar (WebSocket)
    Live {
        /// Xona nomi (default: general)
        #[arg(short, long, default_value = "general")]
        room: String,
    },

    /// 💬 Jonli chat (ferris live ga qisqartma)
    Chat {
        /// Xona nomi (default: general)
        #[arg(short, long, default_value = "general")]
        room: String,
    },

    /// 👤 Profilni ko'rish (profile.ferris.uz/@username)
    Profile {
        /// Foydalanuvchi nomi (agar ko'rsatilmasa, shaxsiy hisob)
        username: Option<String>,
    },

    /// ✉️ Shaxsiy xabarlar (Direct Messages)
    Dm {
        #[command(subcommand)]
        action: DmCommands,
    },

    /// 🔐 Ferris.uz hisobiga terminal orqali ulanish
    Login {
        /// Sessiya tokeni (ferris.uz/profile dan olinadi)
        token: Option<String>,
    },

    /// 🚪 Terminaldagi sessiyadan chiqish
    Logout,

    /// 🚫 Foydalanuvchini bloklash va bloklanganlar ro'yxati
    Block {
        #[command(subcommand)]
        action: BlockCommands,
    },

    /// ✅ Foydalanuvchini blokdan chiqarish (ferris block remove ga qisqartma)
    Unblock {
        /// Foydalanuvchi nomi (@username)
        username: String,
    },

    /// 🛡️ Maxfiylik siyosati (Privacy Policy)
    Privacy,

    /// 📜 Foydalanish qoidalari (Terms of Service)
    Terms,

    /// ⚖️ Ochiq kodli litsenziya shartlari (Dual MIT / Apache 2.0)
    License,

    /// 📚 O‘zbekcha Rasmiy Rust Hujjatlari va Kitobi (doc.ferris.uz)
    Doc {
        /// Bob raqami yoki mavzu (masalan: 1, 4, ownership, cargo, glossary)
        chapter: Option<String>,

        /// Brauzerda doc.ferris.uz portalini ochish
        #[arg(short, long)]
        web: bool,
    },

    /// 🔐 Xavfsiz xotiradagi sirlar ombori (Zero-Disk Secrets Vault)
    Vault {
        #[command(subcommand)]
        action: VaultCommands,
    },

    /// 🚀 Ilovani xavfsiz qobiqda (Enclave) ishga tushirish
    Run {
        /// Shifrlangan sirlar ombori (.env.ferris) yo'li
        #[arg(short, long, default_value = ".env.ferris")]
        vault: String,

        /// Vault uchun maxsus parol (agar o'rnatilgan bo'lsa)
        #[arg(short, long)]
        password: Option<String>,

        /// Yadroviy qamalni (Landlock LSM) yoqish
        #[arg(short, long, default_value_t = true)]
        jail: bool,

        /// Loyiha ildiziga yozishga ruxsat berish (Tavsiya etilmaydi, default: faqat o'qish)
        #[arg(long, default_value_t = false)]
        allow_cwd_write: bool,

        /// To'liq tarmoq izolyatsiyasi (Tashqi internetni uzish)
        #[arg(short = 'n', long, default_value_t = false)]
        isolated_net: bool,

        /// Qamal ichida qo'shimcha yozishga ruxsat etilgan papkalar (masalan: ./logs, ./data)
        #[arg(long)]
        allow_write: Vec<String>,

        /// Unix Domain Socket yo'li (ixtiyoriy, masalan: /run/ferris/app.sock)
        #[arg(short, long)]
        socket: Option<String>,

        /// Bajariladigan buyruq va argumentlari (masalan: ferris run -- python3 bot.py)
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
}

#[derive(Subcommand)]
enum VaultCommands {
    /// Ochiq .env faylni shifrlab muhrlash va diskdan tozalash
    Seal {
        /// Kiruvchi fayl yo'li (default: .env)
        #[arg(default_value = ".env")]
        input: String,

        /// Shifrlangan fayl yo'li (default: .env.ferris)
        #[arg(short, long, default_value = ".env.ferris")]
        output: String,

        /// Shifrlash uchun maxsus parol (ixtiyoriy, berilmasa mashina kaliti ishlatiladi)
        #[arg(short, long)]
        password: Option<String>,

        /// Asl ochiq faylni diskda saqlab qolish (Tavsiya etilmaydi!)
        #[arg(long, default_value_t = false)]
        keep: bool,
    },

    /// Vault tarkibidagi kalitlar ro'yxatini ko'rish
    Inspect {
        /// Shifrlangan fayl yo'li (default: .env.ferris)
        #[arg(default_value = ".env.ferris")]
        vault: String,

        /// Parol (agar seal paytida kiritilgan bo'lsa)
        #[arg(short, long)]
        password: Option<String>,

        /// Maxfiy qiymatlarni ham ekranda ko'rsatish
        #[arg(long, default_value_t = false)]
        reveal: bool,
    },

    /// Shifrlangan vaultni ochib, qayta .env fayliga chiqarish
    Unseal {
        /// Shifrlangan fayl yo'li (default: .env.ferris)
        #[arg(default_value = ".env.ferris")]
        vault: String,

        /// Tiklanadigan fayl yo'li (default: .env)
        #[arg(short, long, default_value = ".env")]
        output: String,

        /// Parol (agar seal paytida kiritilgan bo'lsa)
        #[arg(short, long)]
        password: Option<String>,
    },
}

#[derive(Subcommand)]
enum BlockCommands {
    /// Foydalanuvchini bloklash
    Add {
        /// Foydalanuvchi nomi (@username)
        username: String,
    },
    /// Foydalanuvchini blokdan chiqarish
    Remove {
        /// Foydalanuvchi nomi (@username)
        username: String,
    },
    /// Bloklangan barcha foydalanuvchilar ro'yxati
    List,
}

#[derive(Subcommand)]
enum DmCommands {
    /// Barcha shaxsiy suhbatlar ro'yxati
    List,
    /// Foydalanuvchi bilan yozishish yoki chat tarixini ko'rish
    Chat {
        /// Qabul qiluvchi foydalanuvchi nomi (@username)
        username: String,
        /// Yuboriladigan xabar matni (agar kiritilmasa, chat tarixi ko'rsatiladi)
        text: Option<String>,
    },
    /// Foydalanuvchiga to'g'ridan-to'g'ri xabar yuborish
    Send {
        /// Qabul qiluvchi foydalanuvchi nomi (@username)
        username: String,
        /// Xabar matni
        text: String,
    },
    /// Shaxsiy xabarni o'chirish
    Delete {
        /// Xabar identifikatori (UUID)
        id: String,
    },
}

#[derive(Subcommand)]
enum TopicCommands {
    /// Barcha faol mavzular ro'yxati
    List,
}

#[derive(Subcommand)]
enum GroupCommands {
    /// Barcha faol jamiyatlar ro'yxati
    List,
    /// Jamiyatga a'zo bo'lish
    Join {
        slug: String,
    },
    /// Yangi jamiyat yaratish
    Create {
        slug: String,
        name: String,
        #[arg(short, long)]
        description: Option<String>,
    },
}

#[derive(Subcommand)]
enum PostCommands {
    /// Barcha yangi muhokamalar va postlar ro'yxati
    List,
    /// Postni to'liq matnini va fikrlarni o'qish
    Read {
        /// Post raqami (1, 2...) yoki UUID identifikatori
        id: String,
    },
    /// Postga fikr/izoh bildirish
    Comment {
        /// Post raqami yoki ID
        id: String,
        /// Fikr matni
        content: String,
    },
    /// Yangi muhokama chop etish
    Create {
        /// Mavzu slugi (masalan: f/rust, f/async)
        #[arg(short, long)]
        topic: String,

        /// Sarlavha
        #[arg(short, long)]
        title: String,

        /// Post matni
        #[arg(short, long)]
        content: String,
    },
    /// Postni tahrirlash (faqat muallif yoki admin)
    Edit {
        /// Post raqami yoki ID
        id: String,
        /// Yangi sarlavha
        #[arg(short, long)]
        title: Option<String>,
        /// Yangi matn
        #[arg(short, long)]
        content: Option<String>,
    },
    /// Postni butunlay o'chirish (faqat muallif yoki admin)
    Delete {
        /// Post raqami yoki ID
        id: String,
    },
    /// Izohni o'chirish (faqat muallif yoki admin)
    DeleteComment {
        /// Izoh UUID identifikatori
        id: String,
    },
}

#[derive(Deserialize, Debug)]
struct TopicItem {
    slug: String,
    title: String,
    description: Option<String>,
    icon: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct GroupItem {
    slug: String,
    name: String,
    description: Option<String>,
    members_count: i32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct PostListItem {
    id: String,
    title: String,
    content: String,
    views_count: i32,
    author_name: Option<String>,
    created_at: String,
    comments_count: Option<usize>,
}

#[derive(Deserialize, Serialize, Debug)]
struct CommentItem {
    id: String,
    author_name: String,
    content: String,
    created_at: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct PostDetailResponse {
    id: String,
    title: String,
    content: String,
    views_count: i32,
    author_name: Option<String>,
    created_at: String,
    comments: Vec<CommentItem>,
}

pub fn compute_chat_hash(id: &str, room: &str, username: &str, text: &str, timestamp: u64, prev_hash: Option<&str>) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"FERRIS-PQC-CHAT-V1:");
    hasher.update(id.as_bytes());
    hasher.update(b":");
    hasher.update(room.as_bytes());
    hasher.update(b":");
    hasher.update(username.as_bytes());
    hasher.update(b":");
    hasher.update(timestamp.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(prev_hash.unwrap_or("GENESIS").as_bytes());
    hasher.update(b":");
    hasher.update(text.as_bytes());
    hasher.finalize().to_hex().to_string()
}

pub fn compute_dm_hash(id: &str, sender: &str, receiver: &str, text: &str, timestamp: u64) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"FERRIS-PQC-DM-V1:");
    hasher.update(id.as_bytes());
    hasher.update(b":");
    hasher.update(sender.as_bytes());
    hasher.update(b":");
    hasher.update(receiver.as_bytes());
    hasher.update(b":");
    hasher.update(timestamp.to_string().as_bytes());
    hasher.update(b":");
    hasher.update(text.as_bytes());
    hasher.finalize().to_hex().to_string()
}

#[derive(Deserialize, Debug)]
struct ChatUserProfile {
    username: String,
    #[allow(dead_code)]
    display_name: Option<String>,
}

#[derive(Deserialize, Debug)]
struct ChatMessageIncoming {
    id: Option<String>,
    room: Option<String>,
    user: ChatUserProfile,
    text: String,
    timestamp: Option<u64>,
    #[serde(default)]
    prev_hash: Option<String>,
    #[serde(default)]
    msg_hash: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "event", content = "data")]
#[allow(dead_code)]
enum WsEventIncoming {
    #[serde(rename = "community_chat")]
    CommunityChat(ChatMessageIncoming),

    #[serde(rename = "direct_message")]
    DirectMessage {
        id: String,
        sender_username: String,
        receiver_username: String,
        text: String,
        timestamp: u64,
        #[serde(default)]
        msg_hash: Option<String>,
    },

    #[serde(rename = "post_comment")]
    PostComment {
        id: String,
        post_id: String,
        post_title: String,
        author_name: String,
        content: String,
        timestamp: u64,
    },

    #[serde(rename = "new_post")]
    NewPost {
        id: String,
        topic_slug: Option<String>,
        group_slug: Option<String>,
        author_name: String,
        title: String,
        content: String,
        timestamp: u64,
    },

    #[serde(rename = "group_activity")]
    GroupActivity {
        group_slug: String,
        username: String,
        action: String,
        members_count: i32,
        timestamp: u64,
    },
}

#[derive(Serialize, Deserialize, Default)]
struct CliConfig {
    token: Option<String>,
    username: Option<String>,
}

fn get_config_dir() -> PathBuf {
    let mut p = dirs_fallback();
    p.push(".config");
    p.push("ferris");
    let _ = fs::create_dir_all(&p);
    p
}

fn dirs_fallback() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else {
        PathBuf::from(".")
    }
}

fn load_config() -> CliConfig {
    let mut path = get_config_dir();
    path.push("config.json");
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(cfg) = serde_json::from_slice(&bytes) {
            return cfg;
        }
    }
    CliConfig::default()
}

fn save_config(cfg: &CliConfig) {
    let mut path = get_config_dir();
    path.push("config.json");
    if let Ok(json) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(&path, json);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }
}

fn save_posts_cache(posts: &[PostListItem]) {
    let mut path = get_config_dir();
    path.push("posts_cache.json");
    if let Ok(json) = serde_json::to_string(posts) {
        let _ = fs::write(path, json);
    }
}

fn load_posts_cache() -> Vec<PostListItem> {
    let mut path = get_config_dir();
    path.push("posts_cache.json");
    if let Ok(bytes) = fs::read(&path) {
        if let Ok(posts) = serde_json::from_slice(&bytes) {
            return posts;
        }
    }
    Vec::new()
}

fn resolve_post_id(raw_id: &str) -> String {
    let trimmed = raw_id.trim();
    if let Ok(num) = trimmed.parse::<usize>() {
        let cache = load_posts_cache();
        if num >= 1 && num <= cache.len() {
            return cache[num - 1].id.clone();
        }
    }
    let cache = load_posts_cache();
    for p in &cache {
        if p.id.starts_with(trimmed) {
            return p.id.clone();
        }
    }
    trimmed.to_string()
}

fn format_date(dt: &str) -> String {
    if dt.len() >= 16 {
        dt[..16].replace('T', " ")
    } else {
        dt.to_string()
    }
}

fn format_ts(ts: u64) -> String {
    if ts > 0 {
        chrono::DateTime::from_timestamp(ts as i64, 0)
            .map(|d| d.format("[%H:%M:%S]").to_string())
            .unwrap_or_default()
    } else {
        "".to_string()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();
    let config = load_config();

    match &cli.command {
        Commands::Ping => {
            println!("{}", "🦀 Ferris.uz API tekshirilmoqda...".cyan());
            let res = client.get(format!("{}/api/v1/context", cli.api)).send().await;
            match res {
                Ok(resp) if resp.status().is_success() => {
                    let body: serde_json::Value = resp.json().await?;
                    println!("{} {}", "✅ API aloqada:".green().bold(), cli.api.yellow());
                    println!("  Host: {}", body["host"]);
                    println!("  Subdomain: {}", body["subdomain"].to_string().cyan());
                    println!("  SSL: {}", body["ssl_mode"].to_string().green());
                }
                Ok(resp) => {
                    println!("{} Server status: {}", "⚠️ Xatolik:".yellow(), resp.status());
                }
                Err(err) => {
                    println!("{} {}", "❌ API ga ulanib bo'lmadi:".red().bold(), err);
                }
            }
        }

        Commands::Topic { action } => match action {
            TopicCommands::List => {
                println!("{}", "📌 Ferris.uz Mavzulari (topics.ferris.uz):".yellow().bold());
                let res = client.get(format!("{}/api/v1/topics", cli.api)).send().await;
                match res {
                    Ok(resp) if resp.status().is_success() => {
                        let topics: Vec<TopicItem> = resp.json().await?;
                        for t in topics {
                            let desc = t.description.unwrap_or_default();
                            println!("  {} {:<14} - {} ({})", t.icon, t.slug.cyan().bold(), t.title.white(), desc.dimmed());
                        }
                    }
                    _ => println!("{}", "Mavzularni yuklab bo'lmadi.".red()),
                }
            }
        },

        Commands::Group { action } => match action {
            GroupCommands::List => {
                println!("{}", "👥 Faol Jamiyatlar (groups.ferris.uz):".magenta().bold());
                let res = client.get(format!("{}/api/v1/groups", cli.api)).send().await;
                match res {
                    Ok(resp) if resp.status().is_success() => {
                        let groups: Vec<GroupItem> = resp.json().await?;
                        for g in groups {
                            let status = if g.members_count == 0 {
                                "⚠️ Bo'sh (24h TTL o'chadi)".yellow()
                            } else {
                                format!("[A'zolar: {}]", g.members_count).green()
                            };
                            println!("  • {:<18} {} - {}", g.slug.cyan().bold(), status, g.name.white());
                        }
                    }
                    _ => println!("{}", "Jamiyatlarni yuklab bo'lmadi.".red()),
                }
                println!("\n{}", "ℹ️ Qoida: Agar jamiyatda a'zo 0 ga tushsa, 24 soatda avtomatik o'chiriladi.".dimmed());
            }

            GroupCommands::Join { slug } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Jamiyatga a'zo bo'lish uchun avval tizimga kiring:".yellow());
                    println!("  ferris login <TOKEN>");
                    return Ok(());
                }

                println!("{} '{}' jamiyatiga a'zo bo'linmoqda...", "🚀".cyan(), slug.bold());
                let res = client
                    .post(format!("{}/api/v1/groups/{}/join", cli.api, slug))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        let val: serde_json::Value = resp.json().await?;
                        let is_member = val["is_member"].as_bool().unwrap_or(true);
                        if is_member {
                            println!("{} Tabriklaymiz! Siz f/{} jamiyatiga muvaffaqiyatli a'zo bo'ldingiz! Jami a'zolar: {}", "✅".green().bold(), slug, val["members_count"]);
                        } else {
                            println!("{} Siz f/{} jamiyati a'zoligidan chiqdingiz. Qolgan a'zolar: {}", "ℹ️".yellow(), slug, val["members_count"]);
                        }
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await?;
                        println!("{} {}", "❌ Xatolik:".red(), err["error"]);
                    }
                    Err(e) => println!("{} {}", "❌ Tarmoq xatosi:".red(), e),
                }
            }

            GroupCommands::Create { slug, name, description } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Jamiyat yaratish uchun tizimga kirishingiz shart:".yellow());
                    println!("  ferris login <TOKEN>");
                    return Ok(());
                }

                println!("{} Yangi jamiyat yaratilmoqda: {}", "🛠️".cyan(), slug.bold());
                let payload = serde_json::json!({
                    "slug": slug,
                    "name": name,
                    "description": description
                });
                let res = client
                    .post(format!("{}/api/v1/groups", cli.api))
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&payload)
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        println!("{} Jamiyat muvaffaqiyatli yaratildi! URL: https://groups.ferris.uz/f/{}", "✅".green().bold(), slug);
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await?;
                        println!("{} {}", "❌ Xatolik:".red(), err["error"]);
                    }
                    Err(e) => println!("{} {}", "❌ Tarmoq xatosi:".red(), e),
                }
            }
        },

        Commands::Feed => {
            render_posts_list(&client, &cli.api).await?;
        }

        Commands::Read { id } => {
            render_post_detail(&client, &cli.api, id).await?;
        }

        Commands::Live { room } | Commands::Chat { room } => {
            run_live_chat(room, &config).await?;
        }

        Commands::Post { action } => match action {
            PostCommands::List => {
                render_posts_list(&client, &cli.api).await?;
            }

            PostCommands::Read { id } => {
                render_post_detail(&client, &cli.api, id).await?;
            }

            PostCommands::Comment { id, content } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Fikr yozish uchun avval tizimga kiring:".yellow());
                    println!("  ferris login <TOKEN>");
                    return Ok(());
                }

                let real_id = resolve_post_id(id);
                println!("{} Fikringiz chop etilmoqda...", "💬".cyan());
                let payload = serde_json::json!({ "content": content });
                let res = client
                    .post(format!("{}/api/v1/posts/{}/comments", cli.api, real_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&payload)
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        println!("{} Fikringiz muvaffaqiyatli qo'shildi! Postni ko'rish: ferris post read {}", "✅".green().bold(), id);
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await?;
                        println!("{} {}", "❌ Xatolik:".red(), err["error"]);
                    }
                    Err(e) => println!("{} {}", "❌ Tarmoq xatosi:".red(), e),
                }
            }

            PostCommands::Create { topic, title, content } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Muhokama ochish uchun tizimga kirish shart:".yellow());
                    println!("  ferris login <TOKEN>");
                    return Ok(());
                }

                println!("{}", "📝 Yangi muhokama chop etilmoqda...".cyan());
                let payload = serde_json::json!({
                    "topic_slug": topic,
                    "title": title,
                    "content": content
                });
                let res = client
                    .post(format!("{}/api/v1/posts", cli.api))
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&payload)
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        println!("{} Post qabul qilindi va chop etildi! Ko'rish uchun: ferris feed", "✅".green().bold());
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await?;
                        println!("{} {}", "❌ Xatolik:".red(), err["error"]);
                    }
                    Err(e) => println!("{} {}", "❌ Tarmoq xatosi:".red(), e),
                }
            }

            PostCommands::Edit { id, title, content } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Postni tahrirlash uchun tizimga kirish shart:".yellow());
                    println!("  ferris login <TOKEN>");
                    return Ok(());
                }

                let real_id = resolve_post_id(id);
                println!("{} Post tahrirlanmoqda...", "✏️".cyan());
                let mut payload = serde_json::Map::new();
                if let Some(t) = title {
                    payload.insert("title".to_string(), serde_json::Value::String(t.clone()));
                }
                if let Some(c) = content {
                    payload.insert("content".to_string(), serde_json::Value::String(c.clone()));
                }

                let res = client
                    .put(format!("{}/api/v1/posts/{}", cli.api, real_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .json(&payload)
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        println!("{} Post muvaffaqiyatli tahrirlandi! O'qish: ferris post read {}", "✅".green().bold(), id);
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await?;
                        println!("{} {}", "❌ Xatolik:".red(), err["error"]);
                    }
                    Err(e) => println!("{} {}", "❌ Tarmoq xatosi:".red(), e),
                }
            }

            PostCommands::Delete { id } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Postni o'chirish uchun tizimga kirish shart:".yellow());
                    println!("  ferris login <TOKEN>");
                    return Ok(());
                }

                let real_id = resolve_post_id(id);
                println!("{} Post o'chirilmoqda...", "🗑️".yellow());
                let res = client
                    .delete(format!("{}/api/v1/posts/{}", cli.api, real_id))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        println!("{} Post va barcha bog'liq izohlar PostgreSQL dan butunlay o'chirildi!", "✅".green().bold());
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await?;
                        println!("{} {}", "❌ Xatolik:".red(), err["error"]);
                    }
                    Err(e) => println!("{} {}", "❌ Tarmoq xatosi:".red(), e),
                }
            }

            PostCommands::DeleteComment { id } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Izohni o'chirish uchun tizimga kirish shart:".yellow());
                    println!("  ferris login <TOKEN>");
                    return Ok(());
                }

                println!("{} Izoh o'chirilmoqda...", "🗑️".yellow());
                let res = client
                    .delete(format!("{}/api/v1/comments/{}", cli.api, id))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        println!("{} Izoh muvaffaqiyatli o'chirildi!", "✅".green().bold());
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await?;
                        println!("{} {}", "❌ Xatolik:".red(), err["error"]);
                    }
                    Err(e) => println!("{} {}", "❌ Tarmoq xatosi:".red(), e),
                }
            }
        },

        Commands::Profile { username } => {
            let target_user = if let Some(u) = username {
                u.clone()
            } else if let Some(cfg_user) = &config.username {
                cfg_user.clone()
            } else {
                "ibrohimkoha".to_string()
            };

            let res = client.get(format!("{}/api/v1/profile/{}", cli.api, target_user)).send().await;
            match res {
                Ok(resp) if resp.status().is_success() => {
                    let user: serde_json::Value = resp.json().await?;
                    println!("{} {}", "👤 Profil:".blue().bold(), user["display_name"]);
                    println!("  Subdomen URL: {}", user["subdomain_url"].to_string().cyan());
                    println!("  Qiziqishlar: {}", user["interests"]);
                    if let Some(cfg_user) = &config.username {
                        if cfg_user == &target_user {
                            println!("  {} Faol terminal sessiyasi", "🔑".green());
                        }
                    }
                }
                _ => {
                    println!("{} Foydalanuvchi topilmadi.", "❌".red());
                }
            }
        }

        Commands::Dm { action } => {
            handle_dm_command(action, &client, &cli.api, &config).await?;
        }

        Commands::Login { token } => {
            if let Some(t) = token {
                // Personal Access Token (PAT) yoki Mavjud Sessiya tekshiruvi
                let check_res = client
                    .get(format!("{}/api/auth/me", cli.api))
                    .header("Authorization", format!("Bearer {}", t))
                    .send()
                    .await;

                match check_res {
                    Ok(resp) if resp.status().is_success() => {
                        let user: serde_json::Value = resp.json().await?;
                        let uname = user["username"].as_str().unwrap_or("user").to_string();
                        let new_cfg = CliConfig {
                            token: Some(t.clone()),
                            username: Some(uname.clone()),
                        };
                        save_config(&new_cfg);
                        println!("{} Muvaffaqiyatli kirdingiz, @{}!", "✅".green().bold(), uname.cyan().bold());
                        println!("🔑 Sessiya tokeni ~/.config/ferris/config.toml ga saqlandi.");
                    }
                    _ => {
                        println!("{} Kiritilgan token yaroqsiz yoki muddati o'tgan.", "❌".red());
                    }
                }
            } else {
                // OAuth 2.0 Device Flow (RFC 8628)
                match perform_device_login(&client, &cli.api, "ferris-cli").await {
                    Ok(Some((access_token, uname))) => {
                        let new_cfg = CliConfig {
                            token: Some(access_token),
                            username: Some(uname.clone()),
                        };
                        save_config(&new_cfg);
                        println!("\n{} Muvaffaqiyatli ulandi! Xush kelibsiz, @{}!", "✅".green().bold(), uname.cyan().bold());
                        println!("🔑 Shaxsiy CLI tokeni (PAT) ~/.config/ferris/config.toml ga saqlandi.");
                        println!("Terminalda hamjamiyat, postlar, chat va barcha buyruqlar to'liq faollashtirildi.");
                    }
                    Ok(None) => {
                        // Foydalanuvchi bekor qildi yoki xatolik haqida xabar chiqdi
                    }
                    Err(e) => {
                        println!("{} Ulanishda xatolik yuz berdi: {}", "❌".red(), e);
                    }
                }
            }
        }

        Commands::Logout => {
            let empty_cfg = CliConfig::default();
            save_config(&empty_cfg);
            println!("{} Terminaldagi barcha sessiyalar muvaffaqiyatli tozalandi.", "✅".green());
        }

        Commands::Block { action } => match action {
            BlockCommands::Add { username } => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Bloklash uchun tizimga kirish shart: ferris login <TOKEN>".yellow());
                    return Ok(());
                }
                let clean_user = username.trim_start_matches('@');
                println!("{} @{} bloklanmoqda...", "🚫".yellow(), clean_user);
                let res = client
                    .post(format!("{}/api/v1/users/{}/block", cli.api, clean_user))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;
                match res {
                    Ok(resp) if resp.status().is_success() => {
                        println!("{} @{} muvaffaqiyatli bloklandi! U endi sizga DM yubora olmaydi.", "✅".green().bold(), clean_user);
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await.unwrap_or_default();
                        println!("{} {}", "❌ Xatolik:".red(), err["error"].as_str().unwrap_or("Bloklab bo'lmadi"));
                    }
                    Err(e) => println!("{} Tarmoq xatosi: {}", "❌".red(), e),
                }
            }
            BlockCommands::Remove { username } => {
                handle_unblock(&client, &cli.api, &config, username).await?;
            }
            BlockCommands::List => {
                let token = config.token.as_deref().unwrap_or("");
                if token.is_empty() {
                    println!("{}", "⚠️ Bloklanganlar ro'yxatini ko'rish uchun tizimga kirish shart: ferris login <TOKEN>".yellow());
                    return Ok(());
                }
                let res = client
                    .get(format!("{}/api/v1/blocked-users", cli.api))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;
                match res {
                    Ok(resp) if resp.status().is_success() => {
                        let val: serde_json::Value = resp.json().await?;
                        let list = val["blocked_users"].as_array();
                        println!("{}", "🚫 Bloklangan Foydalanuvchilar:".yellow().bold());
                        if let Some(users) = list {
                            if users.is_empty() {
                                println!("  Hozircha hech kim bloklanmagan.");
                            } else {
                                for u in users {
                                    let name = u.as_str().or_else(|| u["username"].as_str()).unwrap_or("");
                                    println!("  • @{}", name.cyan());
                                }
                                println!("\n💡 Blokdan chiqarish: ferris unblock @username");
                            }
                        }
                    }
                    Ok(resp) => {
                        let err: serde_json::Value = resp.json().await.unwrap_or_default();
                        println!("{} {}", "❌ Xatolik:".red(), err["error"].as_str().unwrap_or("Yuklab bo'lmadi"));
                    }
                    Err(e) => println!("{} Tarmoq xatosi: {}", "❌".red(), e),
                }
            }
        },

        Commands::Unblock { username } => {
            handle_unblock(&client, &cli.api, &config, username).await?;
        }

        Commands::Privacy => {
            print_privacy_policy();
        }

        Commands::Terms => {
            print_terms_of_service();
        }

        Commands::License => {
            print_license();
        }

        Commands::Doc { chapter, web } => {
            handle_doc(chapter.as_deref(), *web).await?;
        }

        Commands::Vault { action } => {
            handle_vault(action)?;
        }

        Commands::Run {
            vault,
            password,
            jail,
            allow_cwd_write,
            isolated_net,
            allow_write,
            socket,
            command,
        } => {
            handle_run(
                vault,
                password.as_deref(),
                *jail,
                *allow_cwd_write,
                *isolated_net,
                allow_write,
                socket.as_deref(),
                command,
            )?;
        }
    }

    Ok(())
}

fn handle_vault(action: &VaultCommands) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        VaultCommands::Seal { input, output, password, keep } => {
            let in_path = std::path::Path::new(input);
            let out_path = std::path::Path::new(output);

            println!("{}", "🔒 [Ferris Vault] Maxfiylik ombori tayyorlanmoqda...".cyan().bold());
            match vault::seal_vault(in_path, out_path, password.as_deref(), *keep) {
                Ok(count) => {
                    println!("{} {}", "✅ Muvaffaqiyatli muhrlandi:".green().bold(), output.bold());
                    println!("   • Aniqlangan va shifrlangan o'zgaruvchilar soni: {}", count.to_string().yellow());
                    println!("   • Shifrlash algoritmi: {}", "ChaCha20-Poly1305 (256-bit AEAD)".bright_cyan());
                    if !*keep {
                        println!("   • {}", "Asl ochiq fayl diskdan izsiz xavfsiz tozalandi (Zero-Disk Footprint).".green());
                    } else {
                        println!("   • {}", "DIQQAT: Asl fayl diskda saqlab qolindi (--keep).".yellow());
                    }
                    println!("\n💡 Endi ilovani ishga tushirishda:");
                    println!("   {} ferris run -- python3 bot.py", "$".bright_black());
                }
                Err(e) => {
                    eprintln!("{} {}", "❌ Xatolik yuz berdi:".red().bold(), e);
                }
            }
        }
        VaultCommands::Inspect { vault, password, reveal } => {
            let v_path = std::path::Path::new(vault);
            match vault::inspect_vault(v_path, password.as_deref(), *reveal) {
                Ok(items) => {
                    println!("{} {} (Jami: {})", "🔐 Vault tarkibi:".cyan().bold(), vault.bold(), items.len().to_string().yellow());
                    println!("{}", "────────────────────────────────────────────────────────────".bright_black());
                    for (k, v) in items {
                        if *reveal {
                            println!("  {} = {}", k.bright_cyan().bold(), v.yellow());
                        } else {
                            println!("  {} = {}", k.bright_cyan().bold(), v.bright_black());
                        }
                    }
                    println!("{}", "────────────────────────────────────────────────────────────".bright_black());
                    if !*reveal {
                        println!("ℹ️  Qiymatlarni ko'rish uchun: {} qo'shing.", "--reveal".yellow());
                    }
                }
                Err(e) => {
                    eprintln!("{} {}", "❌ Xatolik yuz berdi:".red().bold(), e);
                }
            }
        }
        VaultCommands::Unseal { vault, output, password } => {
            let v_path = std::path::Path::new(vault);
            let out_path = std::path::Path::new(output);

            println!("{}", "⚠️  [Ferris Vault] Faylni qayta tiklash (Unseal)...".yellow().bold());
            match vault::unseal_vault(v_path, out_path, password.as_deref()) {
                Ok(count) => {
                    println!("{} {}", "✅ Ochiq fayl tiklandi:".green().bold(), output.bold());
                    println!("   • Tiklangan o'zgaruvchilar: {}", count.to_string().yellow());
                    println!("   • Ruxsatlar: {}", "0600 (Faqat joriy foydalanuvchi)".bright_cyan());
                    println!("   • {}", "DIQQAT: Foydalangach uni yana 'ferris vault seal' qilishni unutmang!".bright_red());
                }
                Err(e) => {
                    eprintln!("{} {}", "❌ Xatolik yuz berdi:".red().bold(), e);
                }
            }
        }
    }
    Ok(())
}

fn handle_run(
    vault_path_str: &str,
    password: Option<&str>,
    enable_jail: bool,
    allow_cwd_write: bool,
    isolated_net: bool,
    allow_write_paths: &[String],
    socket: Option<&str>,
    cmd_args: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if cmd_args.is_empty() {
        eprintln!("{}", "❌ Ishga tushirish uchun buyruq ko'rsatilmadi (masalan: ferris run -- python3 bot.py)".red());
        return Ok(());
    }

    println!("{}", "🛡️  [Ferris Enclave] Universal xavfsiz qobiq ishga tushirilmoqda...".cyan().bold());

    // 1. Vault'ni xotirada ochish
    let mut env_vars = std::collections::HashMap::new();
    let v_path = std::path::Path::new(vault_path_str);
    if v_path.exists() {
        match vault::load_vault_to_env(v_path, password) {
            Ok(map) => {
                let count = map.len();
                env_vars = map;
                println!("{} {} {}", "   • Sirlar ombori:".green(), count.to_string().yellow().bold(), "ta maxfiy kalit operativ xotiraga (RAM) yuklandi.".green());
            }
            Err(e) => {
                eprintln!("{} {}", "⚠️  Vaultni ochishda xatolik:".yellow().bold(), e);
            }
        }
    } else {
        println!("   • Sirlar ombori: {} topilmadi (standart muhit ishlatiladi).", vault_path_str.bright_black());
    }

    // 2. Soket muhitini ineksiya qilish
    let mut socket_parent_dir: Option<std::path::PathBuf> = None;
    if let Some(sock_path) = socket {
        env_vars.insert("FERRIS_SOCKET".to_string(), sock_path.to_string());
        env_vars.insert("PORT".to_string(), sock_path.to_string());
        println!("   • Tarmoq kanali: {} (Unix Domain Socket faol)", sock_path.bright_cyan());

        let sock_p = std::path::Path::new(sock_path);
        if let Some(parent) = sock_p.parent() {
            if !parent.as_os_str().is_empty() {
                socket_parent_dir = Some(parent.to_path_buf());
            }
        }
    }

    if !allow_cwd_write {
        // Python kod papkasiga __pycache__ yozishga urinib xatolik bermasligi uchun
        env_vars.insert("PYTHONDONTWRITEBYTECODE".to_string(), "1".to_string());
    }

    // 3. Yadroviy qamal (Landlock LSM, Memory Hardening & Network Isolation)
    if enable_jail {
        let mut jail_config = jail::JailConfig::default();
        jail_config.allow_cwd_write = allow_cwd_write;
        jail_config.isolate_network = isolated_net;
        for p in allow_write_paths {
            jail_config.extra_write_paths.push(std::path::PathBuf::from(p));
        }
        if let Some(parent) = socket_parent_dir {
            jail_config.extra_write_paths.push(parent);
        }

        if let Err(e) = jail::apply_sandbox(&jail_config) {
            eprintln!("{} {}", "⚠️  Qamalni qo'llashda xatolik:".yellow(), e);
        }
    }

    let program = &cmd_args[0];
    let args = &cmd_args[1..];

    println!("{} {} {}\n", "🚀 [Boshlandi]:".green().bold(), program.bright_yellow(), args.join(" "));

    let mut command = std::process::Command::new(program);
    command.args(args);
    command.envs(&env_vars);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = command.exec();
        eprintln!("{} {}", "❌ Dasturni ishga tushirib bo'lmadi:".red().bold(), err);
        std::process::exit(1);
    }

    #[cfg(not(unix))]
    {
        let status = command.status()
            .map_err(|e| format!("Dasturni ishga tushirib bo'lmadi ('{}'): {}", program, e))?;

        if !status.success() {
            std::process::exit(status.code().unwrap_or(1));
        }

        Ok(())
    }
}

async fn handle_doc(chapter: Option<&str>, web: bool) -> Result<(), Box<dyn std::error::Error>> {
    let base_url = "https://doc.ferris.uz";

    if web {
        let target_url = match chapter {
            Some("1") | Some("install") | Some("start") => format!("{}/ch01-00-getting-started.html", base_url),
            Some("4") | Some("ownership") | Some("egalik") => format!("{}/ch04-00-understanding-ownership.html", base_url),
            Some("glossary") | Some("lugat") | Some("terms") => format!("{}/appendix-03-glossary.html", base_url),
            _ => base_url.to_string(),
        };
        println!("{} {}", "🌐 Brauzerda ochilmoqda:".cyan().bold(), target_url.yellow());
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(&target_url).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(&target_url).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd").args(["/c", "start", &target_url]).spawn();
        return Ok(());
    }

    match chapter {
        None => {
            println!("\n{}", "🦀 Rust Dasturlash Tili (The Rust Programming Language)".bright_red().bold());
            println!("{}", "🇺🇿 Rasmiy O'zbekcha Ochiq Manbali Tarjima — doc.ferris.uz\n".bright_cyan());

            println!("{}", "📚 MUNDARIJA:".bright_yellow().bold());
            println!("  {} Kirish (Rust kimlar uchun va nima uchun?)", "0.".dimmed());
            println!("  {} 1-Bob: Boshlash va O'rnatish (rustup, cargo, Salom Dunyo!)", "1.".green().bold());
            println!("  {} 2-Bob: Taxmin Qilish O'yini (Guessing Game Tutorial)", "2.".white());
            println!("  {} 3-Bob: Dasturlashning Umumiy Tushunchalari (mut, tiplar, funksiyalar)", "3.".white());
            println!("  {} 4-Bob: Egalik Tizimini Tushunish (Ownership, Borrowing, Slices)", "4.".green().bold());
            println!("  {} 5-Bob: Strukturalar (struct) va Metodlar (impl)", "5.".white());
            println!("  {} 6-Bob: Sanamlar va Andaza Moslash (Enums & Pattern Matching)", "6.".white());
            println!("  {} 10-Bob: Generics, Traits va Lifetimes", "10.".white());
            println!("  {} 16-Bob: Qo'rqmas Paralellik (Fearless Concurrency)", "16.".white());
            println!("  {} 20-Bob: Ko'p Oqimli Veb Server Qurish", "20.".white());
            println!("  {} Ilova C: O'zbekcha-Inglizcha Rust Atamalar Lug'ati", "glossary.".cyan().bold());

            println!("\n{}", "💡 BUYRUQLAR:".bright_yellow().bold());
            println!("  {} {}", "ferris doc 1".cyan(), "-> 1-bob: O'rnatish va Cargo qo'llanmasini o'qish".dimmed());
            println!("  {} {}", "ferris doc 4".cyan(), "-> 4-bob: Egalik Tizimi (Ownership) darsini o'qish".dimmed());
            println!("  {} {}", "ferris doc glossary".cyan(), "-> Standartlashtirilgan terminlar lug'atini ko'rish".dimmed());
            println!("  {} {}", "ferris doc --web".cyan(), "-> Veb brauzerda doc.ferris.uz ni ochish".dimmed());
            println!();
        }
        Some("1") | Some("install") | Some("start") => {
            println!("\n{}", "================================================================================".dimmed());
            println!("{}", "📖 1-BOB: ISHNI BOSHLASH VA O'RNATISH (Getting Started)".bright_green().bold());
            println!("{}\n", "================================================================================".dimmed());

            println!("{}", "1. RUST NI O'RNATISH (Linux & macOS):".bright_yellow().bold());
            println!("  Terminalda quyidagi rasmiy buyruqni kiriting:");
            println!("  {}", "$ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh".cyan().bold());
            println!("  O'rnatilgach, terminalni yangilang: {}", "source $HOME/.cargo/env".dimmed());

            println!("\n{}", "2. O'RNATISHNI TEKSHIRISH:".bright_yellow().bold());
            println!("  {}", "$ rustc --version".cyan());
            println!("  {}", "$ cargo --version".cyan());

            println!("\n{}", "3. BIRINCHI DASTUR (main.rs):".bright_yellow().bold());
            println!("  {}", "fn main() {".cyan());
            println!("  {}", "    println!(\"Salom, Dunyo!\");".cyan());
            println!("  {}", "}".cyan());
            println!("  Kompilyatsiya: {}", "rustc main.rs && ./main".green());

            println!("\n{}", "4. CARGO BILAN LOYIHA YARATISH:".bright_yellow().bold());
            println!("  Yangi loyiha:  {}", "cargo new salom_cargo".cyan());
            println!("  Kompilyatsiya: {}", "cargo build".cyan());
            println!("  Ishga tushirish: {}", "cargo run".cyan().bold());
            println!("  Tezkor tekshirish: {}", "cargo check".cyan());
            println!("  Ishlab chiqarish (Release): {}", "cargo build --release".yellow().bold());
            println!("\n{} https://doc.ferris.uz/ch01-00-getting-started.html\n", "🌐 To'liq o'qish:".dimmed());
        }
        Some("4") | Some("ownership") | Some("egalik") => {
            println!("\n{}", "================================================================================".dimmed());
            println!("{}", "🧠 4-BOB: EGALIK TIZIMINI TUSHUNISH (Understanding Ownership)".bright_red().bold());
            println!("{}\n", "================================================================================".dimmed());

            println!("{}", "Rust ning eng qudratli xususiyati — Garbage Collectorsiz (GC) xotira xavfsizligi!".white());

            println!("\n{}", "🏆 EGALIKNING 3 TA OLTIN QOIDASI:".bright_yellow().bold());
            println!("  {} Rust da har bir qiymatning o'z Egasi (Owner) bo'ladi.", "1.".green().bold());
            println!("  {} Bir vaqtning o'zida faqat BITTA Ega bo'lishi mumkin.", "2.".green().bold());
            println!("  {} Ega o'z ko'rinish sohasidan (scope) chiqib ketganda, xotira avtomatik DROP qilinadi.", "3.".green().bold());

            println!("\n{}", "⚡ QARZ OLISH VA HAVOLALAR (References & Borrowing):".bright_yellow().bold());
            println!("  Egalik huquqini bermasdan qiymatdan foydalanish uchun havola (&) ishlatiladi:");
            println!("  {}", "fn hisobla(matn: &String) -> usize { matn.len() }".cyan());
            println!("  * O'zgarmas havola: {}", "&T (cheksiz ko'p o'qish mumkin)".green());
            println!("  * O'zgaruvchan havola: {}", "&mut T (bir vaqtda faqat 1 ta yozish mumkin — Data Race oldi olinadi!)".yellow().bold());

            println!("\n{} https://doc.ferris.uz/ch04-00-understanding-ownership.html\n", "🌐 To'liq darslik:".dimmed());
        }
        Some("glossary") | Some("lugat") | Some("terms") => {
            println!("\n{}", "================================================================================".dimmed());
            println!("{}", "📖 O'ZBEKCHA-INGLIZCHA RUST ATAMALAR LUG'ATI (Glossary)".bright_cyan().bold());
            println!("{}\n", "================================================================================".dimmed());

            let terms = [
                ("Ownership", "Egalik tizimi / Egalik huquqi", "Xotira xavfsizligining markaziy resurs qoidalari"),
                ("Borrowing", "Qarz olish", "O'zgaruvchiga havola (&) orqali vaqtincha murojaat"),
                ("Reference", "Havola", "Boshqa o'zgaruvchi xotira manziliga xavfsiz ko'rsatkich"),
                ("Lifetime", "Yashash muddati", "Havolaning yaroqlilik chegarasi ('a)"),
                ("Trait", "Xislat (Trait)", "Tip xatti-harakatini ifodalovchi interfeys"),
                ("Pattern Matching", "Andaza moslash", "match yordamida tiplarni tahlil qilish"),
                ("Slice", "Kesma", "To'plamning ma'lum qismiga havola (&[T], &str)"),
                ("Smart Pointer", "Aqlli ko'rsatkich", "Qo'shimcha boshqaruvga ega tiplar (Box, Rc, Arc)"),
                ("Zero-cost Abstraction", "Nol-xarajatli abstraksiya", "Tezlikka ta'sir qilmaydigan yuqori darajadagi sintaksis"),
                ("Concurrency", "Paralellik / Ko'p oqimlilik", "Ko'p oqimli xavfsiz dasturlash"),
                ("Data Race", "Ma'lumotlar poygasi", "Oqimlar to'qnashuvi (Rust da 100% oldi olingan)"),
            ];

            for (eng, uz, desc) in terms {
                println!("  {:<24} -> {:<32} {}", eng.bright_yellow(), uz.bright_green(), desc.dimmed());
            }
            println!("\n{} https://doc.ferris.uz/appendix-03-glossary.html\n", "🌐 To'liq lug'at:".dimmed());
        }
        Some(other) => {
            println!("{} \"{}\" bobi bo'yicha to'liq qo'llanma doc.ferris.uz da mavjud.", "ℹ️".cyan(), other);
            println!("{} ferris doc --web yoki https://doc.ferris.uz sahifasiga kiring.\n", "👉".yellow());
        }
    }

    Ok(())
}

async fn handle_unblock(
    client: &reqwest::Client,
    api: &str,
    config: &CliConfig,
    username: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let token = config.token.as_deref().unwrap_or("");
    if token.is_empty() {
        println!("{}", "⚠️ Blokdan chiqarish uchun tizimga kirish shart: ferris login <TOKEN>".yellow());
        return Ok(());
    }
    let clean_user = username.trim_start_matches('@');
    println!("{} @{} blokdan chiqarilmoqda...", "✅".cyan(), clean_user);
    let res = client
        .post(format!("{}/api/v1/users/{}/unblock", api, clean_user))
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await;
    match res {
        Ok(resp) if resp.status().is_success() => {
            println!("{} @{} blokdan chiqarildi! Endi yana yozisha olasiz.", "✅".green().bold(), clean_user);
        }
        Ok(resp) => {
            let err: serde_json::Value = resp.json().await.unwrap_or_default();
            println!("{} {}", "❌ Xatolik:".red(), err["error"].as_str().unwrap_or("Blokdan chiqarib bo'lmadi"));
        }
        Err(e) => println!("{} Tarmoq xatosi: {}", "❌".red(), e),
    }
    Ok(())
}

fn print_privacy_policy() {
    println!("\n{}", "═".repeat(78).cyan());
    println!("  🦀 Ferris.uz — Maxfiylik Siyosati (Privacy Policy)");
    println!("  Rasmiy havola: {}", "https://ferris.uz/privacy".yellow().bold());
    println!("  Versiya: 1.0  •  Kuchga kirish sanasi: 2026-yil 22-oktabr");
    println!("{}\n", "═".repeat(78).cyan());

    println!("{}\n", "1. ASOSIY PRINSIPLAR:".yellow().bold());
    println!("   • Minimalizm va Maxfiylik: Faqat tizim ishlashi uchun zarur bo'lgan minimal ma'lumotlar saqlanadi.");
    println!("   • Ochiq kodli ekotizim: Ferris.uz O'zbekiston Rustaceanlar jamoat loyihasidir.\n");

    println!("{}\n", "2. QANDAY MA'LUMOTLAR YIG'ILADI:".yellow().bold());
    println!("   • Autentifikatsiya: Parollar saqlanmaydi. Faqat GitHub OAuth yoki Telegram SSO orqali ochiq profil ma'lumotlari (username, ism, avatar).");
    println!("   • Kontent: Siz chop etgan mavzular, izohlar va ommaviy chat xabarlari.");
    println!("   • Shaxsiy Xabarlar (DM): Ikki foydalanuvchi o'rtasidagi yozishmalar PostgreSQL 16 da saqlanadi va real-time WebSocket orqali yetkaziladi.");
    println!("   • Texnik sessiya: Yagona ferris_session cookie va tokeni.\n");

    println!("{}\n", "3. COOKIE VA KUZATUVCHILAR (TRACKING):".yellow().bold());
    println!("   • Hech qanday Google Analytics, Yandex Metrika yoki uchinchi tomon reklama trekerlari ISHLATILMAYDI.");
    println!("   • Faqat yagona xavfsiz avtorizatsiya cookie-fayli ishlatiladi (Domain=.ferris.uz; Secure; SameSite=Lax).\n");

    println!("{}\n", "4. XAVFSIZLIK VA BAZA:".yellow().bold());
    println!("   • Barcha ma'lumotlar PostgreSQL 16 relyatsion bazasida xavfsiz saqlanadi.");
    println!("   • Aloqa Cloudflare Universal SSL (TLS 1.3) bilan to'liq shifrlangan.");
    println!("   • 24 soatlik TTL qoidasi: a'zosi 0 bo'lgan jamiyatlar 24 soatda avtomatik o'chiriladi.\n");

    println!("{}\n", "5. MA'LUMOTLARNI UCHINCHI SHAXSLARGA BERMASLIK KAFOLATI:".yellow().bold());
    println!("   • Shaxsiy ma'lumotlar hech qachon sotilmaydi, ijaraga berilmaydi va tijoriy maqsadlarda tarqatilmaydi.\n");

    println!("{}\n", "6. FOYDALANUVCHI HUQUQLARI VA KONTENT NAZORATI (CRUD):".yellow().bold());
    println!("   • Tahrirlash (Rectification): Postlarni Web va CLI orqali (ferris post edit <ID>) tahrirlash huquqi.");
    println!("   • O'chirish (Erasure / Unutilish): Post, izoh va DM xabarlarni darhol o'chirish huquqi.");
    println!("   • Sessiyani yakunlash: 'ferris logout' buyrug'i orqali sessiyani bekor qilish.\n");

    println!("{}\n", "7. FOYDALANUVCHILARNI BLOKLASH VA XAVFSIZLIK:".yellow().bold());
    println!("   • Spam yoki bezorilikdan himoyalanish uchun har qanday foydalanuvchini bloklash mumkin.");
    println!("   • Bloklangan foydalanuvchi sizga DM yubora olmaydi (Serverda 403 Forbidden).");
    println!("   • CLI buyruqlari: ferris block add @user, ferris block remove @user, ferris block list.\n");

    println!("{}\n", "8. MA'LUMOTLARNI DOIMIY O'CHIRISH (HARD DELETE KAFOLATI):".yellow().bold());
    println!("   • Ma'lumotlar yashirib qo'yilmaydi (soft delete yo'q), balki PostgreSQL diskidan to'liq o'chiriladi.");
    println!("   • Post o'chirilganda uning barcha izohlari CASCADE qoidasi bilan birga o'chiriladi.\n");

    println!("{}\n", "9. MA'MURIYAT VA ALOQA:".yellow().bold());
    println!("   • Asoschi: @iskurama (https://profile.ferris.uz/@iskurama)");
    println!("   • GitHub: https://github.com/iskurama/ferris-uz");
    println!("   • Telegram: https://t.me/iskurama\n");
}

fn print_terms_of_service() {
    println!("\n{}", "═".repeat(78).cyan());
    println!("  📜 Ferris.uz — Foydalanish Qoidalari (Terms of Service)");
    println!("  Rasmiy havola: {}", "https://ferris.uz/terms".yellow().bold());
    println!("  Versiya: 1.0  •  Kuchga kirish sanasi: 2026-yil 22-oktabr");
    println!("{}\n", "═".repeat(78).cyan());

    println!("{}\n", "1. QOIDALARNING QABUL QILINISHI:".yellow().bold());
    println!("   • Ferris.uz platformasi (veb-sayt, API, WebSocket va ferris CLI)dan foydalanish");
    println!("     orqali siz ushbu shartlarga to'liq va so'zsiz rozilik bildirasiz.");
    println!("   • Agar biror bandga rozi bo'lmasangiz, platformadan foydalanishni to'xtating.\n");

    println!("{}\n", "2. TAQIQLANGAN HARAKATLAR (ACCEPTABLE USE):".yellow().bold());
    println!("   • O'zbekiston Respublikasining amaldagi qonunchiligiga zid axborotlar tarqatish;");
    println!("   • Zararli kodlar (malware, ransomware, eksploitlar) va fishing havolalari ulashish;");
    println!("   • Spam, botlar orqali flood uyushtirish yoki server resurslariga DDoS hujumlari;");
    println!("   • Boshqa foydalanuvchilarni haqorat qilish, tuhmat yoki noqonuniy ta'qib qilish.\n");

    println!("{}\n", "3. MA'MURIYAT HUQUQLARI VA MODERATSIYA:".yellow().bold());
    println!("   • Qoidalarni buzgan post, izoh yoki xabarlarni ogohlantirishsiz o'chirish (hard delete);");
    println!("   • Qoidabuzar akkauntlarni vaqtinchalik yoki doimiy ravishda bloklash / ban qilish;");
    println!("   • Jamiyat a'zosi 0 ga tushganida 24 soatlik TTL bilan guruhni tozalash.\n");

    println!("{}\n", "4. KAFOLATLARNING RAD ETILISHI VA JAVOBGARLIKNI CHEKLASH (\"AS IS\"):".yellow().bold());
    println!("   • Platforma va barcha xizmatlar kafolatsiz, 'boricha' ('AS IS') taqdim etiladi.");
    println!("   • Ma'muriyat xizmatning uzluksizligiga yoki ma'lumotlar yo'qolmasligiga kafolat bermaydi.");
    println!("   • A'zolar chop etgan kodlar sababli yuzaga kelgan zararlar uchun javobgarlik olinmaydi.\n");

    println!("{}\n", "5. MUALLIFLIK HUQUQLARI VA OCHIQ KOD:".yellow().bold());
    println!("   • O'zingiz yozgan muhokama va kodlarning muallifi bo'lib qolasiz.");
    println!("   • Ferris.uz loyihasining o'zi ochiq kodli tamoyillarga asoslangan.\n");

    println!("{}\n", "6. SHARTLARNING O'ZGARISHI VA ALOQA:".yellow().bold());
    println!("   • Shartlar bir tomonlama yangilanishi mumkin (https://ferris.uz/terms).");
    println!("   • Asoschi & Moderator: @iskurama (https://profile.ferris.uz/@iskurama)");
    println!("   • GitHub: https://github.com/iskurama/ferris-uz");
    println!("   • Telegram: https://t.me/iskurama\n");
}

fn print_license() {
    println!("\n{}", "═".repeat(78).cyan());
    println!("  ⚖️ Ferris CLI — Xalqaro Litsenziya Shartnomasi (Open Source)");
    println!("  Litsenziya turi: {}", "MIT OR Apache-2.0 (Dual License)".yellow().bold());
    println!("  Muallif: {}", "Copyright (c) 2026 iskurama <https://ferris.uz>".green().bold());
    println!("  Kutubxona: {}", "https://crates.io/crates/ferris-cli".cyan());
    println!("{}\n", "═".repeat(78).cyan());

    println!("{}\n", "1. HUQUQLAR VA ERKINLIKLAR (MIT & Apache 2.0):".yellow().bold());
    println!("   • Ushbu dastur bepul va ochiq kodli dasturiy ta'minot hisoblanadi.");
    println!("   • Har qanday jismoniy yoki yuridik shaxs dasturdan shaxsiy, ta'lim yoki");
    println!("     tijoriy maqsadlarda erkin foydalanishi, nusxalashi, o'zgartirishi mumkin.");
    println!("   • Mualliflik huquqi (Copyright) va litsenziya matni saqlanishi shart.\n");

    println!("{}\n", "2. PATENT HIMOYASI (Apache 2.0 Kafolati):".yellow().bold());
    println!("   • Dastur patent himoyasini ta'minlaydi. Hech qaysi tomon ushbu kodga");
    println!("     nisbatan patent da'volari yoki cheklovlar qo'ya olmaydi.\n");

    println!("{}\n", "3. KAFOLATSIZLIK (DISCLAIMER OF LIABILITY):".yellow().bold());
    println!("   • Dastur \"AS IS\" (boricha) taqdim etiladi. Muallif dasturdan foydalanish");
    println!("     natijasida yuzaga kelishi mumkin bo'lgan har qanday texnik xatolik,");
    println!("     ma'lumotlar yo'qolishi yoki tizim uzilishlari uchun javobgar emas.\n");

    println!("{}\n", "4. ASL MANBA VA REPOZITORIY:".yellow().bold());
    println!("   • Rasmiy kodlar: https://github.com/iskurama/ferris-uz");
    println!("   • Muallif: @iskurama (https://profile.ferris.uz/@iskurama)\n");
}

async fn render_posts_list(client: &reqwest::Client, api: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "🦀 Ferris.uz — Yangi Muhokamalar & Postlar (topics.ferris.uz):".yellow().bold());
    println!("{}", "═".repeat(78).dimmed());

    let res = client.get(format!("{}/api/v1/posts", api)).send().await;
    match res {
        Ok(resp) if resp.status().is_success() => {
            let posts: Vec<PostListItem> = resp.json().await?;
            save_posts_cache(&posts);

            if posts.is_empty() {
                println!("  Hozircha hech qanday post mavjud emas.");
                return Ok(());
            }

            for (idx, p) in posts.iter().enumerate() {
                let author = p.author_name.as_deref().unwrap_or("Rustacean");
                let comments = p.comments_count.unwrap_or(0);
                let views = p.views_count;
                let date = format_date(&p.created_at);
                let preview = if p.content.len() > 110 {
                    format!("{}...", &p.content[..110])
                } else {
                    p.content.clone()
                };

                println!(
                    "  {} {} {}",
                    format!("[{}]", idx + 1).green().bold(),
                    p.title.white().bold(),
                    format!("({})", &p.id[..8]).dimmed()
                );
                println!(
                    "      Muallif: {}  •  💬 {} ta fikr  •  👁 {} ta ko'rildi  •  {}",
                    format!("@{}", author).cyan(),
                    comments.to_string().yellow(),
                    views.to_string().cyan(),
                    date.dimmed()
                );
                println!("      {}\n", preview.replace('\n', " ").dimmed());
            }

            println!("{}", "─".repeat(78).dimmed());
            println!("💡 Postni to'liq o'qish uchun:  {}", "ferris post read <raqami yoki ID>".green().bold());
            println!("💡 Masalan: {}", "ferris post read 1".cyan());
        }
        _ => {
            println!("{}", "❌ Postlarni serverdan yuklab bo'lmadi.".red());
        }
    }
    Ok(())
}

async fn render_post_detail(client: &reqwest::Client, api: &str, raw_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let id = resolve_post_id(raw_id);
    let res = client.get(format!("{}/api/v1/posts/{}", api, id)).send().await;

    match res {
        Ok(resp) if resp.status().is_success() => {
            let detail: PostDetailResponse = resp.json().await?;
            let author = detail.author_name.as_deref().unwrap_or("Rustacean");
            let date = format_date(&detail.created_at);

            println!("\n{}", "═".repeat(78).cyan());
            println!("  {}", detail.title.white().bold());
            println!(
                "  Muallif: {}  •  Sana: {}  •  👁 {} ta ko'rildi  •  ID: {}",
                format!("@{}", author).cyan(),
                date.dimmed(),
                detail.views_count.to_string().green(),
                &detail.id[..8].dimmed()
            );
            println!("{}\n", "═".repeat(78).cyan());

            // Post content
            for line in detail.content.lines() {
                println!("  {}", line);
            }
            println!();

            // Comments
            println!("{}", "─".repeat(78).dimmed());
            println!(
                "  💬 Fikrlar va Javoblar ({})",
                detail.comments.len().to_string().yellow().bold()
            );
            println!("{}", "─".repeat(78).dimmed());

            if detail.comments.is_empty() {
                println!("    Hozircha hech kim fikr bildirmagan. Birinchi bo'ling!");
            } else {
                for c in &detail.comments {
                    let c_date = format_date(&c.created_at);
                    println!("  ┌ {} ({})", format!("@{}", c.author_name).cyan().bold(), c_date.dimmed());
                    for cline in c.content.lines() {
                        println!("  │ {}", cline);
                    }
                    println!("  └{}", "─".repeat(70).dimmed());
                }
            }

            println!();
            println!("💡 Fikr bildirish:  {}", format!("ferris post comment {} \"fikringiz...\"", raw_id).green().bold());
        }
        _ => {
            println!("{} '{}' identifikatorli post topilmadi.", "❌".red(), raw_id);
            println!("Barcha postlarni ko'rish uchun: ferris feed");
        }
    }
    Ok(())
}

fn handle_ws_incoming_text(text: &str, current_username: Option<&str>) {
    // 1. Try parsing structured WsEventIncoming
    if let Ok(event) = serde_json::from_str::<WsEventIncoming>(text) {
        match event {
            WsEventIncoming::DirectMessage { id, sender_username, receiver_username, text, timestamp, msg_hash } => {
                let time_str = format_ts(timestamp);
                let is_for_me = current_username.map(|u| u == receiver_username).unwrap_or(false);
                let is_from_me = current_username.map(|u| u == sender_username).unwrap_or(false);

                let sec_badge = if let Some(ref r_hash) = msg_hash {
                    let expected = compute_dm_hash(&id, &sender_username, &receiver_username, &text, timestamp);
                    if r_hash == &expected {
                        let short = if r_hash.len() >= 8 { &r_hash[..8] } else { r_hash.as_str() };
                        format!("[🔒 PQC-DM: {}]", short).green().bold()
                    } else {
                        "[⚠️ SOXTA / HASH NOTO'G'RI]".red().bold()
                    }
                } else {
                    "[🔒 PQC-DM]".dimmed()
                };

                let badge = "[📩 SHAXSIY XABAR]".magenta().bold();
                let sender_disp = if is_from_me { format!("@{}", sender_username).green().bold() } else { format!("@{}", sender_username).cyan().bold() };
                let rec_disp = if is_for_me { format!("@{}", receiver_username).green().bold() } else { format!("@{}", receiver_username).cyan().bold() };

                println!("{} {} {} ({} ➔ {}): {}", time_str.dimmed(), badge, sec_badge, sender_disp, rec_disp, text.white().bold());
            }
            WsEventIncoming::PostComment { id: _, post_id: _, post_title, author_name, content, timestamp } => {
                let time_str = format_ts(timestamp);
                let badge = "[💬 MAVZUGA JAVOB]".yellow().bold();
                let author_disp = format!("@{}", author_name).cyan().bold();
                println!("{} {} \"{}\" mavzusiga {} javob yozdi:", time_str.dimmed(), badge, post_title.cyan(), author_disp);
                println!("      \"{}\"", content.white());
            }
            WsEventIncoming::NewPost { id: _, topic_slug, group_slug, author_name, title, content, timestamp } => {
                let time_str = format_ts(timestamp);
                let badge = "[📝 YANGI MAVZU]".green().bold();
                let author_disp = format!("@{}", author_name).cyan().bold();
                let dest = topic_slug.or(group_slug).unwrap_or_else(|| "ferris".to_string());
                println!("{} {} [#{}] {}: {}", time_str.dimmed(), badge, dest.yellow(), author_disp, title.white().bold());
                let preview: String = content.chars().take(80).collect();
                println!("      \"{}\"", preview.dimmed());
            }
            WsEventIncoming::GroupActivity { group_slug, username, action, members_count, timestamp } => {
                let time_str = format_ts(timestamp);
                let badge = "[👥 JAMIYAT]".blue().bold();
                let user_disp = format!("@{}", username).cyan().bold();
                println!("{} {} f/{}: {} {} (jami: {} a'zo)", time_str.dimmed(), badge, group_slug.yellow(), user_disp, action, members_count.to_string().green());
            }
            WsEventIncoming::CommunityChat(msg) => {
                let ts = msg.timestamp.unwrap_or(0);
                let time_str = format_ts(ts);
                let author = &msg.user.username;
                let colored_author = if current_username.map(|u| u == author).unwrap_or(false) {
                    format!("@{}", author).green().bold()
                } else if author == "iskurama" {
                    format!("@{}", author).red().bold()
                } else {
                    format!("@{}", author).cyan().bold()
                };

                let sec_badge = if let Some(ref r_hash) = msg.msg_hash {
                    let msg_id = msg.id.as_deref().unwrap_or("");
                    let room_str = msg.room.as_deref().unwrap_or("general");
                    let expected = compute_chat_hash(msg_id, room_str, author, &msg.text, ts, msg.prev_hash.as_deref());
                    if r_hash == &expected {
                        let short = if r_hash.len() >= 8 { &r_hash[..8] } else { r_hash.as_str() };
                        format!("[🔒 PQC: {}]", short).green().bold()
                    } else {
                        "[⚠️ SOXTA / HASH NOTO'G'RI]".red().bold()
                    }
                } else {
                    "[🔒 PQC-LEGACY]".dimmed()
                };

                let badge = "[⚡ CHAT]".cyan().bold();
                println!("{} {} {} {}: {}", time_str.dimmed(), badge, sec_badge, colored_author, msg.text);
            }
        }
        let _ = std::io::stdout().flush();
        return;
    }

    // 2. Fallback to raw chat JSON
    match serde_json::from_str::<WsEventIncoming>(text) {
        Ok(_) => {}
        Err(_e) => {
            // Agar deserializatsiya xatosi bo'lsa, xatolikni tekshirish
        }
    }

    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
        let author = val["user"]["username"].as_str().unwrap_or("anon");
        let text_msg = val["text"].as_str().unwrap_or("");
        let ts = val["timestamp"].as_u64().unwrap_or(0);
        let time_str = format_ts(ts);
        let colored_author = if current_username.map(|u| u == author).unwrap_or(false) {
            format!("@{}", author).green().bold()
        } else if author == "iskurama" {
            format!("@{}", author).red().bold()
        } else {
            format!("@{}", author).cyan().bold()
        };
        println!("{} {} {}: {}", time_str.dimmed(), "[⚡ CHAT]".cyan().bold(), colored_author, text_msg);
    }
    let _ = std::io::stdout().flush();
}

async fn handle_dm_command(
    action: &DmCommands,
    client: &reqwest::Client,
    api: &str,
    config: &CliConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let token = match &config.token {
        Some(t) if !t.is_empty() => t,
        _ => {
            println!("{}", "⚠️ Shaxsiy xabarlardan foydalanish uchun avval tizimga kiring:".yellow());
            println!("  ferris login <TOKEN>");
            return Ok(());
        }
    };

    match action {
        DmCommands::List => {
            println!("{}", "✉️ Shaxsiy Suhbatlar (Direct Messages):".magenta().bold());
            let res = client
                .get(format!("{}/api/v1/dm", api))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    let val: serde_json::Value = resp.json().await?;
                    let convos = val["conversations"].as_array();
                    if let Some(list) = convos {
                        if list.is_empty() {
                            println!("  Hozircha hech qanday shaxsiy xabar yo'q.");
                            println!("  Yangi xabar yozish uchun: ferris dm chat @username \"xabar...\"");
                        } else {
                            for item in list {
                                let uname = item["username"].as_str().unwrap_or("");
                                let last_msg = item["last_message"].as_str().unwrap_or("");
                                let ts = item["timestamp"].as_i64().unwrap_or(0);
                                let time_str = if ts > 0 {
                                    chrono::DateTime::from_timestamp(ts, 0)
                                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                                        .unwrap_or_default()
                                } else {
                                    "".to_string()
                                };
                                println!(
                                    "  • {:<16} ({}) : \"{}\"",
                                    format!("@{}", uname).cyan().bold(),
                                    time_str.dimmed(),
                                    last_msg.dimmed()
                                );
                            }
                            println!("\n💡 Xabar yozish yoki o'qish uchun: {}", "ferris dm chat @username".cyan().bold());
                        }
                    }
                }
                _ => println!("{}", "❌ Shaxsiy xabarlar ro'yxatini yuklab bo'lmadi.".red()),
            }
        }
        DmCommands::Chat { username, text } => {
            let clean_user = username.trim_start_matches('@');
            if let Some(msg_text) = text {
                send_dm(client, api, token, clean_user, msg_text).await?;
            } else {
                println!("{}", format!("💬 @{} bilan shaxsiy yozishmalar:", clean_user).cyan().bold());
                let res = client
                    .get(format!("{}/api/v1/dm/{}", api, clean_user))
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;

                match res {
                    Ok(resp) if resp.status().is_success() => {
                        let val: serde_json::Value = resp.json().await?;
                        if val["is_blocked_by_me"].as_bool().unwrap_or(false) {
                            println!("{}", "🚫 Siz bu foydalanuvchini bloklagansiz. (Chiqarish: ferris unblock @username)".yellow().bold());
                        }
                        if val["is_blocked_by_them"].as_bool().unwrap_or(false) {
                            println!("{}", "🚫 Ushbu foydalanuvchi sizni bloklagan. Xabar yuborib bo'lmaydi.".red().bold());
                        }
                        let msgs = val["messages"].as_array();
                        if let Some(list) = msgs {
                            if list.is_empty() {
                                println!("  Yozishmalar mavjud emas. Birinchi xabarni yuboring:");
                                println!("  ferris dm chat @{} \"salom!\"", clean_user);
                            } else {
                                for m in list {
                                    let id = m["id"].as_str().unwrap_or("");
                                    let s = m["sender"].as_str().unwrap_or("");
                                    let t = m["text"].as_str().unwrap_or("");
                                    let ts = m["timestamp"].as_i64().unwrap_or(0);
                                    let time_str = if ts > 0 {
                                        chrono::DateTime::from_timestamp(ts, 0)
                                            .map(|d| d.format("[%H:%M:%S]").to_string())
                                            .unwrap_or_default()
                                    } else {
                                        "".to_string()
                                    };
                                    let sender_fmt = if s == config.username.as_deref().unwrap_or("") {
                                        format!("@{}", s).green().bold()
                                    } else {
                                        format!("@{}", s).cyan().bold()
                                    };
                                    let id_short = if id.len() >= 8 { &id[..8] } else { id };
                                    let r_hash = m["msg_hash"].as_str();
                                    let sec_badge = if let Some(h) = r_hash {
                                        let receiver = if s == config.username.as_deref().unwrap_or("") { clean_user } else { config.username.as_deref().unwrap_or("") };
                                        let expected = compute_dm_hash(id, s, receiver, t, ts as u64);
                                        if h == expected {
                                            let short = if h.len() >= 8 { &h[..8] } else { h };
                                            format!("[🔒 PQC: {}]", short).green().bold()
                                        } else {
                                            "[⚠️ SOXTA]".red().bold()
                                        }
                                    } else {
                                        "[🔒 PQC-LEGACY]".dimmed()
                                    };
                                    println!("  {} {} {} {}: {}", time_str.dimmed(), format!("[{}]", id_short).dimmed(), sec_badge, sender_fmt, t);
                                }
                            }
                        }
                        println!("\n💡 Yangi xabar: {}", format!("ferris dm chat @{} \"xabar...\"", clean_user).green().bold());
                        println!("🗑️ Xabarni o'chirish: {}", "ferris dm delete <ID>".dimmed());
                    }
                    _ => println!("{}", "❌ Chat tarixini yuklab bo'lmadi.".red()),
                }
            }
        }
        DmCommands::Send { username, text } => {
            let clean_user = username.trim_start_matches('@');
            send_dm(client, api, token, clean_user, text).await?;
        }
        DmCommands::Delete { id } => {
            let res = client
                .delete(format!("{}/api/v1/dm/messages/{}", api, id))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await;

            match res {
                Ok(resp) if resp.status().is_success() => {
                    println!("{} Shaxsiy xabar muvaffaqiyatli o'chirildi (ID: {})", "✅".green().bold(), id);
                }
                Ok(resp) => {
                    let err: serde_json::Value = resp.json().await.unwrap_or_default();
                    println!("{} {}", "❌ Xatolik:".red(), err["error"].as_str().unwrap_or("Xabarni o'chirib bo'lmadi"));
                }
                Err(e) => println!("{} Tarmoq xatosi: {}", "❌".red(), e),
            }
        }
    }

    Ok(())
}

async fn send_dm(
    client: &reqwest::Client,
    api: &str,
    token: &str,
    target_user: &str,
    text: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{} @{} ga shaxsiy xabar yuborilmoqda...", "📤".cyan(), target_user.bold());
    let res = client
        .post(format!("{}/api/v1/dm/{}", api, target_user))
        .header("Authorization", format!("Bearer {}", token))
        .json(&serde_json::json!({ "text": text }))
        .send()
        .await;

    match res {
        Ok(resp) if resp.status().is_success() => {
            println!("{} Xabar @{} ga yetkazildi va PostgreSQL ga saqlandi!", "✅".green().bold(), target_user);
        }
        Ok(resp) if resp.status() == reqwest::StatusCode::FORBIDDEN => {
            println!("{} Xabar yetkazilmadi: foydalanuvchi sizni bloklagan yoki siz uni bloklagansiz.", "🚫".red().bold());
        }
        Ok(resp) => {
            let status_str = resp.status().to_string();
            let err: serde_json::Value = resp.json().await.unwrap_or_default();
            println!("{} Xatolik: {}", "❌".red(), err["error"].as_str().unwrap_or(&status_str));
        }
        Err(e) => {
            println!("{} Aloqa xatosi: {}", "❌".red(), e);
        }
    }
    Ok(())
}

async fn run_live_chat(room: &str, config: &CliConfig) -> Result<(), Box<dyn std::error::Error>> {
    let ws_url = "wss://ferris.uz/ws/community";

    println!("{}", "🦀 Ferris.uz Real-Time WebSocket Hubga ulanmoqda...".cyan());
    let (ws_stream, _) = match connect_async(ws_url).await {
        Ok(res) => res,
        Err(e) => {
            println!("{} WebSocket ga ulanib bo'lmadi: {}", "❌".red(), e);
            println!("Iltimos, internet aloqasini tekshiring.");
            return Ok(());
        }
    };

    println!("\n{}", "═".repeat(78).cyan());
    println!("  ⚡ Ferris.uz Jonli Hamjamiyat Chati & Yangiliklar (WebSocket)");
    println!("  Xona: #{}  •  Hub: {}", room.yellow().bold(), "wss://ferris.uz/ws/community".dimmed());
    if let Some(uname) = &config.username {
        println!("  Foydalanuvchi: {} {}", format!("@{}", uname).green().bold(), "(xabar yozishingiz mumkin)".dimmed());
    } else {
        println!("  Holat: {} (yozish uchun: ferris login <TOKEN>)", "Faqat kuzatish".yellow());
    }
    println!("  Chiqish uchun: {} yoki {}", "/exit".red().bold(), "Ctrl+C".red().bold());
    println!("{}\n", "═".repeat(78).cyan());

    let (mut write_half, mut read_half) = ws_stream.split();
    let token = config.token.clone();
    let room_str = room.to_string();

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            Some(msg_result) = read_half.next() => {
                match msg_result {
                    Ok(WsMessage::Text(text)) => {
                        handle_ws_incoming_text(&text, config.username.as_deref());
                    }
                    Ok(WsMessage::Close(_)) => {
                        println!("\n{}", "⚠️ Server aloqani yakunladi.".yellow());
                        break;
                    }
                    Err(e) => {
                        println!("\n{} Xatolik: {}", "⚠️ WebSocket uzildi:".yellow(), e);
                        break;
                    }
                    _ => {}
                }
            }

            line_res = stdin_lines.next_line() => {
                match line_res {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if trimmed == "/exit" || trimmed == "/quit" {
                            println!("{}", "Jonli chatdan chiqildi. Salomat bo'ling! 🦀".cyan());
                            break;
                        }

                        if let Some(t) = &token {
                            let payload = serde_json::json!({
                                "token": t,
                                "room": room_str,
                                "text": trimmed
                            });
                            if let Ok(json_str) = serde_json::to_string(&payload) {
                                if let Err(e) = write_half.send(WsMessage::Text(json_str.into())).await {
                                    println!("{} Xabar yuborilmadi: {}", "❌".red(), e);
                                    break;
                                }
                            }
                        } else {
                            println!("{}", "⚠️ Xabar yuborish uchun tizimga kirish shart: ferris login <TOKEN>".yellow());
                        }
                    }
                    Ok(None) => {
                        // Stdin reached EOF (masalan fon rejimida yoki yo'naltirilgan oqimda)
                        // WebSocket tinglashni to'xtatmaslik uchun kutamiz
                        futures_util::future::pending::<()>().await;
                    }
                    _ => break,
                }
            }
        }
    }

    Ok(())
}

fn open_browser(url: &str) {
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/c", "start", url]).spawn();
    }
}

pub async fn perform_device_login(
    client: &reqwest::Client,
    api_url: &str,
    client_id: &str,
) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    println!("{}", "🦀 Ferris ID (OAuth 2.0 Device Flow - RFC 8628)".cyan().bold());
    println!("{}", "Terminal autentifikatsiya so'rovi yuborilmoqda...".dimmed());

    let code_res = client
        .post(format!("{}/api/v1/oauth/device/code", api_url))
        .json(&serde_json::json!({ "client_id": client_id }))
        .send()
        .await?;

    if !code_res.status().is_success() {
        let err_body: serde_json::Value = code_res.json().await.unwrap_or_default();
        let msg = err_body["error_description"].as_str().unwrap_or("Serverdan device code olib bo'lmadi");
        println!("{} {}", "❌ Xatolik:".red(), msg);
        return Ok(None);
    }

    let code_data: serde_json::Value = code_res.json().await?;
    let device_code = code_data["device_code"].as_str().unwrap_or("").to_string();
    let user_code = code_data["user_code"].as_str().unwrap_or("").to_string();
    let verification_uri = code_data["verification_uri"].as_str().unwrap_or("https://ferris.uz/device");
    let verification_uri_complete = code_data["verification_uri_complete"]
        .as_str()
        .unwrap_or(verification_uri);
    let interval = code_data["interval"].as_u64().unwrap_or(5);

    println!("\n{}", "──────────────────────────────────────────────────────".bright_black());
    println!("  {} {}", "! Bir martalik tasdiqlash kodi:".yellow().bold(), user_code.green().bold());
    println!("  {} {}", "🌐 Brauzer tasdiqlash manzili:".cyan().bold(), verification_uri_complete.underline());
    println!("{}", "──────────────────────────────────────────────────────".bright_black());
    println!("{}", "Brauzer avtomatik ochilmoqda. Agar ochilmasa, yuqoridagi havolani oching.\n".dimmed());

    let url_clone = verification_uri_complete.to_string();
    tokio::task::spawn_blocking(move || {
        open_browser(&url_clone);
    });

    print!("{}", "⏳ Brauzerda tasdiqlash kutilmoqda ".dimmed());
    let _ = std::io::stdout().flush();

    let poll_interval = Duration::from_secs(interval.max(3));
    let start_time = std::time::Instant::now();
    let max_duration = Duration::from_secs(600); // 10 daqiqa

    while start_time.elapsed() < max_duration {
        tokio::time::sleep(poll_interval).await;

        let token_res = client
            .post(format!("{}/api/v1/oauth/device/token", api_url))
            .json(&serde_json::json!({
                "client_id": client_id,
                "device_code": device_code
            }))
            .send()
            .await;

        match token_res {
            Ok(resp) if resp.status().is_success() => {
                let token_data: serde_json::Value = resp.json().await?;
                let access_token = token_data["access_token"].as_str().unwrap_or("").to_string();
                let username = token_data["user"]["username"].as_str().unwrap_or("user").to_string();
                return Ok(Some((access_token, username)));
            }
            Ok(resp) => {
                let err_data: serde_json::Value = resp.json().await.unwrap_or_default();
                let err_code = err_data["error"].as_str().unwrap_or("");
                if err_code == "authorization_pending" {
                    print!(".");
                    let _ = std::io::stdout().flush();
                    continue;
                } else if err_code == "access_denied" {
                    println!("\n{} Foydalanuvchi ilovaga ruxsat bermadi.", "❌".red().bold());
                    return Ok(None);
                } else if err_code == "expired_token" {
                    println!("\n{} Tasdiqlash kodi muddati o'tdi. Qaytadan urinib ko'ring.", "❌".red().bold());
                    return Ok(None);
                } else {
                    println!("\n{} Xatolik: {}", "❌".red(), err_code);
                    return Ok(None);
                }
            }
            Err(_) => {
                print!("*");
                let _ = std::io::stdout().flush();
                continue;
            }
        }
    }

    println!("\n{} Tasdiqlash vaqti tugadi (Timeout).", "❌".red());
    Ok(None)
}
