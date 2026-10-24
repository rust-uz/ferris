use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Redirect},
    routing::{delete, get, post},
    Router,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::{info, warn};
use uuid::Uuid;

use askama::Template;
pub mod models;
use models::{Comment, Jamiyat, Post, Topic};
use sqlx::{PgPool, Row};

#[derive(Clone, Serialize, Deserialize)]
pub struct PostWithComments {
    pub post: Post,
    pub comments: Vec<Comment>,
}

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub topics: Vec<Topic>,
    pub groups: Vec<Jamiyat>,
}

#[derive(Template)]
#[template(path = "topics.html")]
pub struct TopicsTemplate {
    pub topics: Vec<Topic>,
    pub posts: Vec<PostWithComments>,
}

#[derive(Template)]
#[template(path = "groups.html")]
pub struct GroupsTemplate {
    pub groups: Vec<Jamiyat>,
}

#[derive(Template)]
#[template(path = "group_detail.html")]
pub struct GroupDetailTemplate {
    pub group: Jamiyat,
    pub members_count: usize,
    pub posts: Vec<PostWithComments>,
    pub members: Vec<String>,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {}

#[derive(Template)]
#[template(path = "device.html")]
pub struct DeviceTemplate {
    pub current_user: Option<UserProfile>,
    pub initial_code: String,
    pub client_name: Option<String>,
    pub client_id: Option<String>,
    pub allowed_scopes: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UserIdentity {
    pub provider: String,
    pub provider_id: String,
    pub provider_username: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ActiveTokenItem {
    pub id: String,
    pub client_id: String,
    pub client_name: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Template)]
#[template(path = "profile.html")]
pub struct ProfileTemplate {
    pub user: UserProfile,
    pub joined_groups: Vec<Jamiyat>,
    pub user_posts: Vec<Post>,
    pub identities: Vec<UserIdentity>,
    pub is_own_profile: bool,
    pub has_github: bool,
    pub has_telegram: bool,
    pub github_username: Option<String>,
    pub telegram_username: Option<String>,
    pub active_tokens: Vec<ActiveTokenItem>,
    pub can_unlink: bool,
}

#[derive(Template)]
#[template(path = "privacy.html")]
pub struct PrivacyTemplate {}

#[derive(Template)]
#[template(path = "terms.html")]
pub struct TermsTemplate {}

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct UserProfile {
    pub id: String,
    pub provider: String, // "telegram" | "github" | "local"
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub created_at: u64,
    #[serde(default)]
    pub expires_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChatMessage {
    pub id: String,
    pub room: String,
    pub user: UserProfile,
    pub text: String,
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub msg_hash: Option<String>,
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

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "event", content = "data")]
pub enum WsEvent {
    #[serde(rename = "community_chat")]
    CommunityChat(ChatMessage),

    #[serde(rename = "direct_message")]
    DirectMessage {
        id: String,
        sender_username: String,
        receiver_username: String,
        text: String,
        timestamp: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        msg_hash: Option<String>,
    },

    #[serde(rename = "post_comment")]
    PostComment {
        id: String,
        post_id: Uuid,
        post_title: String,
        author_name: String,
        content: String,
        timestamp: u64,
    },

    #[serde(rename = "new_post")]
    NewPost {
        id: Uuid,
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

#[derive(Clone)]
pub struct AppState {
    pub db: Option<PgPool>,
    pub sessions: Arc<RwLock<HashMap<String, UserProfile>>>,
    pub chat_tx: broadcast::Sender<WsEvent>,
    pub chat_history: Arc<RwLock<Vec<ChatMessage>>>,
    pub online_count: Arc<RwLock<usize>>,
    pub http_client: reqwest::Client,
    pub topics: Arc<RwLock<Vec<Topic>>>,
    pub groups: Arc<RwLock<Vec<Jamiyat>>>,
    pub group_members: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    pub posts: Arc<RwLock<Vec<Post>>>,
    pub comments: Arc<RwLock<Vec<Comment>>>,
    pub post_views: Arc<RwLock<HashMap<Uuid, HashSet<String>>>>,
    pub bot_token: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub rate_limiter: Arc<RwLock<HashMap<String, Vec<i64>>>>,
    pub user_ws: Arc<RwLock<HashMap<String, Vec<tokio::sync::mpsc::UnboundedSender<WsEvent>>>>>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct PersistentData {
    pub sessions: HashMap<String, UserProfile>,
    pub groups: Vec<Jamiyat>,
    pub group_members: HashMap<String, HashSet<String>>,
    pub posts: Vec<Post>,
    pub comments: Vec<Comment>,
    #[serde(default)]
    pub chat_history: Vec<ChatMessage>,
    #[serde(default)]
    pub post_views: HashMap<Uuid, HashSet<String>>,
}

const DB_PATH: &str = "ferris_db.json";

fn load_db() -> Option<PersistentData> {
    if let Ok(content) = std::fs::read_to_string(DB_PATH) {
        if let Ok(data) = serde_json::from_str::<PersistentData>(&content) {
            info!(
                "📂 [DB] Holat {} faylidan muvaffaqiyatli tiklandi: {} ta sessiya, {} ta jamiyat, {} ta post, {} ta chat xabari",
                DB_PATH,
                data.sessions.len(),
                data.groups.len(),
                data.posts.len(),
                data.chat_history.len()
            );
            return Some(data);
        }
    }
    None
}

fn save_db(state: &AppState) {
    // When PostgreSQL is active, it is our permanent durable storage.
    // Avoid blocking Tokio worker threads with heavy synchronous JSON serialization and disk writes.
    if state.db.is_some() {
        return;
    }

    let data = PersistentData {
        sessions: state.sessions.read().unwrap().clone(),
        groups: state.groups.read().unwrap().clone(),
        group_members: state.group_members.read().unwrap().clone(),
        posts: state.posts.read().unwrap().clone(),
        comments: state.comments.read().unwrap().clone(),
        chat_history: state.chat_history.read().unwrap().clone(),
        post_views: state.post_views.read().unwrap().clone(),
    };

    tokio::task::spawn_blocking(move || {
        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let temp_path = format!("{}.tmp", DB_PATH);
            if std::fs::write(&temp_path, json).is_ok() {
                let _ = std::fs::rename(temp_path, DB_PATH);
            }
        }
    });
}

async fn load_from_postgres(pool: &PgPool) -> Option<(
    Vec<Jamiyat>,
    HashMap<String, HashSet<String>>,
    Vec<Post>,
    Vec<Comment>,
    HashMap<String, UserProfile>,
    Vec<ChatMessage>,
    HashMap<Uuid, HashSet<String>>,
)> {
    // 1. Users & Sessions
    let mut sessions = HashMap::new();
    let session_rows = sqlx::query(
        r#"
        SELECT s.token, u.id, u.provider, u.username, u.display_name, u.avatar_url, s.created_at
        FROM sessions s
        JOIN users u ON s.user_id = u.id
        "#
    )
    .fetch_all(pool)
    .await
    .ok()?;

    for r in session_rows {
        let token: String = r.try_get("token").unwrap_or_default();
        let id: String = r.try_get("id").unwrap_or_default();
        let provider: String = r.try_get("provider").unwrap_or_default();
        let username: String = r.try_get("username").unwrap_or_default();
        let display_name: String = r.try_get("display_name").unwrap_or_default();
        let avatar_url: Option<String> = r.try_get("avatar_url").ok();
        let created_at: i64 = r.try_get("created_at").unwrap_or(0);
        let created_dt = chrono::DateTime::from_timestamp(created_at, 0).unwrap_or_else(Utc::now);
        let expires_at = created_dt + chrono::Duration::days(30);

        // Prune already expired sessions on startup
        if Utc::now() > expires_at {
            continue;
        }

        sessions.insert(
            token,
            UserProfile {
                id,
                provider,
                username,
                display_name,
                avatar_url,
                created_at: created_at as u64,
                expires_at: Some(expires_at),
            },
        );
    }

    // 2. Groups
    let mut groups = Vec::new();
    let group_rows = sqlx::query(
        "SELECT id, slug, name, description, avatar_url, owner_id, empty_since, is_bot_enabled, created_at, updated_at FROM groups"
    )
    .fetch_all(pool)
    .await
    .ok()?;

    for r in group_rows {
        let id: Uuid = r.try_get("id").unwrap_or_default();
        let slug: String = r.try_get("slug").unwrap_or_default();
        let name: String = r.try_get("name").unwrap_or_default();
        let description: Option<String> = r.try_get("description").ok();
        let avatar_url: Option<String> = r.try_get("avatar_url").ok();
        let owner_id: Option<Uuid> = r.try_get("owner_id").ok();
        let empty_since: Option<chrono::DateTime<Utc>> = r.try_get("empty_since").ok();
        let is_bot_enabled: bool = r.try_get("is_bot_enabled").unwrap_or(false);
        let created_at: chrono::DateTime<Utc> = r.try_get("created_at").unwrap_or_else(|_| Utc::now());
        let updated_at: chrono::DateTime<Utc> = r.try_get("updated_at").unwrap_or_else(|_| Utc::now());

        groups.push(Jamiyat {
            id,
            slug,
            name,
            description,
            avatar_url: avatar_url.unwrap_or_else(|| "/logo.jpg".to_string()),
            owner_id,
            members_count: 0,
            empty_since,
            is_bot_enabled,
            created_at,
            updated_at,
        });
    }

    // 3. Group Members
    let mut group_members: HashMap<String, HashSet<String>> = HashMap::new();
    let member_rows = sqlx::query("SELECT group_slug, username FROM group_members")
        .fetch_all(pool)
        .await
        .ok()?;

    for r in member_rows {
        let gslug: String = r.try_get("group_slug").unwrap_or_default();
        let uname: String = r.try_get("username").unwrap_or_default();
        group_members.entry(gslug).or_default().insert(uname);
    }

    for g in groups.iter_mut() {
        if let Some(m) = group_members.get(&g.slug) {
            g.members_count = m.len() as i32;
        }
    }

    // 4. Posts
    let mut posts = Vec::new();
    let post_rows = sqlx::query(
        "SELECT id, topic_slug, author_name, author_avatar, title, content, views_count, created_at, updated_at FROM posts ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await
    .ok()?;

    for r in post_rows {
        let id: Uuid = r.try_get("id").unwrap_or_default();
        let topic_slug: Option<String> = r.try_get("topic_slug").ok();
        let author_name: String = r.try_get("author_name").unwrap_or_default();
        let author_avatar: Option<String> = r.try_get("author_avatar").ok();
        let title: String = r.try_get("title").unwrap_or_default();
        let content: String = r.try_get("content").unwrap_or_default();
        let views_count: i32 = r.try_get("views_count").unwrap_or(0);
        let created_at: chrono::DateTime<Utc> = r.try_get("created_at").unwrap_or_else(|_| Utc::now());
        let updated_at: chrono::DateTime<Utc> = r.try_get("updated_at").unwrap_or_else(|_| Utc::now());

        posts.push(Post {
            id,
            topic_slug,
            topic_id: None,
            jamiyat_id: None,
            author_id: Uuid::new_v4(),
            author_name: Some(author_name),
            author_avatar,
            title,
            content,
            views_count,
            created_at,
            updated_at,
        });
    }

    // 5. Comments
    let mut comments = Vec::new();
    let comment_rows = sqlx::query(
        "SELECT id, post_id, author_name, author_avatar, content, created_at FROM comments ORDER BY created_at ASC"
    )
    .fetch_all(pool)
    .await
    .ok()?;

    for r in comment_rows {
        let id: Uuid = r.try_get("id").unwrap_or_default();
        let post_id: Uuid = r.try_get("post_id").unwrap_or_default();
        let author_name: String = r.try_get("author_name").unwrap_or_default();
        let author_avatar: Option<String> = r.try_get("author_avatar").ok();
        let content: String = r.try_get("content").unwrap_or_default();
        let created_at: chrono::DateTime<Utc> = r.try_get("created_at").unwrap_or_else(|_| Utc::now());

        comments.push(Comment {
            id,
            post_id,
            author_name,
            author_avatar: author_avatar.unwrap_or_else(|| "/logo.jpg".to_string()),
            content,
            created_at,
        });
    }

    // 6. Chat messages with PQC BLAKE3 cryptographic chain
    let mut chat_history = Vec::new();
    let chat_rows = sqlx::query(
        "SELECT id, room, author_username, author_avatar, text, timestamp, msg_hash, prev_hash FROM chat_messages ORDER BY timestamp ASC LIMIT 100"
    )
    .fetch_all(pool)
    .await
    .ok()?;

    for r in chat_rows {
        let id: Uuid = r.try_get("id").unwrap_or_default();
        let room: String = r.try_get("room").unwrap_or_default();
        let uname: String = r.try_get("author_username").unwrap_or_default();
        let avatar: Option<String> = r.try_get("author_avatar").ok();
        let text: String = r.try_get("text").unwrap_or_default();
        let timestamp: i64 = r.try_get("timestamp").unwrap_or(0);
        let prev_hash: Option<String> = r.try_get("prev_hash").ok();
        let msg_hash: Option<String> = r.try_get("msg_hash").ok().or_else(|| {
            Some(compute_chat_hash(&id.to_string(), &room, &uname, &text, timestamp as u64, prev_hash.as_deref()))
        });

        chat_history.push(ChatMessage {
            id: id.to_string(),
            room,
            user: UserProfile {
                id: format!("usr:{}", uname),
                provider: "community".to_string(),
                username: uname.clone(),
                display_name: uname,
                avatar_url: avatar,
                created_at: timestamp as u64,
                expires_at: None,
            },
            text,
            timestamp: timestamp as u64,
            prev_hash,
            msg_hash,
        });
    }

    // 7. Post Views (deduplicated views per user or IP)
    let mut post_views: HashMap<Uuid, HashSet<String>> = HashMap::new();
    if let Ok(view_rows) = sqlx::query("SELECT post_id, viewer_key FROM post_views")
        .fetch_all(pool)
        .await
    {
        for r in view_rows {
            let pid: Uuid = r.try_get("post_id").unwrap_or_default();
            let vk: String = r.try_get("viewer_key").unwrap_or_default();
            post_views.entry(pid).or_default().insert(vk);
        }
    }

    info!(
        "🐘 [PostgreSQL 16] Ma'lumotlar bazasidan muvaffaqiyatli yuklandi: {} ta sessiya, {} ta jamiyat, {} ta post, {} ta ko'rishlar rekordi, {} ta chat xabari",
        sessions.len(),
        groups.len(),
        posts.len(),
        post_views.len(),
        chat_history.len()
    );

    Some((groups, group_members, posts, comments, sessions, chat_history, post_views))
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let (chat_tx, _) = broadcast::channel(100);

    // Dastlabki mavzular (Topics)
    let initial_topics = vec![
        Topic {
            id: Uuid::new_v4(),
            slug: "f/rust".to_string(),
            title: "Rust Core & Ekotizim".to_string(),
            description: Some("Rust tili asoslari, borrow checker, ownership va yangi relizlar".to_string()),
            icon: "🦀".to_string(),
            created_at: Utc::now(),
        },
        Topic {
            id: Uuid::new_v4(),
            slug: "f/async".to_string(),
            title: "Asinxron Dasturlash".to_string(),
            description: Some("Tokio, Axum, Tower va yuqori RPS mikroservislar".to_string()),
            icon: "⚡".to_string(),
            created_at: Utc::now(),
        },
        Topic {
            id: Uuid::new_v4(),
            slug: "f/systems".to_string(),
            title: "Tizimli Dasturlash & Linux".to_string(),
            description: Some("Linux yadrosi, drayverlar, Embedded va xotira optimallashtirish".to_string()),
            icon: "🐧".to_string(),
            created_at: Utc::now(),
        },
        Topic {
            id: Uuid::new_v4(),
            slug: "f/quantum".to_string(),
            title: "Kvant Hisoblash & AI".to_string(),
            description: Some("Kvant algoritmlari, Qiskit, QCS va kelajak kriptografiyasi".to_string()),
            icon: "🔮".to_string(),
            created_at: Utc::now(),
        },
    ];

    // Dastlabki jamiyatlar (Groups)
    let rust_uz_id = Uuid::new_v4();
    let systems_id = Uuid::new_v4();

    let initial_groups = vec![
        Jamiyat {
            id: rust_uz_id,
            slug: "rust-uz".to_string(),
            name: "O'zbekiston Rustaceanlar Jamiyati".to_string(),
            description: Some("Rust tilini o'rganuvchilar va professional dasturchilar bosh jamoasi.".to_string()),
            avatar_url: "/logo.jpg".to_string(),
            owner_id: None,
            members_count: 1,
            empty_since: None,
            is_bot_enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        Jamiyat {
            id: systems_id,
            slug: "systems-tashkent".to_string(),
            name: "Toshkent Systems Meetup".to_string(),
            description: Some("Low-level, OS muhandisligi va server arxitekturasi qiziquvchilari.".to_string()),
            avatar_url: "/logo.jpg".to_string(),
            owner_id: None,
            members_count: 1,
            empty_since: None,
            is_bot_enabled: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
    ];

    let mut initial_members: HashMap<String, HashSet<String>> = HashMap::new();
    let mut rust_uz_set = HashSet::new();
    rust_uz_set.insert("iskurama".to_string());
    rust_uz_set.insert("ibrohimkoha".to_string());
    initial_members.insert("rust-uz".to_string(), rust_uz_set);

    let mut systems_set = HashSet::new();
    systems_set.insert("iskurama".to_string());
    initial_members.insert("systems-tashkent".to_string(), systems_set);

    let sample_post_id = Uuid::new_v4();
    let group_post_id = Uuid::new_v4();
    let initial_posts = vec![
        Post {
            id: sample_post_id,
            topic_slug: Some("f/rust".to_string()),
            topic_id: None,
            jamiyat_id: None,
            author_id: Uuid::new_v4(),
            author_name: Some("Rustacean".to_string()),
            author_avatar: Some("/logo.jpg".to_string()),
            title: "Rust 1.85 relizida asinxron traitlar va yangi qulayliklar".to_string(),
            content: "Rust tilida `async fn in traits` to'liq barqarorlashdi. Tizim arxitekturasida buni qanday qo'llash mumkin?".to_string(),
            views_count: 5,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        Post {
            id: group_post_id,
            topic_slug: Some("f/rust".to_string()),
            topic_id: None,
            jamiyat_id: Some(rust_uz_id),
            author_id: Uuid::new_v4(),
            author_name: Some("iskurama".to_string()),
            author_avatar: Some("/logo.jpg".to_string()),
            title: "O'zbekiston Rustaceanlar Jamiyatiga xush kelibsiz!".to_string(),
            content: "Ushbu jamiyat O'zbekistondagi Rust tilini o'rganayotgan va unda ishlayotgan barcha dasturchilar uchun rasmiy platformadir. Loyihangizni baham ko'ring!".to_string(),
            views_count: 12,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    ];

    let initial_comments = vec![
        Comment {
            id: Uuid::new_v4(),
            post_id: sample_post_id,
            author_name: "Rustacean".to_string(),
            author_avatar: "/logo.jpg".to_string(),
            content: "Tokio bilan birgalikda microservice arxitekturasida Dynamic Dispatch o'rniga static dispatch bilan RPS ni 30% oshirish mumkin!".to_string(),
            created_at: Utc::now(),
        },
        Comment {
            id: Uuid::new_v4(),
            post_id: group_post_id,
            author_name: "Rustacean".to_string(),
            author_avatar: "/logo.jpg".to_string(),
            content: "Ajoyib tashabbus! Rust ekotizimini birgalikda rivojlantiramiz 🦀".to_string(),
            created_at: Utc::now(),
        }
    ];

    // PostgreSQL 16 ulanish
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://ferris_user:ferris_secure_2026_pw@127.0.0.1:5432/ferris_db".to_string());

    let db_pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(50)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(300))
        .connect(&database_url)
        .await
    {
        Ok(pool) => {
            info!("🐘 [PostgreSQL 16] Bazasiga muvaffaqiyatli ulandi: {}", database_url);
            Some(pool)
        }
        Err(e) => {
            warn!("⚠️ [PostgreSQL 16] Ulanib bo'lmadi ({}), JSON fayl rejimi qo'llaniladi", e);
            None
        }
    };

    let pg_data = if let Some(ref pool) = db_pool {
        load_from_postgres(pool).await
    } else {
        None
    };

    let (mut saved_groups, saved_members, saved_posts, saved_comments, saved_sessions, saved_chat, saved_views) =
        if let Some((groups, members, posts, comments, sessions, chat, views)) = pg_data {
            (groups, members, posts, comments, sessions, chat, views)
        } else if let Some(db) = load_db() {
            (db.groups, db.group_members, db.posts, db.comments, db.sessions, db.chat_history, db.post_views)
        } else {
            (initial_groups, initial_members, initial_posts, initial_comments, HashMap::new(), Vec::new(), HashMap::new())
        };

    // Har bir jamiyatning a'zolar sonini haqiqiy a'zolar ro'yxatiga sinxronlash
    for g in saved_groups.iter_mut() {
        if let Some(members) = saved_members.get(&g.slug) {
            g.members_count = members.len() as i32;
        }
    }

    let bot_token = std::env::var("TELEGRAM_BOT_TOKEN")
        .unwrap_or_else(|_| "8928247696:AAF_82P6DUzUQwAwqB5hJ_5RZW-PvkF0xig".to_string());
    let github_client_id = std::env::var("GITHUB_CLIENT_ID")
        .unwrap_or_else(|_| "Ov23lilJD2yyUUmIL6aZ".to_string());
    let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET")
        .unwrap_or_else(|_| "1ab8683885a6cc0428c846cd7d56559e61473224".to_string());

    let state = AppState {
        db: db_pool,
        sessions: Arc::new(RwLock::new(saved_sessions)),
        chat_tx,
        chat_history: Arc::new(RwLock::new(saved_chat)),
        online_count: Arc::new(RwLock::new(1)),
        http_client: reqwest::Client::builder()
            .user_agent("FerrisUz-Auth/2.0")
            .build()
            .unwrap(),
        topics: Arc::new(RwLock::new(initial_topics)),
        groups: Arc::new(RwLock::new(saved_groups)),
        group_members: Arc::new(RwLock::new(saved_members)),
        posts: Arc::new(RwLock::new(saved_posts)),
        comments: Arc::new(RwLock::new(saved_comments)),
        post_views: Arc::new(RwLock::new(saved_views)),
        bot_token,
        github_client_id,
        github_client_secret,
        rate_limiter: Arc::new(RwLock::new(HashMap::new())),
        user_ws: Arc::new(RwLock::new(HashMap::new())),
    };

    // Saqlab qo'yish
    save_db(&state);

    // =========================================================================
    // BACKGROUND WORKER: 24 SOATLIK BO'SH JAMIYATLARNI AVTO-TOZALASH
    // =========================================================================
    let worker_groups = Arc::clone(&state.groups);
    let worker_db = state.db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60)); // Har daqiqada tekshiruv
        loop {
            interval.tick().await;
            let now = Utc::now();
            let mut removed_slugs = Vec::new();
            {
                let mut groups = worker_groups.write().unwrap();
                groups.retain(|g| {
                    if g.members_count == 0 {
                        if let Some(empty_time) = g.empty_since {
                            let hours_empty = (now - empty_time).num_hours();
                            if hours_empty >= 24 {
                                removed_slugs.push(g.slug.clone());
                                return false; // o'chirish
                            }
                        }
                    }
                    true
                });
            }

            if !removed_slugs.is_empty() {
                info!("🧹 [Auto-Cleanup] {} ta bo'sh jamiyat tozalandi: {:?}", removed_slugs.len(), removed_slugs);
                if let Some(ref pool) = worker_db {
                    for slug in removed_slugs {
                        let _ = sqlx::query("DELETE FROM groups WHERE slug = $1 AND members_count = 0")
                            .bind(&slug)
                            .execute(pool)
                            .await;
                    }
                }
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
            if let Ok(origin_str) = origin.to_str() {
                if origin_str == "https://ferris.uz"
                    || origin_str.ends_with(".ferris.uz")
                    || origin_str == "https://hikaku.uz"
                    || origin_str.ends_with(".hikaku.uz")
                    || origin_str.starts_with("http://localhost:")
                    || origin_str.starts_with("http://127.0.0.1:")
                {
                    return true;
                }
            }
            false
        }))
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        // Sahifalar va Host Router (Alohida Subdomen Sahifalari)
        .route("/", get(root_host_router))
        .route("/topics", get(topics_page_handler))
        .route("/groups", get(groups_page_handler))
        .route("/f/:slug", get(group_detail_page_handler))
        .route("/groups/:slug", get(group_detail_page_handler))
        .route("/login", get(login_page_handler))
        .route("/device", get(device_page_handler))
        .route("/profile", get(my_profile_page_handler))
        .route("/profile/:username", get(user_profile_page_handler))
        .route("/u/:username", get(user_profile_page_handler))
        .route("/@:username", get(user_profile_page_handler))
        .route("/privacy", get(privacy_page_handler))
        .route("/privacy-policy", get(privacy_page_handler))
        .route("/terms", get(terms_page_handler))
        .route("/terms-of-service", get(terms_page_handler))
        .route("/install.sh", get(install_sh_handler))
        .route("/install.ps1", get(install_ps1_handler))
        .route("/robots.txt", get(robots_handler))
        .route("/sitemap.xml", get(dynamic_sitemap_handler))
        .route("/favicon.ico", get(favicon_handler))
        // Tizim & Holat
        .route("/api/v1/ping", get(ping_handler))
        .route("/api/v1/stats", get(stats_handler))
        .route("/api/v1/context", get(context_handler))
        .route("/api/v1/search", get(search_handler))
        // Mavzular (Topics)
        .route("/api/v1/topics", get(topics_list_handler))
        .route("/api/v1/topics/:slug", get(topic_detail_handler))
        // Jamiyatlar (Groups)
        .route("/api/v1/groups", get(groups_list_handler).post(group_create_handler))
        .route("/api/v1/groups/:slug", get(group_detail_handler))
        .route("/api/v1/groups/:slug/join", post(group_join_handler))
        .route("/api/v1/my-groups", get(my_groups_handler))
        // Postlar (Posts) & Kommentlar (Javoblar)
        .route("/api/v1/posts", get(posts_list_handler).post(post_create_handler))
        .route("/api/v1/posts/:id", get(post_detail_handler).put(post_update_handler).delete(post_delete_handler))
        .route("/api/v1/posts/:id/view", post(post_record_view_handler))
        .route("/api/v1/posts/:id/comments", get(get_comments_handler).post(add_comment_handler))
        .route("/api/v1/comments/:id", delete(comment_delete_handler))
        // Foydalanuvchi Profili va Suveren Username
        .route("/api/v1/profile/:username", get(profile_handler))
        .route("/api/v1/users/check-username", get(check_username_handler))
        .route("/api/v1/profile/username", post(update_username_handler))
        // Real Rust Playground API
        .route("/api/v1/playground/execute", post(playground_execute_handler))
        // Rustacean Leaderboard & Badges
        .route("/api/v1/leaderboard", get(leaderboard_handler))
        // Autentifikatsiya
        .route("/api/auth/telegram", post(telegram_auth_handler))
        .route("/api/auth/github", get(github_login_redirect))
        .route("/api/auth/github/callback", get(github_callback_handler))
        .route("/api/auth/me", get(auth_me_handler))
        .route("/api/auth/logout", post(auth_logout_handler))
        // Universal SSO (Ferris ID) & Device Flow (RFC 8628)
        .route("/api/v1/oauth/device/code", post(oauth_device_code_handler))
        .route("/api/v1/oauth/device/token", post(oauth_device_token_handler))
        .route("/api/v1/oauth/device/info", get(oauth_device_info_handler))
        .route("/api/v1/oauth/device/approve", post(oauth_device_approve_handler))
        .route("/api/v1/oauth/device/deny", post(oauth_device_deny_handler))
        .route("/api/v1/oauth/verify", get(oauth_verify_handler))
        .route("/api/v1/oauth/tokens", get(oauth_tokens_list_handler))
        .route("/api/v1/oauth/tokens/:id", delete(oauth_token_revoke_handler))
        .route("/api/v1/oauth/tokens/revoke-all", post(oauth_token_revoke_all_handler))
        .route("/api/v1/auth/identities/:provider", delete(auth_identity_unlink_handler))
        // Shaxsiy Xabarlar (Direct Messages) & Xabarni O'chirish
        .route("/api/v1/dm", get(dm_conversations_handler))
        .route("/api/v1/dm/:username", get(dm_messages_handler).post(dm_send_handler))
        .route("/api/v1/dm/messages/:id", delete(dm_message_delete_handler))
        // Foydalanuvchilarni Bloklash / Blokdan chiqarish
        .route("/api/v1/users/:username/block", post(user_block_handler))
        .route("/api/v1/users/:username/unblock", post(user_unblock_handler))
        .route("/api/v1/blocked-users", get(blocked_users_list_handler))
        // Real-time WebSocket
        .route("/ws/community", get(ws_community_handler))
        .fallback_service(ServeDir::new("public"))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 5000));
    info!("🦀 [FERRIS.UZ] Engine 2.0 with Full Subdomain API online on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// ==============================================================================
// 0. SUBDOMENLAR UCHUN ALOHIDA SAHIFALAR ROUTERI (ASKAMA RUST SSR)
// ==============================================================================
async fn root_host_router(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");

    if host.starts_with("topics.") {
        topics_page_handler(State(state)).await.into_response()
    } else if host.starts_with("groups.") {
        groups_page_handler(State(state)).await.into_response()
    } else {
        let topics = state.topics.read().unwrap().clone();
        let groups = state.groups.read().unwrap().clone();
        let template = IndexTemplate { topics, groups };
        match template.render() {
            Ok(html) => axum::response::Html(html).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    }
}

async fn topics_page_handler(State(state): State<AppState>) -> impl IntoResponse {
    let topics = state.topics.read().unwrap().clone();
    let posts = state.posts.read().unwrap().clone();
    let comments = state.comments.read().unwrap().clone();

    let posts_with_comments: Vec<PostWithComments> = posts
        .into_iter()
        .map(|p| {
            let p_comments: Vec<Comment> = comments
                .iter()
                .filter(|c| c.post_id == p.id)
                .cloned()
                .collect();
            PostWithComments {
                post: p,
                comments: p_comments,
            }
        })
        .collect();

    let template = TopicsTemplate {
        topics,
        posts: posts_with_comments,
    };
    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn groups_page_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut groups = state.groups.read().unwrap().clone();
    let members_map = state.group_members.read().unwrap();
    for g in groups.iter_mut() {
        if let Some(m) = members_map.get(&g.slug) {
            g.members_count = m.len() as i32;
        }
    }
    let template = GroupsTemplate { groups };
    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub fn check_rate_limit(
    limiter: &Arc<RwLock<HashMap<String, Vec<i64>>>>,
    key: &str,
    max_requests: usize,
    window_secs: i64,
) -> bool {
    let now = chrono::Utc::now().timestamp();
    let cutoff = now - window_secs;
    let mut map = limiter.write().unwrap();

    // Prevent unbounded memory growth by pruning expired entries periodically
    if map.len() > 1000 {
        map.retain(|_, timestamps| {
            timestamps.retain(|&ts| ts > cutoff);
            !timestamps.is_empty()
        });
    }

    let entries = map.entry(key.to_string()).or_default();
    entries.retain(|&ts| ts > cutoff);
    if entries.len() >= max_requests {
        false
    } else {
        entries.push(now);
        true
    }
}

pub fn get_client_ip(headers: &HeaderMap) -> String {
    // 1. CF-Connecting-IP (from Cloudflare)
    if let Some(cf_ip) = headers.get("cf-connecting-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = cf_ip.trim();
        if trimmed.parse::<std::net::IpAddr>().is_ok() {
            return trimmed.to_string();
        }
    }
    // 2. X-Real-IP (from trusted local Nginx proxy)
    if let Some(real_ip) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let trimmed = real_ip.trim();
        if trimmed.parse::<std::net::IpAddr>().is_ok() {
            return trimmed.to_string();
        }
    }
    // 3. X-Forwarded-For
    if let Some(x_forwarded) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = x_forwarded.split(',').next() {
            let trimmed = first.trim();
            if trimmed.parse::<std::net::IpAddr>().is_ok() {
                return trimmed.to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

pub fn get_authenticated_user(headers: &HeaderMap, state: &AppState) -> Option<UserProfile> {
    let now = Utc::now();
    let is_valid = |user: &UserProfile| -> bool {
        if let Some(exp) = user.expires_at {
            if now > exp {
                return false;
            }
        }
        true
    };

    // 1. Authorization: Bearer <token>
    if let Some(auth_val) = headers.get(axum::http::header::AUTHORIZATION).and_then(|h| h.to_str().ok()) {
        if let Some(token) = auth_val.strip_prefix("Bearer ") {
            let sessions = state.sessions.read().unwrap();
            if let Some(user) = sessions.get(token.trim()) {
                if is_valid(user) {
                    return Some(user.clone());
                }
            }
        }
    }
    // 2. Cookie: ferris_session=<token>
    if let Some(cookie_val) = headers.get(axum::http::header::COOKIE).and_then(|h| h.to_str().ok()) {
        for cookie in cookie_val.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix("ferris_session=") {
                let sessions = state.sessions.read().unwrap();
                if let Some(user) = sessions.get(token.trim()) {
                    if is_valid(user) {
                        return Some(user.clone());
                    }
                }
            }
        }
    }
    None
}

pub fn get_viewer_key(headers: &HeaderMap, state: &AppState) -> String {
    if let Some(user) = get_authenticated_user(headers, state) {
        format!("u:{}", user.username)
    } else {
        format!("ip:{}", get_client_ip(headers))
    }
}

pub fn record_post_view(post_id: Uuid, viewer_key: &str, state: &AppState) -> bool {
    let mut views_map = state.post_views.write().unwrap();
    let set = views_map.entry(post_id).or_insert_with(HashSet::new);
    if set.insert(viewer_key.to_string()) {
        let views_calc = set.len() as i32;
        let mut posts = state.posts.write().unwrap();
        if let Some(post) = posts.iter_mut().find(|p| p.id == post_id) {
            post.views_count = views_calc;
        }
        drop(posts);
        drop(views_map);

        save_db(state);

        if let Some(ref pool) = state.db {
            let pool = pool.clone();
            let vk = viewer_key.to_string();
            tokio::spawn(async move {
                let _ = sqlx::query(
                    "INSERT INTO post_views (post_id, viewer_key) VALUES ($1, $2) ON CONFLICT DO NOTHING"
                )
                .bind(post_id)
                .bind(&vk)
                .execute(&pool)
                .await;

                let _ = sqlx::query(
                    "UPDATE posts SET views_count = (SELECT COUNT(*) FROM post_views WHERE post_id = $1) WHERE id = $1"
                )
                .bind(post_id)
                .execute(&pool)
                .await;
            });
        }
        true
    } else {
        false
    }
}

async fn group_detail_page_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    _headers: HeaderMap,
) -> impl IntoResponse {
    let clean_slug = slug.trim_start_matches("f/").to_string();
    let groups = state.groups.read().unwrap();
    let group = match groups.iter().find(|g| g.slug == clean_slug).cloned() {
        Some(g) => g,
        None => return (StatusCode::NOT_FOUND, "Jamiyat topilmadi").into_response(),
    };

    let members = {
        let members_map = state.group_members.read().unwrap();
        members_map.get(&clean_slug).cloned().unwrap_or_default().into_iter().collect::<Vec<_>>()
    };
    let members_count = members.len();

    let all_posts = state.posts.read().unwrap().clone();
    let all_comments = state.comments.read().unwrap().clone();

    let posts: Vec<PostWithComments> = all_posts
        .into_iter()
        .filter(|p| p.jamiyat_id == Some(group.id))
        .map(|p| {
            let p_comments: Vec<Comment> = all_comments
                .iter()
                .filter(|c| c.post_id == p.id)
                .cloned()
                .collect();
            PostWithComments {
                post: p,
                comments: p_comments,
            }
        })
        .collect();

    let template = GroupDetailTemplate {
        group,
        members_count,
        posts,
        members,
    };

    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn login_page_handler() -> impl IntoResponse {
    let template = LoginTemplate {};
    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn device_page_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let current_user = get_authenticated_user(&headers, &state);
    let initial_code = query.get("code").cloned().unwrap_or_default().trim().to_uppercase();

    let mut client_name = None;
    let mut client_id = None;
    let mut allowed_scopes = Vec::new();

    if !initial_code.is_empty() {
        if let Some(ref pool) = state.db {
            if let Ok(Some(row)) = sqlx::query(
                "SELECT dc.client_id, oc.client_name, oc.allowed_scopes 
                 FROM oauth_device_codes dc 
                 LEFT JOIN oauth_clients oc ON dc.client_id = oc.client_id 
                 WHERE dc.user_code = $1 AND dc.status = 'pending' AND dc.expires_at > NOW()"
            )
            .bind(&initial_code)
            .fetch_optional(pool)
            .await {
                let cid: String = row.try_get("client_id").unwrap_or_default();
                let cname: Option<String> = row.try_get("client_name").ok();
                let scopes: Vec<String> = row.try_get("allowed_scopes").unwrap_or_default();
                client_id = Some(cid);
                client_name = cname;
                allowed_scopes = scopes;
            }
        }
    }

    let template = DeviceTemplate {
        current_user,
        initial_code,
        client_name,
        client_id,
        allowed_scopes,
    };
    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn my_groups_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<String>>, StatusCode> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    let members_map = state.group_members.read().unwrap();
    let mut user_groups = Vec::new();
    for (slug, members) in members_map.iter() {
        if members.contains(&auth_user.username) {
            user_groups.push(slug.clone());
        }
    }
    Ok(Json(user_groups))
}

async fn my_profile_page_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Redirect::to("/login?return_to=https://ferris.uz/profile").into_response(),
    };

    render_profile_view(&auth_user, &state, true).await
}

async fn user_profile_page_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(username): Path<String>,
) -> impl IntoResponse {
    let clean_user = username.trim_start_matches('@').to_string();
    let current_auth = get_authenticated_user(&headers, &state);

    let mut matched_user = None;

    if let Some(ref pool) = state.db {
        if let Ok(Some(row)) = sqlx::query(
            "SELECT id, username, display_name, provider, avatar_url, created_at 
             FROM users WHERE LOWER(username) = LOWER($1)"
        )
        .bind(&clean_user)
        .fetch_optional(pool)
        .await {
            let uid: String = row.try_get("id").unwrap_or_default();
            let uname: String = row.try_get("username").unwrap_or_default();
            let dname: String = row.try_get("display_name").unwrap_or_default();
            let prov: String = row.try_get("provider").unwrap_or_default();
            let aurl: Option<String> = row.try_get("avatar_url").ok();
            let cat: i64 = row.try_get("created_at").unwrap_or(0);
            matched_user = Some(UserProfile {
                id: uid,
                provider: prov,
                username: uname,
                display_name: dname,
                avatar_url: aurl,
                created_at: cat as u64,
                expires_at: None,
            });
        }
    }

    let user = matched_user.unwrap_or_else(|| {
        let sessions = state.sessions.read().unwrap();
        sessions
            .values()
            .find(|u| u.username.eq_ignore_ascii_case(&clean_user))
            .cloned()
            .unwrap_or_else(|| {
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                UserProfile {
                    id: format!("usr:{}", clean_user),
                    provider: "community".to_string(),
                    username: clean_user.clone(),
                    display_name: clean_user.clone(),
                    avatar_url: Some("/logo.jpg".to_string()),
                    created_at: now,
                    expires_at: None,
                }
            })
    });

    let is_own = current_auth
        .as_ref()
        .map(|u| u.id == user.id || u.username.eq_ignore_ascii_case(&user.username))
        .unwrap_or(false);

    render_profile_view(&user, &state, is_own).await
}

async fn render_profile_view(
    user: &UserProfile,
    state: &AppState,
    is_own_profile: bool,
) -> axum::response::Response {
    let (joined_groups, user_posts) = {
        let groups = state.groups.read().unwrap();
        let members_map = state.group_members.read().unwrap();

        let joined: Vec<Jamiyat> = groups
            .iter()
            .filter(|g| {
                members_map
                    .get(&g.slug)
                    .map(|m| m.contains(&user.username))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();

        let all_posts = state.posts.read().unwrap();
        let posts: Vec<Post> = all_posts
            .iter()
            .filter(|p| {
                p.author_name.as_deref() == Some(&user.username)
                    || p.author_name.as_deref() == Some(&user.display_name)
            })
            .cloned()
            .collect();

        (joined, posts)
    };

    let mut identities = Vec::new();
    if let Some(ref pool) = state.db {
        if let Ok(rows) = sqlx::query(
            "SELECT provider, provider_id, provider_username, avatar_url FROM user_identities WHERE user_id = $1"
        )
        .bind(&user.id)
        .fetch_all(pool)
        .await {
            for r in rows {
                let prov: String = r.try_get("provider").unwrap_or_default();
                let pid: String = r.try_get("provider_id").unwrap_or_default();
                let puname: Option<String> = r.try_get("provider_username").ok();
                let aurl: Option<String> = r.try_get("avatar_url").ok();
                identities.push(UserIdentity {
                    provider: prov,
                    provider_id: pid,
                    provider_username: puname,
                    avatar_url: aurl,
                });
            }
        }
    }

    let has_github = identities.iter().any(|i| i.provider == "github");
    let has_telegram = identities.iter().any(|i| i.provider == "telegram");
    let github_username = identities
        .iter()
        .find(|i| i.provider == "github")
        .and_then(|i| i.provider_username.clone());
    let telegram_username = identities
        .iter()
        .find(|i| i.provider == "telegram")
        .and_then(|i| i.provider_username.clone());

    let can_unlink = identities.len() > 1;
    let mut active_tokens = Vec::new();

    if is_own_profile {
        if let Some(ref pool) = state.db {
            if let Ok(rows) = sqlx::query(
                "SELECT pat.id, pat.client_id, COALESCE(oc.client_name, pat.client_id) as client_name,
                        pat.name, pat.created_at, pat.last_used_at, pat.scopes
                 FROM personal_access_tokens pat
                 LEFT JOIN oauth_clients oc ON pat.client_id = oc.client_id
                 WHERE pat.user_id = $1
                 ORDER BY pat.created_at DESC",
            )
            .bind(&user.id)
            .fetch_all(pool)
            .await {
                for r in rows {
                    let tid: String = r.try_get("id").unwrap_or_default();
                    let cid: String = r.try_get("client_id").unwrap_or_default();
                    let cname: String = r.try_get("client_name").unwrap_or_default();
                    let name: String = r.try_get("name").unwrap_or_default();
                    let cat: chrono::DateTime<Utc> = r.try_get("created_at").unwrap_or_else(|_| Utc::now());
                    let lat: Option<chrono::DateTime<Utc>> = r.try_get("last_used_at").ok().flatten();
                    let scopes: Vec<String> = r.try_get("scopes").unwrap_or_default();

                    active_tokens.push(ActiveTokenItem {
                        id: tid,
                        client_id: cid,
                        client_name: cname,
                        name,
                        created_at: cat.format("%Y-%m-%d %H:%M").to_string(),
                        last_used_at: lat.map(|t| t.format("%Y-%m-%d %H:%M").to_string()),
                        scopes,
                    });
                }
            }
        }
    }

    let template = ProfileTemplate {
        user: user.clone(),
        joined_groups,
        user_posts,
        identities,
        is_own_profile,
        has_github,
        has_telegram,
        github_username,
        telegram_username,
        active_tokens,
        can_unlink,
    };

    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn install_sh_handler() -> impl IntoResponse {
    let content = std::fs::read_to_string("public/install.sh")
        .unwrap_or_else(|_| std::fs::read_to_string("../static/install.sh").unwrap_or_default());
    ([(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], content)
}

async fn install_ps1_handler() -> impl IntoResponse {
    let content = std::fs::read_to_string("public/install.ps1")
        .unwrap_or_else(|_| std::fs::read_to_string("../static/install.ps1").unwrap_or_default());
    ([(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], content)
}

async fn robots_handler() -> impl IntoResponse {
    let content = "User-agent: *\nAllow: /\nAllow: /topics\nAllow: /groups\nAllow: /f/\nAllow: /@\nAllow: /profile/\nDisallow: /api/\nDisallow: /device\n\nSitemap: https://ferris.uz/sitemap.xml\n";
    ([(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")], content)
}

async fn dynamic_sitemap_handler(State(state): State<AppState>) -> impl IntoResponse {
    let now = Utc::now().format("%Y-%m-%d").to_string();

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");

    // 1. Asosiy statik sahifalar
    let static_pages = [
        ("https://ferris.uz/", "1.0", "daily"),
        ("https://topics.ferris.uz", "0.9", "hourly"),
        ("https://groups.ferris.uz", "0.9", "daily"),
        ("https://ferris.uz/device", "0.7", "monthly"),
        ("https://ferris.uz/privacy", "0.5", "monthly"),
        ("https://ferris.uz/terms", "0.5", "monthly"),
    ];

    for (url, priority, changefreq) in static_pages {
        xml.push_str(&format!(
            "  <url>\n    <loc>{}</loc>\n    <lastmod>{}</lastmod>\n    <changefreq>{}</changefreq>\n    <priority>{}</priority>\n  </url>\n",
            url, now, changefreq, priority
        ));
    }

    // 2. Dinamik Jamiyatlar (Groups)
    {
        let groups = state.groups.read().unwrap();
        for g in groups.iter() {
            xml.push_str(&format!(
                "  <url>\n    <loc>https://ferris.uz/f/{}</loc>\n    <lastmod>{}</lastmod>\n    <changefreq>daily</changefreq>\n    <priority>0.8</priority>\n  </url>\n",
                g.slug,
                g.updated_at.format("%Y-%m-%d")
            ));
        }
    }

    // 3. Dinamik Mavzular (Topics)
    {
        let topics = state.topics.read().unwrap();
        for t in topics.iter() {
            xml.push_str(&format!(
                "  <url>\n    <loc>https://topics.ferris.uz#{}</loc>\n    <lastmod>{}</lastmod>\n    <changefreq>daily</changefreq>\n    <priority>0.8</priority>\n  </url>\n",
                t.slug, now
            ));
        }
    }

    // 4. Dinamik Foydalanuvchi Profillari (Users)
    if let Some(pool) = &state.db {
        if let Ok(rows) = sqlx::query("SELECT username, updated_at FROM users WHERE username IS NOT NULL LIMIT 200")
            .fetch_all(pool)
            .await
        {
            for r in rows {
                let uname: Option<String> = r.get("username");
                if let Some(username) = uname {
                    let updated_at: chrono::DateTime<Utc> = r.get("updated_at");
                    xml.push_str(&format!(
                        "  <url>\n    <loc>https://ferris.uz/@{}</loc>\n    <lastmod>{}</lastmod>\n    <changefreq>weekly</changefreq>\n    <priority>0.7</priority>\n  </url>\n",
                        username,
                        updated_at.format("%Y-%m-%d")
                    ));
                }
            }
        }
    }

    xml.push_str("</urlset>");
    ([(axum::http::header::CONTENT_TYPE, "application/xml; charset=utf-8")], xml)
}

// ==============================================================================
// GLOBAL INSTANT SEARCH API
// ==============================================================================

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchTopicItem {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchPostItem {
    pub id: Uuid,
    pub title: String,
    pub topic_slug: Option<String>,
    pub author_name: Option<String>,
    pub created_at: String,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
pub struct SearchGroupItem {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: String,
    pub members_count: i32,
}

#[derive(Debug, Serialize)]
pub struct SearchUserItem {
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub topics: Vec<SearchTopicItem>,
    pub posts: Vec<SearchPostItem>,
    pub groups: Vec<SearchGroupItem>,
    pub users: Vec<SearchUserItem>,
}

async fn search_handler(
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    let q = match params.q {
        Some(s) => s.trim().to_lowercase(),
        None => String::new(),
    };

    if q.len() < 2 {
        return Json(SearchResponse {
            query: q,
            topics: vec![],
            posts: vec![],
            groups: vec![],
            users: vec![],
        });
    }

    // 1. Mavzular (Topics)
    let topics: Vec<SearchTopicItem> = {
        let guard = state.topics.read().unwrap();
        guard
            .iter()
            .filter(|t| {
                t.slug.to_lowercase().contains(&q)
                    || t.title.to_lowercase().contains(&q)
                    || t.description.as_deref().unwrap_or("").to_lowercase().contains(&q)
            })
            .take(5)
            .map(|t| SearchTopicItem {
                slug: t.slug.clone(),
                title: t.title.clone(),
                description: t.description.clone(),
            })
            .collect()
    };

    // 2. Jamiyatlar (Groups)
    let groups: Vec<SearchGroupItem> = {
        let guard = state.groups.read().unwrap();
        guard
            .iter()
            .filter(|g| {
                g.name.to_lowercase().contains(&q)
                    || g.slug.to_lowercase().contains(&q)
                    || g.description.as_deref().unwrap_or("").to_lowercase().contains(&q)
            })
            .take(5)
            .map(|g| SearchGroupItem {
                slug: g.slug.clone(),
                name: g.name.clone(),
                description: g.description.clone(),
                avatar_url: g.avatar_url.clone(),
                members_count: g.members_count,
            })
            .collect()
    };

    // 3. Postlar (Muhokamalar)
    let posts: Vec<SearchPostItem> = {
        let guard = state.posts.read().unwrap();
        guard
            .iter()
            .filter(|p| {
                p.title.to_lowercase().contains(&q) || p.content.to_lowercase().contains(&q)
            })
            .take(15)
            .map(|p| {
                let content_lower = p.content.to_lowercase();
                let snippet = if let Some(idx) = content_lower.find(&q) {
                    let start = idx.saturating_sub(40);
                    let end = (idx + q.len() + 80).min(p.content.len());
                    let sub = &p.content[start..end];
                    format!("...{}...", sub.replace('\n', " "))
                } else {
                    let end = 100.min(p.content.len());
                    format!("{}...", &p.content[..end].replace('\n', " "))
                };

                SearchPostItem {
                    id: p.id,
                    title: p.title.clone(),
                    topic_slug: p.topic_slug.clone(),
                    author_name: p.author_name.clone(),
                    created_at: p.created_at.format("%Y-%m-%d").to_string(),
                    snippet,
                }
            })
            .collect()
    };

    // 4. Foydalanuvchilar (Users)
    let mut users = Vec::new();
    if let Some(pool) = &state.db {
        let pattern = format!("%{}%", q);
        if let Ok(rows) = sqlx::query(
            "SELECT username, display_name, avatar_url FROM users WHERE username ILIKE $1 OR display_name ILIKE $1 LIMIT 6"
        )
        .bind(&pattern)
        .fetch_all(pool)
        .await
        {
            for r in rows {
                let uname: Option<String> = r.try_get("username").ok().flatten();
                if let Some(username) = uname {
                    let display_name: String = r.try_get("display_name").unwrap_or_else(|_| "Rustacean".to_string());
                    let avatar_url: String = r.try_get("avatar_url").unwrap_or_else(|_| "/logo.png".to_string());
                    users.push(SearchUserItem {
                        username,
                        display_name,
                        avatar_url,
                    });
                }
            }
        }
    } else {
        let sessions = state.sessions.read().unwrap();
        for (_, profile) in sessions.iter() {
            if profile.username.to_lowercase().contains(&q)
                || profile.display_name.to_lowercase().contains(&q)
            {
                if !users.iter().any(|u: &SearchUserItem| u.username == profile.username) {
                    users.push(SearchUserItem {
                        username: profile.username.clone(),
                        display_name: profile.display_name.clone(),
                        avatar_url: profile.avatar_url.clone().unwrap_or_else(|| "/logo.png".to_string()),
                    });
                }
            }
            if users.len() >= 6 {
                break;
            }
        }
    }

    Json(SearchResponse {
        query: q,
        topics,
        posts,
        groups,
        users,
    })
}

// ==============================================================================
// SOVEREIGN USERNAME (FERRIS ID) VALIDATION & HANDLERS
// ==============================================================================

pub const RESERVED_USERNAMES: &[&str] = &[
    "admin", "administrator", "root", "system", "ferris", "ferrisuz", "rust", "rustuz",
    "official", "support", "help", "mod", "moderator", "bot", "security", "staff",
    "topics", "groups", "profile", "privacy", "terms", "auth", "oauth", "device",
    "settings", "static", "public", "install", "ping", "stats", "context", "dm",
    "chat", "live", "ws", "search", "api", "doc", "book", "playground", "leaderboard",
];

pub fn validate_sovereign_username(u: &str) -> Result<String, &'static str> {
    let clean = u.trim().to_lowercase();
    if clean.len() < 3 {
        return Err("Username kamida 3 ta belgidan iborat bo'lishi kerak");
    }
    if clean.len() > 20 {
        return Err("Username ko'pi bilan 20 ta belgidan oshmasligi kerak");
    }
    let chars: Vec<char> = clean.chars().collect();
    if !chars[0].is_ascii_alphanumeric() || !chars.last().unwrap().is_ascii_alphanumeric() {
        return Err("Username harf yoki raqam bilan boshlanishi va tugashi kerak");
    }
    for c in &chars {
        if !c.is_ascii_alphanumeric() && *c != '_' {
            return Err("Faqat kichik lotin harflari, raqamlar va pastki chiziqdan foydalaning");
        }
    }
    if clean.contains("__") {
        return Err("Ketma-ket ikkita pastki chiziq qo'llash mumkin emas");
    }
    if RESERVED_USERNAMES.contains(&clean.as_str()) {
        return Err("Ushbu nom tizim tomonidan band qilingan");
    }
    Ok(clean)
}

#[derive(Debug, Deserialize)]
pub struct CheckUsernameQuery {
    pub u: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CheckUsernameResponse {
    pub username: String,
    pub available: bool,
    pub message: String,
}

async fn check_username_handler(
    State(state): State<AppState>,
    Query(params): Query<CheckUsernameQuery>,
) -> impl IntoResponse {
    let raw = params.u.unwrap_or_default();
    let clean = match validate_sovereign_username(&raw) {
        Ok(c) => c,
        Err(err_msg) => {
            return Json(CheckUsernameResponse {
                username: raw,
                available: false,
                message: err_msg.to_string(),
            });
        }
    };

    // Check PostgreSQL
    if let Some(pool) = &state.db {
        let exists = sqlx::query("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
            .bind(&clean)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

        if exists.is_some() {
            return Json(CheckUsernameResponse {
                username: clean,
                available: false,
                message: "Ushbu username allaqachon band qilingan".to_string(),
            });
        }
    } else {
        let sessions = state.sessions.read().unwrap();
        for (_, profile) in sessions.iter() {
            if profile.username.to_lowercase() == clean {
                return Json(CheckUsernameResponse {
                    username: clean,
                    available: false,
                    message: "Ushbu username allaqachon band qilingan".to_string(),
                });
            }
        }
    }

    Json(CheckUsernameResponse {
        username: clean,
        available: true,
        message: "Ushbu username bo'sh va foydalanishga tayyor!".to_string(),
    })
}

#[derive(Debug, Deserialize)]
pub struct UpdateUsernameRequest {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateUsernameResponse {
    pub success: bool,
    pub username: String,
    pub message: String,
}

async fn update_username_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateUsernameRequest>,
) -> Result<Json<UpdateUsernameResponse>, (StatusCode, Json<serde_json::Value>)> {
    let current_user = get_authenticated_user(&headers, &state).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Avval tizimga kiring" })),
        )
    })?;

    let clean = validate_sovereign_username(&payload.username).map_err(|err_msg| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err_msg })),
        )
    })?;

    if current_user.username.to_lowercase() == clean {
        return Ok(Json(UpdateUsernameResponse {
            success: true,
            username: current_user.username,
            message: "Username o'zgarishsiz qoldi".to_string(),
        }));
    }

    let old_username = current_user.username.clone();

    // Check collision in PostgreSQL
    if let Some(pool) = &state.db {
        let collision = sqlx::query(
            "SELECT id FROM users WHERE LOWER(username) = LOWER($1) AND id != $2"
        )
        .bind(&clean)
        .bind(&current_user.id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Baza xatoligi: {}", e) })),
            )
        })?;

        if collision.is_some() {
            return Err((
                StatusCode::CONFLICT,
                Json(serde_json::json!({ "error": "Ushbu username boshqa foydalanuvchi tomonidan band qilingan" })),
            ));
        }

        // Update database (users, posts, comments)
        let _ = sqlx::query("UPDATE users SET username = $1 WHERE id = $2")
            .bind(&clean)
            .bind(&current_user.id)
            .execute(pool)
            .await;

        let _ = sqlx::query("UPDATE posts SET author_name = $1 WHERE author_name = $2")
            .bind(&clean)
            .bind(&old_username)
            .execute(pool)
            .await;

        let _ = sqlx::query("UPDATE comments SET author_name = $1 WHERE author_name = $2")
            .bind(&clean)
            .bind(&old_username)
            .execute(pool)
            .await;
    }

    // Update in-memory sessions
    {
        let mut sessions = state.sessions.write().unwrap();
        for (_, profile) in sessions.iter_mut() {
            if profile.id == current_user.id {
                profile.username = clean.clone();
            }
        }
    }

    // Update in-memory posts
    {
        let mut posts = state.posts.write().unwrap();
        for p in posts.iter_mut() {
            if p.author_id.to_string() == current_user.id {
                p.author_name = Some(clean.clone());
            }
        }
    }

    Ok(Json(UpdateUsernameResponse {
        success: true,
        username: clean,
        message: "Sizning suveren Ferris username'ingiz muvaffaqiyatli saqlandi!".to_string(),
    }))
}

// ==============================================================================
// REAL RUST PLAYGROUND API (RUSTC 1.85+ PROXY)
// ==============================================================================

#[derive(Debug, Deserialize)]
pub struct PlaygroundExecuteRequest {
    pub code: String,
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default = "default_edition")]
    pub edition: String,
}

fn default_channel() -> String {
    "stable".to_string()
}
fn default_edition() -> String {
    "2021".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PlaygroundExecuteResponse {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    #[serde(rename = "exitDetail")]
    pub exit_detail: Option<String>,
}

async fn playground_execute_handler(
    State(state): State<AppState>,
    Json(payload): Json<PlaygroundExecuteRequest>,
) -> impl IntoResponse {
    if payload.code.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Kod bo'sh bo'lishi mumkin emas" })),
        )
            .into_response();
    }

    if payload.code.len() > 65_536 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Kod hajmi 64KB dan oshmasligi kerak" })),
        )
            .into_response();
    }

    let body = serde_json::json!({
        "channel": payload.channel,
        "mode": "debug",
        "edition": payload.edition,
        "crateType": "bin",
        "tests": false,
        "code": payload.code,
        "backtrace": false
    });

    let resp = match state
        .http_client
        .post("https://play.rust-lang.org/execute")
        .json(&body)
        .timeout(Duration::from_secs(15))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": format!("Rust Playground dvigateliga ulanishda xatolik: {}", e) })),
            )
                .into_response();
        }
    };

    match resp.json::<PlaygroundExecuteResponse>().await {
        Ok(data) => Json(data).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Natijani tahlil qilib bo'lmadi: {}", e) })),
        )
            .into_response(),
    }
}

// ==============================================================================
// RUSTACEAN LEADERBOARD & BADGES (KARMA SYSTEM)
// ==============================================================================

#[derive(Debug, Serialize)]
pub struct LeaderboardUserItem {
    pub rank: usize,
    pub username: String,
    pub display_name: String,
    pub avatar_url: String,
    pub karma: i64,
    pub posts_count: i64,
    pub comments_count: i64,
    pub total_views: i64,
    pub badges: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct LeaderboardResponse {
    pub users: Vec<LeaderboardUserItem>,
    pub total_count: usize,
}

async fn leaderboard_handler(State(state): State<AppState>) -> impl IntoResponse {
    let mut leaderboard_users = Vec::new();

    if let Some(pool) = &state.db {
        let query = r#"
            SELECT 
                u.id, 
                u.username, 
                u.display_name, 
                u.avatar_url, 
                u.created_at,
                COALESCE(p.posts_count, 0) as posts_count,
                COALESCE(p.total_views, 0) as total_views,
                COALESCE(c.comments_count, 0) as comments_count
            FROM users u
            LEFT JOIN (
                SELECT author_name, COUNT(*) as posts_count, SUM(views_count) as total_views 
                FROM posts 
                GROUP BY author_name
            ) p ON LOWER(p.author_name) = LOWER(u.username)
            LEFT JOIN (
                SELECT author_name, COUNT(*) as comments_count 
                FROM comments 
                GROUP BY author_name
            ) c ON LOWER(c.author_name) = LOWER(u.username)
            WHERE u.username IS NOT NULL
            ORDER BY (COALESCE(p.posts_count, 0) * 15 + COALESCE(c.comments_count, 0) * 5 + COALESCE(p.total_views, 0) / 10) DESC
            LIMIT 20
        "#;

        if let Ok(rows) = sqlx::query(query).fetch_all(pool).await {
            for (idx, r) in rows.into_iter().enumerate() {
                let uname: Option<String> = r.try_get("username").ok().flatten();
                if let Some(username) = uname {
                    let user_id: String = r.try_get("id").unwrap_or_default();
                    let display_name: String = r.try_get("display_name").unwrap_or_else(|_| "Rustacean".to_string());
                    let avatar_url: String = r.try_get("avatar_url").unwrap_or_else(|_| "/logo.png".to_string());
                    let posts_count: i64 = r.try_get("posts_count").unwrap_or(0);
                    let comments_count: i64 = r.try_get("comments_count").unwrap_or(0);
                    let total_views: i64 = r.try_get("total_views").unwrap_or(0);
                    let is_admin = user_id == "tg:5415350162" || user_id.contains("ibrohimkoha") || username.to_lowercase() == "iskurama";

                    let karma = (posts_count * 15) + (comments_count * 5) + (total_views / 10);

                    let mut badges = Vec::new();
                    badges.push("🦀 Rust Pioneer".to_string());
                    badges.push("🛡️ PQC Verified".to_string());
                    if posts_count >= 2 {
                        badges.push("⚡ Systems Architect".to_string());
                    }
                    if comments_count >= 1 {
                        badges.push("💬 Core Contributor".to_string());
                    }
                    if is_admin {
                        badges.push("👑 Community Founder".to_string());
                    }

                    leaderboard_users.push(LeaderboardUserItem {
                        rank: idx + 1,
                        username,
                        display_name,
                        avatar_url,
                        karma,
                        posts_count,
                        comments_count,
                        total_views,
                        badges,
                    });
                }
            }
        }
    }

    let total = leaderboard_users.len();
    Json(LeaderboardResponse {
        users: leaderboard_users,
        total_count: total,
    })
}

async fn favicon_handler() -> impl IntoResponse {
    let bytes = std::fs::read("public/favicon.ico")
        .unwrap_or_else(|_| include_bytes!("../../static/favicon.ico").to_vec());
    ([(axum::http::header::CONTENT_TYPE, "image/x-icon")], bytes)
}

async fn privacy_page_handler() -> impl IntoResponse {
    let template = PrivacyTemplate {};
    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn terms_page_handler() -> impl IntoResponse {
    let template = TermsTemplate {};
    match template.render() {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ==============================================================================
// 1. TIZIM VA SUBDOMEN CONTEXT HANDLER
// ==============================================================================
async fn ping_handler() -> Json<serde_json::Value> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    Json(serde_json::json!({
        "status": "online",
        "engine": "Ferris.uz 2.0 (Rust Tokio Axum)",
        "timestamp": now,
        "kernel": "Linux 6.x",
        "zero_cost": true
    }))
}

async fn context_handler(headers: HeaderMap) -> Json<serde_json::Value> {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("ferris.uz");

    let active_subdomain = if host.starts_with("topics.") {
        "topics"
    } else if host.starts_with("groups.") {
        "groups"
    } else if host.starts_with("profile.") {
        "profile"
    } else if host.starts_with("api.") {
        "api"
    } else {
        "main"
    };

    Json(serde_json::json!({
        "host": host,
        "subdomain": active_subdomain,
        "ssl_mode": "Cloudflare Universal SSL (Single-Level)",
        "routing": {
            "main": "https://ferris.uz",
            "topics": "https://topics.ferris.uz",
            "groups": "https://groups.ferris.uz",
            "profile": "https://profile.ferris.uz",
            "api": "https://api.ferris.uz"
        }
    }))
}

async fn stats_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    let online = *state.online_count.read().unwrap();
    let history_len = state.chat_history.read().unwrap().len();
    let topics_count = state.topics.read().unwrap().len();
    let groups_count = state.groups.read().unwrap().len();
    let posts_count = state.posts.read().unwrap().len();

    Json(serde_json::json!({
        "active_ws": online,
        "chat_messages": history_len,
        "topics_count": topics_count,
        "groups_count": groups_count,
        "posts_count": posts_count,
        "cpu_usage": 1.2,
        "ram_mb": 14.8,
        "engine": "Axum 0.7 + Tokio (100% Rust)",
        "ssl": "Cloudflare Ready"
    }))
}

// ==============================================================================
// 2. TOPICS HANDLERS (topics.ferris.uz/f/mavzu)
// ==============================================================================
async fn topics_list_handler(State(state): State<AppState>) -> Json<Vec<Topic>> {
    let topics = state.topics.read().unwrap();
    Json(topics.clone())
}

async fn topic_detail_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let topics = state.topics.read().unwrap();
    let full_slug = if slug.starts_with("f/") { slug } else { format!("f/{}", slug) };
    
    if let Some(topic) = topics.iter().find(|t| t.slug == full_slug) {
        let posts = state.posts.read().unwrap();
        let topic_posts: Vec<Post> = posts.iter().filter(|p| p.topic_id == Some(topic.id)).cloned().collect();
        Ok(Json(serde_json::json!({
            "topic": topic,
            "posts": topic_posts
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

// ==============================================================================
// 3. GROUPS HANDLERS (groups.ferris.uz/f/slug)
// ==============================================================================
async fn groups_list_handler(State(state): State<AppState>) -> Json<Vec<Jamiyat>> {
    let mut groups = state.groups.read().unwrap().clone();
    let members_map = state.group_members.read().unwrap();
    for g in groups.iter_mut() {
        if let Some(m) = members_map.get(&g.slug) {
            g.members_count = m.len() as i32;
        }
    }
    Json(groups)
}

#[derive(Deserialize)]
struct CreateGroupPayload {
    slug: String,
    name: String,
    description: Option<String>,
}

async fn group_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateGroupPayload>,
) -> Result<Json<Jamiyat>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Jamiyat ochish uchun tizimga kirishingiz shart!"
                })),
            ));
        }
    };

    let slug = payload.slug.trim().to_lowercase().replace(' ', "-");
    if slug.is_empty() || payload.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Slug va jamiyat nomi bo'sh bo'lishi mumkin emas!" })),
        ));
    }

    let mut groups = state.groups.write().unwrap();
    if groups.iter().any(|g| g.slug == slug) {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({ "error": "Bu slug bilan jamiyat allaqachon mavjud!" })),
        ));
    }

    let new_group = Jamiyat {
        id: Uuid::new_v4(),
        slug: slug.clone(),
        name: payload.name,
        description: payload.description,
        avatar_url: "/logo.jpg".to_string(),
        owner_id: None,
        members_count: 1, // Yaratuvchining o'zi
        empty_since: None,
        is_bot_enabled: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    groups.push(new_group.clone());
    drop(groups);

    let mut members_map = state.group_members.write().unwrap();
    let members_set = members_map.entry(slug).or_insert_with(HashSet::new);
    members_set.insert(auth_user.username.clone());
    drop(members_map);

    info!("👥 Yangi jamiyat yaratildi: {}", new_group.slug);
    save_db(&state);

    if let Some(ref pool) = state.db {
        let pool = pool.clone();
        let g = new_group.clone();
        let u = auth_user.username.clone();
        tokio::spawn(async move {
            let _ = sqlx::query(
                "INSERT INTO groups (id, slug, name, description, avatar_url, owner_id, empty_since, is_bot_enabled, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"
            )
            .bind(g.id)
            .bind(&g.slug)
            .bind(&g.name)
            .bind(&g.description)
            .bind(&g.avatar_url)
            .bind(g.owner_id)
            .bind(g.empty_since)
            .bind(g.is_bot_enabled)
            .bind(g.created_at)
            .bind(g.updated_at)
            .execute(&pool)
            .await;

            let _ = sqlx::query("INSERT INTO group_members (group_slug, username) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                .bind(&g.slug)
                .bind(&u)
                .execute(&pool)
                .await;
        });
    }

    Ok(Json(new_group))
}

async fn group_detail_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<Jamiyat>, StatusCode> {
    let groups = state.groups.read().unwrap();
    if let Some(group) = groups.iter().find(|g| g.slug == slug) {
        Ok(Json(group.clone()))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn group_join_handler(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Jamiyatga a'zo bo'lish uchun tizimga kirishingiz shart!"
                })),
            ));
        }
    };

    let username = auth_user.username.clone();
    let (is_now_member, count) = {
        let mut members_map = state.group_members.write().unwrap();
        let members_set = members_map.entry(slug.clone()).or_insert_with(HashSet::new);
        let is_member = if members_set.contains(&username) {
            members_set.remove(&username);
            false // a'zolikdan chiqdi
        } else {
            members_set.insert(username.clone());
            true // a'zo bo'ldi
        };
        (is_member, members_set.len() as i32)
    };

    let found = {
        let mut groups = state.groups.write().unwrap();
        if let Some(group) = groups.iter_mut().find(|g| g.slug == slug) {
            group.members_count = count;
            if count == 0 {
                group.empty_since = Some(Utc::now());
            } else {
                group.empty_since = None;
            }
            true
        } else {
            false
        }
    };

    if found {
        save_db(&state);

        if let Some(ref pool) = state.db {
            if is_now_member {
                let _ = sqlx::query("INSERT INTO group_members (group_slug, username) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                    .bind(&slug).bind(&username).execute(pool).await;
            } else {
                let _ = sqlx::query("DELETE FROM group_members WHERE group_slug = $1 AND username = $2")
                    .bind(&slug).bind(&username).execute(pool).await;
            }
            let _ = sqlx::query("UPDATE groups SET members_count = $1 WHERE slug = $2")
                .bind(count).bind(&slug).execute(pool).await;
        }

        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let action_str = if is_now_member { "a'zo bo'ldi 🎉" } else { "a'zolikdan chiqdi" };
        let msg_id = Uuid::new_v4().to_string();
        let msg_text = format!("👥 f/{} jamiyatiga {}", slug, action_str);
        let prev_hash = {
            let hist = state.chat_history.read().unwrap();
            hist.iter().rev().find(|m| m.room == "general").and_then(|m| m.msg_hash.clone())
        };
        let msg_hash = compute_chat_hash(&msg_id, "general", &auth_user.username, &msg_text, now, prev_hash.as_deref());
        let chat_msg = ChatMessage {
            id: msg_id,
            room: "general".to_string(),
            user: auth_user.clone(),
            text: msg_text,
            timestamp: now,
            prev_hash,
            msg_hash: Some(msg_hash),
        };
        {
            let mut hist = state.chat_history.write().unwrap();
            hist.push(chat_msg.clone());
            if hist.len() > 200 { hist.remove(0); }
        }

        let _ = state.chat_tx.send(WsEvent::GroupActivity {
            group_slug: slug.clone(),
            username: username.clone(),
            action: action_str.to_string(),
            members_count: count,
            timestamp: now,
        });
        let _ = state.chat_tx.send(WsEvent::CommunityChat(chat_msg));

        Ok(Json(serde_json::json!({
            "success": true,
            "slug": slug,
            "is_member": is_now_member,
            "members_count": count,
            "username": username
        })))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Jamiyat topilmadi!" })),
        ))
    }
}

// ==============================================================================
// 4. POSTS & COMMENTS HANDLERS
// ==============================================================================
#[derive(Serialize, Clone)]
struct PostListItem {
    #[serde(flatten)]
    post: Post,
    comments_count: usize,
}

async fn posts_list_handler(State(state): State<AppState>) -> Json<Vec<PostListItem>> {
    let posts = state.posts.read().unwrap();
    let comments = state.comments.read().unwrap();

    let mut list: Vec<PostListItem> = posts
        .iter()
        .map(|p| {
            let count = comments.iter().filter(|c| c.post_id == p.id).count();
            PostListItem {
                post: p.clone(),
                comments_count: count,
            }
        })
        .collect();

    list.sort_by(|a, b| b.post.created_at.cmp(&a.post.created_at));
    Json(list)
}

#[derive(Serialize)]
struct PostDetailResponse {
    #[serde(flatten)]
    post: Post,
    comments: Vec<Comment>,
}

async fn post_detail_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<PostDetailResponse>, (StatusCode, Json<serde_json::Value>)> {
    let viewer_key = get_viewer_key(&headers, &state);
    record_post_view(id, &viewer_key, &state);

    let post_opt = {
        let posts = state.posts.read().unwrap();
        posts.iter().find(|p| p.id == id).cloned()
    };

    if let Some(post) = post_opt {
        let comments = {
            let all_comments = state.comments.read().unwrap();
            all_comments.iter().filter(|c| c.post_id == id).cloned().collect()
        };
        Ok(Json(PostDetailResponse { post, comments }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Post topilmadi!" })),
        ))
    }
}

async fn post_record_view_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let viewer_key = get_viewer_key(&headers, &state);
    let recorded = record_post_view(id, &viewer_key, &state);
    let views_count = {
        let posts = state.posts.read().unwrap();
        posts.iter().find(|p| p.id == id).map(|p| p.views_count).unwrap_or(0)
    };
    Json(serde_json::json!({
        "success": true,
        "recorded": recorded,
        "views_count": views_count
    }))
}

#[derive(Deserialize)]
struct AddCommentPayload {
    content: String,
}

async fn add_comment_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<AddCommentPayload>,
) -> Result<Json<Comment>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Fikr bildirish uchun tizimga kirishingiz shart!"
                })),
            ));
        }
    };

    let content_trimmed = payload.content.trim();
    if content_trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Javob matni bo'sh bo'lishi mumkin emas!" })),
        ));
    }
    if content_trimmed.chars().count() > 5000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Izoh matni 5,000 belgidan oshmasligi kerak!" })),
        ));
    }

    let client_ip = get_client_ip(&headers);
    let rate_key = format!("comment:{}:{}", client_ip, auth_user.username);
    if !check_rate_limit(&state.rate_limiter, &rate_key, 10, 30) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": "Juda ko'p izoh yuborildi. Iltimos, 30 soniya kuting (Rate Limit)!" }))
        ));
    }

    let comment = Comment {
        id: Uuid::new_v4(),
        post_id: id,
        author_name: auth_user.display_name.clone(),
        author_avatar: auth_user.avatar_url.clone().unwrap_or_else(|| "/logo.jpg".to_string()),
        content: payload.content,
        created_at: Utc::now(),
    };

    if let Some(ref pool) = state.db {
        if let Err(e) = sqlx::query(
            "INSERT INTO comments (id, post_id, author_name, author_avatar, content, created_at) VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(comment.id)
        .bind(comment.post_id)
        .bind(&comment.author_name)
        .bind(&comment.author_avatar)
        .bind(&comment.content)
        .bind(comment.created_at)
        .execute(pool)
        .await {
            tracing::error!("PostgreSQL INSERT comment error: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Izohni bazaga saqlashda xatolik yuz berdi" }))));
        }
    }

    {
        let mut comments = state.comments.write().unwrap();
        comments.push(comment.clone());
    }

    save_db(&state);

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let post_title = {
        let posts = state.posts.read().unwrap();
        posts.iter().find(|p| p.id == id).map(|p| p.title.clone()).unwrap_or_else(|| "Mavzu".to_string())
    };

    let msg_id = Uuid::new_v4().to_string();
    let msg_text = format!("💬 Yangi javob qoldirdi: \"{}\"", comment.content);
    let prev_hash = {
        let hist = state.chat_history.read().unwrap();
        hist.iter().rev().find(|m| m.room == "general").and_then(|m| m.msg_hash.clone())
    };
    let msg_hash = compute_chat_hash(&msg_id, "general", &auth_user.username, &msg_text, now, prev_hash.as_deref());
    let chat_msg = ChatMessage {
        id: msg_id,
        room: "general".to_string(),
        user: auth_user.clone(),
        text: msg_text,
        timestamp: now,
        prev_hash,
        msg_hash: Some(msg_hash),
    };
    {
        let mut hist = state.chat_history.write().unwrap();
        hist.push(chat_msg.clone());
        if hist.len() > 200 {
            hist.remove(0);
        }
    }

    let _ = state.chat_tx.send(WsEvent::PostComment {
        id: comment.id.to_string(),
        post_id: id,
        post_title,
        author_name: auth_user.username.clone(),
        content: comment.content.clone(),
        timestamp: now,
    });
    let _ = state.chat_tx.send(WsEvent::CommunityChat(chat_msg));

    info!("💬 Yangi javob qo'shildi: post {} by @{}", id, auth_user.username);
    Ok(Json(comment))
}

async fn get_comments_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Json<Vec<Comment>> {
    let comments = state.comments.read().unwrap();
    let post_comments = comments.iter().filter(|c| c.post_id == id).cloned().collect();
    Json(post_comments)
}

#[derive(Deserialize)]
struct CreatePostPayload {
    topic_slug: Option<String>,
    jamiyat_slug: Option<String>,
    title: String,
    content: String,
}

async fn post_create_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreatePostPayload>,
) -> Result<Json<Post>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Muhokama chop etish uchun avval tizimga kiring!"
                })),
            ));
        }
    };

    let title_trimmed = payload.title.trim();
    let content_trimmed = payload.content.trim();
    if title_trimmed.is_empty() || content_trimmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Sarlavha va matn bo'sh bo'lishi mumkin emas!" })),
        ));
    }
    if title_trimmed.chars().count() > 200 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Sarlavha 200 belgidan oshmasligi kerak!" })),
        ));
    }
    if content_trimmed.chars().count() > 50000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Post matni 50,000 belgidan oshmasligi kerak!" })),
        ));
    }

    let client_ip = get_client_ip(&headers);
    let rate_key = format!("post:{}:{}", client_ip, auth_user.username);
    if !check_rate_limit(&state.rate_limiter, &rate_key, 5, 30) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": "Juda ko'p post yuborildi. Iltimos, 30 soniya kuting (Rate Limit)!" }))
        ));
    }

    let topic_id = if let Some(slug) = &payload.topic_slug {
        let topics = state.topics.read().unwrap();
        topics.iter().find(|t| t.slug == *slug).map(|t| t.id)
    } else {
        None
    };

    let jamiyat_id = if let Some(slug) = &payload.jamiyat_slug {
        let groups = state.groups.read().unwrap();
        groups.iter().find(|g| g.slug == *slug).map(|g| g.id)
    } else {
        None
    };

    let new_post = Post {
        id: Uuid::new_v4(),
        topic_slug: payload.topic_slug.clone(),
        topic_id,
        jamiyat_id,
        author_id: Uuid::new_v4(),
        author_name: Some(auth_user.display_name.clone()),
        author_avatar: auth_user.avatar_url.clone().or_else(|| Some("/logo.jpg".to_string())),
        title: payload.title,
        content: payload.content,
        views_count: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    };

    if let Some(ref pool) = state.db {
        if let Err(e) = sqlx::query(
            "INSERT INTO posts (id, author_name, author_avatar, title, content, views_count, created_at, updated_at) VALUES ($1, $2, $3, $4, $5, 0, $6, $6)"
        )
        .bind(new_post.id)
        .bind(new_post.author_name.as_deref().unwrap_or_default())
        .bind(&new_post.author_avatar)
        .bind(&new_post.title)
        .bind(&new_post.content)
        .bind(new_post.created_at)
        .execute(pool)
        .await {
            tracing::error!("PostgreSQL INSERT post error: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Postni bazaga yozishda xatolik yuz berdi" }))));
        }
    }

    {
        let mut posts = state.posts.write().unwrap();
        posts.push(new_post.clone());
    }

    save_db(&state);

    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let msg_id = Uuid::new_v4().to_string();
    let msg_text = format!("📝 Yangi muhokama ochdi: \"{}\"", new_post.title);
    let prev_hash = {
        let hist = state.chat_history.read().unwrap();
        hist.iter().rev().find(|m| m.room == "general").and_then(|m| m.msg_hash.clone())
    };
    let msg_hash = compute_chat_hash(&msg_id, "general", &auth_user.username, &msg_text, now, prev_hash.as_deref());
    let chat_msg = ChatMessage {
        id: msg_id,
        room: "general".to_string(),
        user: auth_user.clone(),
        text: msg_text,
        timestamp: now,
        prev_hash,
        msg_hash: Some(msg_hash),
    };
    {
        let mut hist = state.chat_history.write().unwrap();
        hist.push(chat_msg.clone());
        if hist.len() > 200 {
            hist.remove(0);
        }
    }

    let _ = state.chat_tx.send(WsEvent::NewPost {
        id: new_post.id,
        topic_slug: payload.topic_slug.clone(),
        group_slug: payload.jamiyat_slug.clone(),
        author_name: auth_user.username.clone(),
        title: new_post.title.clone(),
        content: new_post.content.clone(),
        timestamp: now,
    });
    let _ = state.chat_tx.send(WsEvent::CommunityChat(chat_msg));

    info!("📝 Yangi post yaratildi: '{}' by @{}", new_post.title, auth_user.username);
    Ok(Json(new_post))
}

#[derive(Deserialize)]
struct UpdatePostPayload {
    title: Option<String>,
    content: Option<String>,
}

async fn post_update_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
    Json(payload): Json<UpdatePostPayload>,
) -> Result<Json<Post>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tish shart!" })))),
    };

    let client_ip = get_client_ip(&headers);
    let rate_key = format!("post_edit:{}:{}", client_ip, auth_user.username);
    if !check_rate_limit(&state.rate_limiter, &rate_key, 10, 30) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({ "error": "Juda ko'p tahrirlash so'rovi yuborildi. Iltimos, bir oz kuting!" }))
        ));
    }

    let (mut post_to_update, is_author, is_admin) = {
        let posts = state.posts.read().unwrap();
        if let Some(p) = posts.iter().find(|p| p.id == id) {
            let author = p.author_name.as_deref() == Some(&auth_user.username) 
                || p.author_name.as_deref() == Some(&auth_user.display_name);
            let admin = auth_user.username == "iskurama";
            (p.clone(), author, admin)
        } else {
            return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Post topilmadi" }))));
        }
    };

    if !is_author && !is_admin {
        return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Faqat post muallifi yoki admin tahrirlashi mumkin!" }))));
    }

    if let Some(new_title) = payload.title {
        let t = new_title.trim();
        if t.is_empty() || t.chars().count() > 200 {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Sarlavha 1-200 belgi oralig'ida bo'lishi kerak!" }))));
        }
        post_to_update.title = t.to_string();
    }
    if let Some(new_content) = payload.content {
        let c = new_content.trim();
        if c.is_empty() || c.chars().count() > 50000 {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Post matni 1-50,000 belgi oralig'ida bo'lishi kerak!" }))));
        }
        post_to_update.content = c.to_string();
    }
    post_to_update.updated_at = Utc::now();

    if let Some(ref pool) = state.db {
        if let Err(e) = sqlx::query("UPDATE posts SET title = $1, content = $2, updated_at = $3 WHERE id = $4")
            .bind(&post_to_update.title)
            .bind(&post_to_update.content)
            .bind(post_to_update.updated_at)
            .bind(id)
            .execute(pool)
            .await {
                tracing::error!("PostgreSQL UPDATE post error: {}", e);
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Postni bazada tahrirlashda xatolik yuz berdi" }))));
            }
    }

    {
        let mut posts = state.posts.write().unwrap();
        if let Some(p) = posts.iter_mut().find(|p| p.id == id) {
            p.title = post_to_update.title.clone();
            p.content = post_to_update.content.clone();
            p.updated_at = post_to_update.updated_at;
        }
    }

    save_db(&state);

    info!("✏️ Post tahrirlandi: '{}' by @{}", post_to_update.title, auth_user.username);
    Ok(Json(post_to_update))
}

async fn post_delete_handler(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tish shart!" })))),
    };

    let found_and_authorized = {
        let posts = state.posts.read().unwrap();
        if let Some(p) = posts.iter().find(|p| p.id == id) {
            let is_author = p.author_name.as_deref() == Some(&auth_user.username) 
                || p.author_name.as_deref() == Some(&auth_user.display_name);
            let is_admin = auth_user.username == "iskurama";
            if !is_author && !is_admin {
                return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Faqat post muallifi yoki admin o'chira oladi!" }))));
            }
            true
        } else {
            false
        }
    };

    if !found_and_authorized {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Post topilmadi" }))));
    }

    if let Some(ref pool) = state.db {
        if let Err(e) = sqlx::query("DELETE FROM posts WHERE id = $1").bind(id).execute(pool).await {
            tracing::error!("PostgreSQL DELETE post error: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Postni bazadan o'chirishda xatolik yuz berdi" }))));
        }
    }

    {
        let mut posts = state.posts.write().unwrap();
        posts.retain(|p| p.id != id);
    }
    {
        let mut comments = state.comments.write().unwrap();
        comments.retain(|c| c.post_id != id);
    }

    save_db(&state);

    info!("🗑️ Post o'chirildi: {} by @{}", id, auth_user.username);
    Ok(Json(serde_json::json!({ "success": true, "deleted_post_id": id })))
}

async fn comment_delete_handler(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tish shart!" })))),
    };

    let target_id = {
        let comments = state.comments.read().unwrap();
        if let Some(c) = comments.iter().find(|c| c.id.to_string().starts_with(&id_str)) {
            let is_comment_author = c.author_name == auth_user.username || c.author_name == auth_user.display_name;
            let is_admin = auth_user.username == "iskurama";
            if !is_comment_author && !is_admin {
                return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Faqat izoh muallifi yoki admin o'chira oladi!" }))));
            }
            c.id
        } else {
            return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Izoh topilmadi" }))));
        }
    };

    if let Some(ref pool) = state.db {
        if let Err(e) = sqlx::query("DELETE FROM comments WHERE id = $1").bind(target_id).execute(pool).await {
            tracing::error!("PostgreSQL DELETE comment error: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Izohni bazadan o'chirishda xatolik yuz berdi" }))));
        }
    }

    {
        let mut comments = state.comments.write().unwrap();
        comments.retain(|c| c.id != target_id);
    }

    save_db(&state);

    info!("🗑️ Izoh o'chirildi: {} by @{}", target_id, auth_user.username);
    Ok(Json(serde_json::json!({ "success": true, "deleted_comment_id": target_id })))
}

// ==============================================================================
// 5. PROFILE HANDLER (profile.ferris.uz/@username)
// ==============================================================================
async fn profile_handler(Path(username): Path<String>) -> Json<serde_json::Value> {
    let clean_user = username.trim_start_matches('@');
    Json(serde_json::json!({
        "username": clean_user,
        "display_name": format!("Ferris Developer (@{})", clean_user),
        "avatar_url": "/logo.jpg",
        "subdomain_url": format!("https://profile.ferris.uz/@{}", clean_user),
        "interests": ["Rust", "Tokio", "Kvant Hisoblash", "Linux"],
        "projects": [
            {
                "title": "ferris",
                "description": "O'zbekiston Rustaceanlar suveren ekotizimi va ferris-cli",
                "github": "https://github.com/rust-uz/ferris"
            }
        ]
    }))
}

// ==============================================================================
// 6. GITHUB OAUTH 2.0 HANDLERS
// ==============================================================================
fn sanitize_return_to(url_str: &str) -> String {
    let fallback = "https://ferris.uz".to_string();
    let trimmed = url_str.trim();
    if trimmed.is_empty() {
        return fallback;
    }
    // Relative path (must start with single '/', not protocol relative '//')
    if trimmed.starts_with('/') && !trimmed.starts_with("//") && !trimmed.contains('\\') {
        return format!("https://ferris.uz{}", trimmed);
    }
    // Absolute URL: only ferris.uz, *.ferris.uz, localhost, 127.0.0.1
    if let Ok(parsed) = reqwest::Url::parse(trimmed) {
        if let Some(host) = parsed.host_str() {
            if host == "ferris.uz" || host.ends_with(".ferris.uz") || host == "localhost" || host == "127.0.0.1" {
                return parsed.to_string();
            }
        }
    }
    fallback
}

fn create_oauth_state(return_to: &str, bound_user_id: Option<&str>, secret: &str) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let sanitized = sanitize_return_to(return_to);
    let bound = bound_user_id.unwrap_or("");
    let payload = format!("{}|{}|{}", sanitized, bound, now);

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC size");
    mac.update(payload.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());

    let raw = format!("{}|{}", payload, sig);
    urlencoding::encode(&raw).to_string()
}

fn verify_oauth_state(raw_state: &str, secret: &str) -> Option<(String, Option<String>)> {
    let decoded = urlencoding::decode(raw_state).ok()?;
    let parts: Vec<&str> = decoded.split('|').collect();
    if parts.len() != 4 {
        return None;
    }
    let return_to = parts[0];
    let bound_user_id = parts[1];
    let ts_str = parts[2];
    let sig_hex = parts[3];

    let ts = ts_str.parse::<u64>().ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if now < ts || now - ts > 600 {
        return None;
    }

    let payload = format!("{}|{}|{}", return_to, bound_user_id, ts);
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(payload.as_bytes());

    if let Ok(sig_bytes) = hex::decode(sig_hex) {
        if mac.verify_slice(&sig_bytes).is_ok() {
            let bound_opt = if bound_user_id.is_empty() {
                None
            } else {
                Some(bound_user_id.to_string())
            };
            return Some((sanitize_return_to(return_to), bound_opt));
        }
    }
    None
}

#[derive(Deserialize)]
struct GitHubLoginQuery {
    return_to: Option<String>,
}

async fn github_login_redirect(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GitHubLoginQuery>,
) -> impl IntoResponse {
    let return_to = query.return_to.unwrap_or_else(|| "https://ferris.uz".to_string());
    let current_user = get_authenticated_user(&headers, &state);
    let bound_uid = current_user.as_ref().map(|u| u.id.as_str());

    let signed_state = create_oauth_state(&return_to, bound_uid, &state.github_client_secret);
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=read:user,user:email&state={}",
        state.github_client_id, signed_state
    );
    Redirect::temporary(&url)
}

#[derive(Deserialize)]
struct GitHubCallbackQuery {
    code: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
struct GitHubTokenResponse {
    access_token: Option<String>,
}

#[derive(Deserialize)]
struct GitHubUserResponse {
    id: i64,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

async fn resolve_unique_username(pool: &PgPool, base: &str, user_id: &str) -> String {
    let clean = base.trim_start_matches('@').trim();
    let initial = if clean.is_empty() { "user" } else { clean };

    let existing = sqlx::query("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(initial)
        .fetch_optional(pool)
        .await;

    match existing {
        Ok(Some(row)) => {
            let existing_id: String = row.try_get("id").unwrap_or_default();
            if existing_id == user_id {
                return initial.to_string();
            }
            // Username collision with another account! Find next available suffix
            for counter in 1..=100 {
                let candidate = format!("{}_{}", initial, counter);
                let check = sqlx::query("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
                    .bind(&candidate)
                    .fetch_optional(pool)
                    .await;
                if let Ok(None) = check {
                    return candidate;
                }
            }
            format!("{}_{}", initial, &Uuid::new_v4().to_string()[..4])
        }
        _ => initial.to_string(),
    }
}

async fn github_callback_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<GitHubCallbackQuery>,
) -> impl IntoResponse {
    let code = match query.code {
        Some(c) => c,
        None => return Redirect::to("/?error=no_github_code").into_response(),
    };

    let (return_to, bound_user_id) = query
        .state
        .as_deref()
        .and_then(|raw| verify_oauth_state(raw, &state.github_client_secret))
        .unwrap_or_else(|| ("https://ferris.uz".to_string(), None));

    let token_res = state
        .http_client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("client_id", state.github_client_id.as_str()),
            ("client_secret", state.github_client_secret.as_str()),
            ("code", &code),
        ])
        .send()
        .await;

    let access_token = match token_res {
        Ok(res) => match res.json::<GitHubTokenResponse>().await {
            Ok(t) => t.access_token,
            Err(_) => None,
        },
        Err(_) => None,
    };

    let token = match access_token {
        Some(t) => t,
        None => return Redirect::to("/?error=github_token_failed").into_response(),
    };

    let user_res = state
        .http_client
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await;

    let gh_user: GitHubUserResponse = match user_res {
        Ok(res) => match res.json().await {
            Ok(u) => u,
            Err(_) => return Redirect::to("/?error=github_profile_failed").into_response(),
        },
        Err(_) => return Redirect::to("/?error=github_profile_failed").into_response(),
    };

    let gh_id_str = gh_user.id.to_string();
    let gh_username = gh_user.login.clone();
    let gh_display_name = gh_user.name.clone().unwrap_or_else(|| gh_username.clone());
    let gh_avatar = gh_user.avatar_url.clone();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    let mut final_profile: Option<UserProfile> = None;

    if let Some(ref pool) = state.db {
        // 1. Check if this GitHub account is already linked to a user via user_identities
        let linked_user_id = sqlx::query(
            "SELECT user_id FROM user_identities WHERE provider = 'github' AND provider_id = $1"
        )
        .bind(&gh_id_str)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String, _>("user_id").ok());

        if let Some(uid) = linked_user_id {
            // Found existing user!
            if let Ok(Some(row)) = sqlx::query(
                "SELECT id, username, display_name, provider, avatar_url, created_at FROM users WHERE id = $1"
            )
            .bind(&uid)
            .fetch_optional(pool)
            .await {
                let uname: String = row.try_get("username").unwrap_or_default();
                let dname: String = row.try_get("display_name").unwrap_or_default();
                let prov: String = row.try_get("provider").unwrap_or_default();
                let aurl: Option<String> = row.try_get("avatar_url").ok();
                let cat: i64 = row.try_get("created_at").unwrap_or(0);
                final_profile = Some(UserProfile {
                    id: uid,
                    provider: prov,
                    username: uname,
                    display_name: dname,
                    avatar_url: aurl,
                    created_at: cat as u64,
                    expires_at: Some(Utc::now() + chrono::Duration::days(30)),
                });
            }
        } else {
            // Not linked yet. Is this an authenticated account linking flow?
            if let Some(ref expected_uid) = bound_user_id {
                let current_auth = get_authenticated_user(&headers, &state);
                if let Some(cur) = current_auth {
                    if &cur.id != expected_uid {
                        warn!("⚠️ OAuth account linking CSRF detected! Logged in: {}, Expected: {}", cur.id, expected_uid);
                        return Redirect::to("/profile?error=account_linking_csrf").into_response();
                    }
                    // Link this GitHub account to the currently logged in user!
                    let _ = sqlx::query(
                        "INSERT INTO user_identities (user_id, provider, provider_id, provider_username, avatar_url)
                         VALUES ($1, 'github', $2, $3, $4)
                         ON CONFLICT (provider, provider_id) DO UPDATE SET user_id = EXCLUDED.user_id, provider_username = EXCLUDED.provider_username"
                    )
                    .bind(&cur.id)
                    .bind(&gh_id_str)
                    .bind(&gh_username)
                    .bind(&gh_avatar)
                    .execute(pool)
                    .await;

                    info!("🔗 GitHub account @{} successfully linked to user @{}", gh_username, cur.username);
                    final_profile = Some(cur);
                } else {
                    return Redirect::to("/login?error=account_linking_session_lost").into_response();
                }
            } else {
                // Brand new user login via GitHub
                let user_id = format!("gh:{}", gh_user.id);
                let clean_username = resolve_unique_username(pool, &gh_username, &user_id).await;

                let _ = sqlx::query(
                    r#"
                    INSERT INTO users (id, provider, username, display_name, avatar_url, created_at)
                    VALUES ($1, 'github', $2, $3, $4, $5)
                    ON CONFLICT (id) DO UPDATE SET
                        username = EXCLUDED.username,
                        display_name = EXCLUDED.display_name,
                        avatar_url = EXCLUDED.avatar_url
                    "#
                )
                .bind(&user_id)
                .bind(&clean_username)
                .bind(&gh_display_name)
                .bind(&gh_avatar)
                .bind(now as i64)
                .execute(pool)
                .await;

                let _ = sqlx::query(
                    "INSERT INTO user_identities (user_id, provider, provider_id, provider_username, avatar_url)
                     VALUES ($1, 'github', $2, $3, $4)
                     ON CONFLICT (provider, provider_id) DO UPDATE SET user_id = EXCLUDED.user_id"
                )
                .bind(&user_id)
                .bind(&gh_id_str)
                .bind(&gh_username)
                .bind(&gh_avatar)
                .execute(pool)
                .await;

                final_profile = Some(UserProfile {
                    id: user_id,
                    provider: "github".to_string(),
                    username: clean_username,
                    display_name: gh_display_name.clone(),
                    avatar_url: gh_avatar.clone(),
                    created_at: now,
                    expires_at: Some(Utc::now() + chrono::Duration::days(30)),
                });
            }
        }
    }

    let profile = match final_profile {
        Some(p) => p,
        None => {
            UserProfile {
                id: format!("gh:{}", gh_user.id),
                provider: "github".to_string(),
                username: gh_username,
                display_name: gh_display_name,
                avatar_url: gh_avatar,
                created_at: now,
                expires_at: Some(Utc::now() + chrono::Duration::days(30)),
            }
        }
    };

    let session_token = uuid::Uuid::new_v4().to_string();

    if let Some(ref pool) = state.db {
        let _ = sqlx::query(
            "INSERT INTO sessions (token, user_id, created_at) VALUES ($1, $2, $3) ON CONFLICT (token) DO NOTHING"
        )
        .bind(&session_token)
        .bind(&profile.id)
        .bind(profile.created_at as i64)
        .execute(pool)
        .await;
    }

    {
        let mut sessions = state.sessions.write().unwrap();
        sessions.insert(session_token.clone(), profile.clone());
    }
    save_db(&state);

    info!("✅ GitHub user authenticated/linked: @{} (user: @{})", gh_user.login, profile.username);

    let final_return_to = if return_to.is_empty() {
        format!("https://ferris.uz/?github_token={}", session_token)
    } else {
        return_to
    };

    let cookie_val = format!(
        "ferris_session={}; Domain=.ferris.uz; Path=/; Max-Age=2592000; SameSite=Lax; Secure",
        session_token
    );

    (
        [
            (axum::http::header::SET_COOKIE, cookie_val),
            (axum::http::header::LOCATION, final_return_to),
        ],
        StatusCode::FOUND,
    )
        .into_response()
}

// ==============================================================================
// 7. TELEGRAM LOGIN HANDLER
// ==============================================================================
#[derive(Deserialize)]
pub struct TelegramAuthPayload {
    pub id: i64,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub username: Option<String>,
    pub photo_url: Option<String>,
    pub auth_date: i64,
    pub hash: String,
}

fn verify_telegram_hash(payload: &TelegramAuthPayload, bot_token: &str) -> bool {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
    // 1. Replay attack protection: reject authentication older than 24 hours or from far in the future
    if payload.auth_date > now + 300 || now - payload.auth_date > 86400 {
        warn!("Telegram auth expired or invalid auth_date: {} (now: {})", payload.auth_date, now);
        return false;
    }

    let mut parts = Vec::new();
    parts.push(format!("auth_date={}", payload.auth_date));
    if let Some(ref first) = payload.first_name {
        parts.push(format!("first_name={}", first));
    }
    parts.push(format!("id={}", payload.id));
    if let Some(ref last) = payload.last_name {
        parts.push(format!("last_name={}", last));
    }
    if let Some(ref photo) = payload.photo_url {
        parts.push(format!("photo_url={}", photo));
    }
    if let Some(ref user) = payload.username {
        parts.push(format!("username={}", user));
    }
    parts.sort();
    let data_check_string = parts.join("\n");

    let mut hasher = Sha256::new();
    hasher.update(bot_token.as_bytes());
    let secret_key = hasher.finalize();

    let mut mac = HmacSha256::new_from_slice(&secret_key).expect("HMAC can take key of any size");
    mac.update(data_check_string.as_bytes());

    // 2. Constant-time verification to prevent timing attacks
    if let Ok(expected_bytes) = hex::decode(&payload.hash) {
        mac.verify_slice(&expected_bytes).is_ok()
    } else {
        false
    }
}

async fn telegram_auth_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TelegramAuthPayload>,
) -> Result<impl IntoResponse, (StatusCode, Json<serde_json::Value>)> {
    if !verify_telegram_hash(&payload, &state.bot_token) {
        warn!("Telegram auth hash mismatch for id: {}", payload.id);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Autentifikatsiya imzosi noto'g'ri (Invalid Hash)" })),
        ));
    }

    let tg_id_str = payload.id.to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let username = payload.username.clone().unwrap_or_else(|| format!("tg_{}", payload.id));
    let display_name = payload.first_name.clone().unwrap_or_else(|| username.clone());

    let mut final_profile: Option<UserProfile> = None;

    if let Some(ref pool) = state.db {
        // 1. Check if Telegram account is already linked via user_identities
        let linked_user_id = sqlx::query(
            "SELECT user_id FROM user_identities WHERE provider = 'telegram' AND provider_id = $1"
        )
        .bind(&tg_id_str)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String, _>("user_id").ok());

        if let Some(uid) = linked_user_id {
            if let Ok(Some(row)) = sqlx::query(
                "SELECT id, username, display_name, provider, avatar_url, created_at FROM users WHERE id = $1"
            )
            .bind(&uid)
            .fetch_optional(pool)
            .await {
                let uname: String = row.try_get("username").unwrap_or_default();
                let dname: String = row.try_get("display_name").unwrap_or_default();
                let prov: String = row.try_get("provider").unwrap_or_default();
                let aurl: Option<String> = row.try_get("avatar_url").ok();
                let cat: i64 = row.try_get("created_at").unwrap_or(0);
                final_profile = Some(UserProfile {
                    id: uid,
                    provider: prov,
                    username: uname,
                    display_name: dname,
                    avatar_url: aurl,
                    created_at: cat as u64,
                    expires_at: Some(Utc::now() + chrono::Duration::days(30)),
                });
            }
        } else {
            // Check if user is currently logged in (linking account)
            let current_auth = get_authenticated_user(&headers, &state);
            if let Some(cur) = current_auth {
                let _ = sqlx::query(
                    "INSERT INTO user_identities (user_id, provider, provider_id, provider_username, avatar_url)
                     VALUES ($1, 'telegram', $2, $3, $4)
                     ON CONFLICT (provider, provider_id) DO UPDATE SET user_id = EXCLUDED.user_id, provider_username = EXCLUDED.provider_username"
                )
                .bind(&cur.id)
                .bind(&tg_id_str)
                .bind(&username)
                .bind(&payload.photo_url)
                .execute(pool)
                .await;

                info!("🔗 Telegram account @{} linked to user @{}", username, cur.username);
                final_profile = Some(cur);
            } else {
                let user_id = format!("tg:{}", payload.id);
                let clean_username = resolve_unique_username(pool, &username, &user_id).await;

                let _ = sqlx::query(
                    r#"
                    INSERT INTO users (id, provider, username, display_name, avatar_url, created_at)
                    VALUES ($1, 'telegram', $2, $3, $4, $5)
                    ON CONFLICT (id) DO UPDATE SET
                        username = EXCLUDED.username,
                        display_name = EXCLUDED.display_name,
                        avatar_url = EXCLUDED.avatar_url
                    "#
                )
                .bind(&user_id)
                .bind(&clean_username)
                .bind(&display_name)
                .bind(&payload.photo_url)
                .bind(now as i64)
                .execute(pool)
                .await;

                let _ = sqlx::query(
                    "INSERT INTO user_identities (user_id, provider, provider_id, provider_username, avatar_url)
                     VALUES ($1, 'telegram', $2, $3, $4)
                     ON CONFLICT (provider, provider_id) DO UPDATE SET user_id = EXCLUDED.user_id"
                )
                .bind(&user_id)
                .bind(&tg_id_str)
                .bind(&username)
                .bind(&payload.photo_url)
                .execute(pool)
                .await;

                final_profile = Some(UserProfile {
                    id: user_id,
                    provider: "telegram".to_string(),
                    username: clean_username,
                    display_name: display_name.clone(),
                    avatar_url: payload.photo_url.clone(),
                    created_at: now,
                    expires_at: Some(Utc::now() + chrono::Duration::days(30)),
                });
            }
        }
    }

    let profile = final_profile.unwrap_or_else(|| UserProfile {
        id: format!("tg:{}", payload.id),
        provider: "telegram".to_string(),
        username: username.clone(),
        display_name: display_name.clone(),
        avatar_url: payload.photo_url.clone(),
        created_at: now,
        expires_at: Some(Utc::now() + chrono::Duration::days(30)),
    });

    let session_token = uuid::Uuid::new_v4().to_string();

    if let Some(ref pool) = state.db {
        let _ = sqlx::query(
            "INSERT INTO sessions (token, user_id, created_at) VALUES ($1, $2, $3) ON CONFLICT (token) DO NOTHING"
        )
        .bind(&session_token)
        .bind(&profile.id)
        .bind(profile.created_at as i64)
        .execute(pool)
        .await;
    }

    {
        let mut sessions = state.sessions.write().unwrap();
        sessions.insert(session_token.clone(), profile.clone());
    }
    save_db(&state);

    info!("✅ Telegram user authenticated/linked: @{} (user: @{})", username, profile.username);

    let cookie_val = format!(
        "ferris_session={}; Domain=.ferris.uz; Path=/; Max-Age=2592000; SameSite=Lax; Secure",
        session_token
    );

    Ok((
        [(axum::http::header::SET_COOKIE, cookie_val)],
        Json(serde_json::json!({
            "success": true,
            "token": session_token,
            "user": profile
        })),
    ))
}

#[derive(Deserialize)]
struct AuthQuery {
    token: Option<String>,
}

async fn auth_me_handler(
    State(state): State<AppState>,
    Query(query): Query<AuthQuery>,
    headers: HeaderMap,
) -> axum::response::Response {
    let token = match query
        .token
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        })
        .or_else(|| {
            headers
                .get(axum::http::header::COOKIE)
                .and_then(|h| h.to_str().ok())
                .and_then(|cv| {
                    for cookie in cv.split(';') {
                        let cookie = cookie.trim();
                        if let Some(tok) = cookie.strip_prefix("ferris_session=") {
                            return Some(tok.trim().to_string());
                        }
                    }
                    None
                })
        }) {
        Some(t) => t,
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Token topilmadi" })),
            )
                .into_response();
        }
    };

    let cached_user = {
        let sessions = state.sessions.read().unwrap();
        sessions.get(&token).cloned()
    };
    if let Some(user) = cached_user {
        return Json(user).into_response();
    }

    // If starts with ferris_pat_, check personal_access_tokens in database
    if token.starts_with("ferris_pat_") {
        if let Some(ref pool) = state.db {
            let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
            if let Ok(Some(r)) = sqlx::query(
                "SELECT pat.id, pat.user_id, pat.expires_at,
                        u.username, u.display_name, u.provider, u.avatar_url, u.created_at
                 FROM personal_access_tokens pat
                 JOIN users u ON pat.user_id = u.id
                 WHERE pat.token_hash = $1"
            )
            .bind(&token_hash)
            .fetch_optional(pool)
            .await {
                let exp: Option<chrono::DateTime<Utc>> = r.try_get("expires_at").ok();
                if let Some(e) = exp {
                    if Utc::now() > e {
                        return (
                            StatusCode::UNAUTHORIZED,
                            Json(serde_json::json!({ "error": "Token muddati o'tgan" })),
                        )
                            .into_response();
                    }
                }

                let uid: String = r.try_get("user_id").unwrap_or_default();
                let uname: String = r.try_get("username").unwrap_or_default();
                let dname: String = r.try_get("display_name").unwrap_or_default();
                let prov: String = r.try_get("provider").unwrap_or_default();
                let aurl: Option<String> = r.try_get("avatar_url").ok();
                let cat: i64 = r.try_get("created_at").unwrap_or(0);

                let profile = UserProfile {
                    id: uid,
                    provider: prov,
                    username: uname,
                    display_name: dname,
                    avatar_url: aurl,
                    created_at: cat as u64,
                    expires_at: exp,
                };

                {
                    let mut sessions = state.sessions.write().unwrap();
                    sessions.insert(token.clone(), profile.clone());
                }

                let pat_id: String = r.try_get("id").unwrap_or_default();
                let pool = pool.clone();
                tokio::spawn(async move {
                    let _ = sqlx::query("UPDATE personal_access_tokens SET last_used_at = NOW() WHERE id = $1")
                        .bind(&pat_id)
                        .execute(&pool)
                        .await;
                });

                return Json(profile).into_response();
            }
        }
    }

    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({ "error": "Yaroqsiz yoki muddati o'tgan sessiya" })),
    )
        .into_response()
}

async fn auth_logout_handler(
    State(state): State<AppState>,
    Query(query): Query<AuthQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let token = query
        .token
        .or_else(|| {
            headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()))
        })
        .or_else(|| {
            headers
                .get(axum::http::header::COOKIE)
                .and_then(|h| h.to_str().ok())
                .and_then(|cv| {
                    for cookie in cv.split(';') {
                        let cookie = cookie.trim();
                        if let Some(tok) = cookie.strip_prefix("ferris_session=") {
                            return Some(tok.trim().to_string());
                        }
                    }
                    None
                })
        });

    if let Some(token) = token {
        let mut sessions = state.sessions.write().unwrap();
        sessions.remove(&token);
        if let Some(ref pool) = state.db {
            let pool = pool.clone();
            let tok = token.clone();
            tokio::spawn(async move {
                let _ = sqlx::query("DELETE FROM sessions WHERE token = $1")
                    .bind(&tok)
                    .execute(&pool)
                    .await;
            });
        }
    }
    save_db(&state);
    let cookie_val = "ferris_session=; Domain=.ferris.uz; Path=/; Max-Age=0; SameSite=Lax; Secure";
    (
        [(axum::http::header::SET_COOKIE, cookie_val.to_string())],
        Json(serde_json::json!({ "success": true })),
    )
}

// ==============================================================================
// 7.5. UNIVERSAL SSO (FERRIS ID) & DEVICE FLOW (RFC 8628) HANDLERS
// ==============================================================================

#[derive(Deserialize)]
pub struct DeviceCodeReq {
    pub client_id: Option<String>,
}

async fn oauth_device_code_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<DeviceCodeReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client_ip = get_client_ip(&headers);
    let rate_key = format!("device_code_gen:{}", client_ip);
    if !check_rate_limit(&state.rate_limiter, &rate_key, 10, 60) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "too_many_requests",
                "error_description": "Juda ko'p qurilma kodi so'raldi. Iltimos, 1 daqiqa kuting."
            })),
        ));
    }

    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "database_unavailable" })),
            ));
        }
    };

    let client_id = payload.client_id.unwrap_or_else(|| "ferris-cli".to_string());

    // Generate secure device_code (64-char hex)
    let device_code = format!(
        "{:x}",
        Sha256::digest(format!("{}:{}:device", Uuid::new_v4(), Utc::now().timestamp_nanos_opt().unwrap_or(0)).as_bytes())
    );

    // Generate human-friendly user_code: FRIS-XXXX (using non-ambiguous chars)
    let chars = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
    let hash = blake3::hash(Uuid::new_v4().as_bytes());
    let hash_bytes = hash.as_bytes();
    let part: String = (0..4)
        .map(|i| {
            let idx = (hash_bytes[i] as usize) % chars.len();
            chars[idx] as char
        })
        .collect();
    let user_code = format!("FRIS-{}", part);

    let expires_in_secs: i64 = 600; // 10 minutes
    let expires_at = Utc::now() + chrono::Duration::seconds(expires_in_secs);

    let res = sqlx::query(
        "INSERT INTO oauth_device_codes (device_code, user_code, client_id, status, expires_at, created_at)
         VALUES ($1, $2, $3, 'pending', $4, NOW())",
    )
    .bind(&device_code)
    .bind(&user_code)
    .bind(&client_id)
    .bind(expires_at)
    .execute(pool)
    .await;

    if let Err(e) = res {
        warn!("Failed to create device code: {}", e);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": "server_error",
                "error_description": "Device code yaratishda xatolik yuz berdi"
            })),
        ));
    }

    Ok(Json(serde_json::json!({
        "device_code": device_code,
        "user_code": user_code,
        "verification_uri": "https://ferris.uz/device",
        "verification_uri_complete": format!("https://ferris.uz/device?code={}", user_code),
        "expires_in": expires_in_secs,
        "interval": 5
    })))
}

#[derive(Deserialize)]
pub struct DeviceInfoQuery {
    pub code: String,
}

async fn oauth_device_info_handler(
    State(state): State<AppState>,
    Query(query): Query<DeviceInfoQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "database_unavailable" })),
            ));
        }
    };

    let clean_code = query.code.trim().to_uppercase();
    let row = sqlx::query(
        "SELECT dc.client_id, dc.status, dc.expires_at, oc.client_name, oc.allowed_scopes
         FROM oauth_device_codes dc
         LEFT JOIN oauth_clients oc ON dc.client_id = oc.client_id
         WHERE dc.user_code = $1 AND dc.expires_at > NOW()"
    )
    .bind(&clean_code)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    match row {
        Some(r) => {
            let status: String = r.try_get("status").unwrap_or_default();
            let client_id: String = r.try_get("client_id").unwrap_or_default();
            let client_name: String = r.try_get("client_name").unwrap_or_else(|_| client_id.clone());
            let scopes: Vec<String> = r.try_get("allowed_scopes").unwrap_or_default();

            Ok(Json(serde_json::json!({
                "valid": status == "pending",
                "status": status,
                "client_id": client_id,
                "client_name": client_name,
                "scopes": scopes
            })))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "valid": false,
                "error": "Kod topilmadi yoki muddati o'tgan"
            })),
        )),
    }
}

#[derive(Deserialize)]
pub struct DeviceTokenReq {
    pub client_id: Option<String>,
    pub device_code: String,
}

async fn oauth_device_token_handler(
    State(state): State<AppState>,
    Json(payload): Json<DeviceTokenReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "database_unavailable" })),
            ));
        }
    };

    let row = sqlx::query(
        "SELECT device_code, user_code, client_id, user_id, status, expires_at, last_poll_at
         FROM oauth_device_codes WHERE device_code = $1",
    )
    .bind(&payload.device_code)
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        warn!("DB error checking device token: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "server_error" })),
        )
    })?;

    let row = match row {
        Some(r) => r,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "Device code topilmadi yoki eskirgan"
                })),
            ));
        }
    };

    let status: String = row.try_get("status").unwrap_or_else(|_| "pending".to_string());
    let expires_at: chrono::DateTime<Utc> = row.try_get("expires_at").unwrap_or_else(|_| Utc::now());
    let client_id: String = row.try_get("client_id").unwrap_or_else(|_| "ferris-cli".to_string());

    if Utc::now() > expires_at {
        let _ = sqlx::query("DELETE FROM oauth_device_codes WHERE device_code = $1")
            .bind(&payload.device_code)
            .execute(pool)
            .await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "expired_token",
                "error_description": "Tasdiqlash kodi muddati o'tgan. Iltimos qaytadan login qiling."
            })),
        ));
    }

    if status == "denied" {
        let _ = sqlx::query("DELETE FROM oauth_device_codes WHERE device_code = $1")
            .bind(&payload.device_code)
            .execute(pool)
            .await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "access_denied",
                "error_description": "Foydalanuvchi ilovaga ruxsat bermadi"
            })),
        ));
    }

    if status == "pending" {
        // RFC 8628 Section 3.5: Slow Down protection
        let last_poll: Option<chrono::DateTime<Utc>> = row.try_get("last_poll_at").ok().flatten();
        if let Some(lp) = last_poll {
            let diff = Utc::now().signed_duration_since(lp).num_seconds();
            if diff < 3 {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "slow_down",
                        "error_description": "So'rovlar juda tez yuborilmoqda. Kamida 5 soniya kuting.",
                        "interval": 10
                    })),
                ));
            }
        }

        let _ = sqlx::query("UPDATE oauth_device_codes SET last_poll_at = NOW() WHERE device_code = $1")
            .bind(&payload.device_code)
            .execute(pool)
            .await;
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "authorization_pending",
                "error_description": "Foydalanuvchi hali brauzerda tasdiqlamadi"
            })),
        ));
    }

    if status == "approved" {
        let user_id: Option<String> = row.try_get("user_id").ok();
        let user_id = match user_id {
            Some(uid) => uid,
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "server_error",
                        "error_description": "Foydalanuvchi ma'lumoti aniqlanmadi"
                    })),
                ));
            }
        };

        // Fetch user from users table
        let user_row = sqlx::query(
            "SELECT id, username, display_name, provider, avatar_url, created_at FROM users WHERE id = $1",
        )
        .bind(&user_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
        })?;

        let user_row = match user_row {
            Some(u) => u,
            None => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "user_not_found",
                        "error_description": "Foydalanuvchi topilmadi"
                    })),
                ));
            }
        };

        let uname: String = user_row.try_get("username").unwrap_or_default();
        let dname: String = user_row.try_get("display_name").unwrap_or_default();
        let prov: String = user_row.try_get("provider").unwrap_or_default();
        let aurl: Option<String> = user_row.try_get("avatar_url").ok();
        let cat: i64 = user_row.try_get("created_at").unwrap_or(0);

        let user_profile = UserProfile {
            id: user_id.clone(),
            provider: prov,
            username: uname.clone(),
            display_name: dname.clone(),
            avatar_url: aurl.clone(),
            created_at: cat as u64,
            expires_at: None,
        };

        // Generate high-entropy PAT token: ferris_pat_<64_hex>
        let raw_token_hex = format!(
            "{:x}",
            Sha256::digest(format!("{}:{}:pat", Uuid::new_v4(), Utc::now().timestamp_nanos_opt().unwrap_or(0)).as_bytes())
        );
        let access_token = format!("ferris_pat_{}", raw_token_hex);
        let token_hash = format!("{:x}", Sha256::digest(access_token.as_bytes()));
        let pat_id = Uuid::new_v4().to_string();

        let _ = sqlx::query(
            "INSERT INTO personal_access_tokens (id, user_id, client_id, token_hash, token_prefix, name, scopes, created_at)
             VALUES ($1, $2, $3, $4, 'ferris_pat_', $5, '{\"full\"}', NOW())",
        )
        .bind(&pat_id)
        .bind(&user_id)
        .bind(&client_id)
        .bind(&token_hash)
        .bind(format!("{} Login", client_id))
        .execute(pool)
        .await;

        // Persist session into PostgreSQL sessions table so it survives server reboots
        let now_sec = Utc::now().timestamp();
        let _ = sqlx::query(
            "INSERT INTO sessions (token, user_id, created_at) VALUES ($1, $2, $3) ON CONFLICT (token) DO NOTHING",
        )
        .bind(&access_token)
        .bind(&user_id)
        .bind(now_sec)
        .execute(pool)
        .await;

        // Also insert into in-memory sessions cache for instant auth
        {
            let mut sessions = state.sessions.write().unwrap();
            sessions.insert(access_token.clone(), user_profile.clone());
        }

        // Clean up the device code
        let _ = sqlx::query("DELETE FROM oauth_device_codes WHERE device_code = $1")
            .bind(&payload.device_code)
            .execute(pool)
            .await;

        info!("🔑 Device flow completed successfully for @{} (client: {})", uname, client_id);

        return Ok(Json(serde_json::json!({
            "access_token": access_token,
            "token_type": "Bearer",
            "expires_in": null,
            "user": {
                "id": user_id,
                "username": uname,
                "display_name": dname,
                "avatar_url": aurl
            }
        })));
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "invalid_grant" })),
    ))
}

#[derive(Deserialize)]
pub struct DeviceApproveReq {
    pub user_code: String,
}

async fn oauth_device_approve_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<DeviceApproveReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client_ip = get_client_ip(&headers);
    let rate_key = format!("device_approve:{}", client_ip);
    if !check_rate_limit(&state.rate_limiter, &rate_key, 6, 60) {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": "too_many_requests",
                "message": "Juda ko'p xato urinishlar qilindi. Iltimos, 1 daqiqa kuting!"
            })),
        ));
    }

    let user = get_authenticated_user(&headers, &state).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "unauthorized",
                "message": "Tasdiqlash uchun tizimga kirish shart"
            })),
        )
    })?;

    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "database_unavailable" })),
            ));
        }
    };

    let clean_code = payload.user_code.trim().to_uppercase();

    let res = sqlx::query(
        "UPDATE oauth_device_codes 
         SET status = 'approved', user_id = $1 
         WHERE user_code = $2 AND status = 'pending' AND expires_at > NOW()",
    )
    .bind(&user.id)
    .bind(&clean_code)
    .execute(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    if res.rows_affected() == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "invalid_code",
                "message": "Kod topilmadi, muddati o'tgan yoki allaqachon ishlatilgan"
            })),
        ));
    }

    info!("✅ Device code {} approved by @{}", clean_code, user.username);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Ilova muvaffaqiyatli tasdiqlandi! Endi terminalingizga qaytishingiz mumkin."
    })))
}

async fn oauth_device_deny_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<DeviceApproveReq>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&headers, &state);

    if let Some(ref pool) = state.db {
        let clean_code = payload.user_code.trim().to_uppercase();
        let _ = sqlx::query(
            "UPDATE oauth_device_codes SET status = 'denied' WHERE user_code = $1 AND status = 'pending'",
        )
        .bind(&clean_code)
        .execute(pool)
        .await;
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Ilovaga kirish rad etildi."
    })))
}

async fn oauth_verify_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer ").map(|s| s.trim().to_string()));

    let token = match auth_header {
        Some(t) => t,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "valid": false,
                    "error": "Authorization Bearer token taqdim etilmadi"
                })),
            ));
        }
    };

    // First check memory sessions
    {
        let sessions = state.sessions.read().unwrap();
        if let Some(user) = sessions.get(&token) {
            return Ok(Json(serde_json::json!({
                "valid": true,
                "user": user,
                "token_type": if token.starts_with("ferris_pat_") { "pat" } else { "session" },
                "scopes": ["full"]
            })));
        }
    }

    // If starts with ferris_pat_, check personal_access_tokens in DB
    if token.starts_with("ferris_pat_") {
        if let Some(ref pool) = state.db {
            let token_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
            let row = sqlx::query(
                "SELECT pat.id, pat.user_id, pat.client_id, pat.scopes, pat.expires_at,
                        u.username, u.display_name, u.provider, u.avatar_url, u.created_at
                 FROM personal_access_tokens pat
                 JOIN users u ON pat.user_id = u.id
                 WHERE pat.token_hash = $1",
            )
            .bind(&token_hash)
            .fetch_optional(pool)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
            })?;

            if let Some(r) = row {
                let exp: Option<chrono::DateTime<Utc>> = r.try_get("expires_at").ok();
                if let Some(e) = exp {
                    if Utc::now() > e {
                        return Err((
                            StatusCode::UNAUTHORIZED,
                            Json(serde_json::json!({
                                "valid": false,
                                "error": "Token muddati o'tgan"
                            })),
                        ));
                    }
                }

                let uid: String = r.try_get("user_id").unwrap_or_default();
                let uname: String = r.try_get("username").unwrap_or_default();
                let dname: String = r.try_get("display_name").unwrap_or_default();
                let prov: String = r.try_get("provider").unwrap_or_default();
                let aurl: Option<String> = r.try_get("avatar_url").ok();
                let cat: i64 = r.try_get("created_at").unwrap_or(0);
                let client_id: Option<String> = r.try_get("client_id").ok();
                let scopes: Vec<String> = r.try_get("scopes").unwrap_or_default();

                let user_profile = UserProfile {
                    id: uid,
                    provider: prov,
                    username: uname,
                    display_name: dname,
                    avatar_url: aurl,
                    created_at: cat as u64,
                    expires_at: exp,
                };

                // Cache into memory
                {
                    let mut sessions = state.sessions.write().unwrap();
                    sessions.insert(token.clone(), user_profile.clone());
                }

                // Update last_used_at asynchronously
                let pool = pool.clone();
                let pat_id: String = r.try_get("id").unwrap_or_default();
                tokio::spawn(async move {
                    let _ = sqlx::query("UPDATE personal_access_tokens SET last_used_at = NOW() WHERE id = $1")
                        .bind(&pat_id)
                        .execute(&pool)
                        .await;
                });

                return Ok(Json(serde_json::json!({
                    "valid": true,
                    "user": user_profile,
                    "client_id": client_id,
                    "token_type": "pat",
                    "scopes": scopes
                })));
            }
        }
    }

    Err((
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({
            "valid": false,
            "error": "Yaroqsiz yoki mavjud bo'lmagan token"
        })),
    ))
}

async fn oauth_tokens_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = get_authenticated_user(&headers, &state).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tish shart" })),
        )
    })?;

    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Baza mavjud emas" })),
            ));
        }
    };

    let rows = sqlx::query(
        "SELECT pat.id, pat.client_id, COALESCE(oc.client_name, pat.client_id) as client_name,
                pat.name, pat.created_at, pat.last_used_at, pat.scopes
         FROM personal_access_tokens pat
         LEFT JOIN oauth_clients oc ON pat.client_id = oc.client_id
         WHERE pat.user_id = $1
         ORDER BY pat.created_at DESC",
    )
    .bind(&auth_user.id)
    .fetch_all(pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
    })?;

    let mut tokens = Vec::new();
    for r in rows {
        let tid: String = r.try_get("id").unwrap_or_default();
        let cid: String = r.try_get("client_id").unwrap_or_default();
        let cname: String = r.try_get("client_name").unwrap_or_default();
        let name: String = r.try_get("name").unwrap_or_default();
        let cat: chrono::DateTime<Utc> = r.try_get("created_at").unwrap_or_else(|_| Utc::now());
        let lat: Option<chrono::DateTime<Utc>> = r.try_get("last_used_at").ok().flatten();
        let scopes: Vec<String> = r.try_get("scopes").unwrap_or_default();

        tokens.push(ActiveTokenItem {
            id: tid,
            client_id: cid,
            client_name: cname,
            name,
            created_at: cat.format("%Y-%m-%d %H:%M").to_string(),
            last_used_at: lat.map(|t| t.format("%Y-%m-%d %H:%M").to_string()),
            scopes,
        });
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "tokens": tokens
    })))
}

async fn oauth_token_revoke_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(token_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = get_authenticated_user(&headers, &state).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tish shart" })),
        )
    })?;

    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Baza mavjud emas" })),
            ));
        }
    };

    let row = sqlx::query(
        "SELECT token_hash FROM personal_access_tokens WHERE id = $1 AND user_id = $2"
    )
    .bind(&token_id)
    .bind(&auth_user.id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    let token_hash: Option<String> = row.and_then(|r| r.try_get("token_hash").ok());

    let res = sqlx::query("DELETE FROM personal_access_tokens WHERE id = $1 AND user_id = $2")
        .bind(&token_id)
        .bind(&auth_user.id)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    if res.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Token topilmadi yoki sizga tegishli emas" })),
        ));
    }

    // Also purge from in-memory cache and sessions table
    {
        let mut sessions = state.sessions.write().unwrap();
        sessions.retain(|token, user| {
            if user.id == auth_user.id && token.starts_with("ferris_pat_") {
                if let Some(ref th) = token_hash {
                    let h = format!("{:x}", Sha256::digest(token.as_bytes()));
                    return &h != th;
                }
            }
            true
        });
    }

    info!("🗑️ PAT token {} revoked for @{}", token_id, auth_user.username);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Token muvaffaqiyatli bekor qilindi (Revoked)"
    })))
}

async fn oauth_token_revoke_all_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = get_authenticated_user(&headers, &state).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tish shart" })),
        )
    })?;

    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Baza mavjud emas" })),
            ));
        }
    };

    let _ = sqlx::query("DELETE FROM personal_access_tokens WHERE user_id = $1")
        .bind(&auth_user.id)
        .execute(pool)
        .await;

    // Purge all PAT tokens for this user from memory
    {
        let mut sessions = state.sessions.write().unwrap();
        sessions.retain(|token, user| {
            !(user.id == auth_user.id && token.starts_with("ferris_pat_"))
        });
    }

    info!("🗑️ All PAT tokens revoked for @{}", auth_user.username);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Barcha boshqa qurilma tokenlari bekor qilindi"
    })))
}

async fn auth_identity_unlink_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(provider): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = get_authenticated_user(&headers, &state).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tish shart" })),
        )
    })?;

    let pool = match state.db.as_ref() {
        Some(p) => p,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Baza mavjud emas" })),
            ));
        }
    };

    let clean_provider = provider.to_lowercase();
    if clean_provider != "github" && clean_provider != "telegram" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Noma'lum provayder" })),
        ));
    }

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_identities WHERE user_id = $1")
        .bind(&auth_user.id)
        .fetch_one(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    if count <= 1 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Akkauntdan oxirgi kirish provayderini uzib bo'lmaydi. Kamida bitta autentifikatsiya usuli qolishi shart!"
            })),
        ));
    }

    let res = sqlx::query("DELETE FROM user_identities WHERE user_id = $1 AND provider = $2")
        .bind(&auth_user.id)
        .bind(&clean_provider)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    if res.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "Ushbu provayder hisobingizga ulanmagan" })),
        ));
    }

    info!("🔓 Provider {} unlinked for @{}", clean_provider, auth_user.username);

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("{} hisobi muvaffaqiyatli uzildi", clean_provider)
    })))
}

// ==============================================================================
// 8. SHAXSIY XABARLAR (DIRECT MESSAGES) HANDLERS
// ==============================================================================
#[derive(Deserialize)]
struct SendDmPayload {
    text: String,
}

async fn dm_conversations_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tilmagan" })))),
    };

    if let Some(ref pool) = state.db {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT
                CASE WHEN sender_username = $1 THEN receiver_username ELSE sender_username END as peer_username,
                text, timestamp
            FROM direct_messages
            WHERE sender_username = $1 OR receiver_username = $1
            ORDER BY timestamp DESC
            "#
        )
        .bind(&auth_user.username)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let mut convos = Vec::new();
        let mut seen = HashSet::new();
        for r in rows {
            let peer: String = r.try_get("peer_username").unwrap_or_default();
            if seen.insert(peer.clone()) {
                let text: String = r.try_get("text").unwrap_or_default();
                let timestamp: i64 = r.try_get("timestamp").unwrap_or(0);
                convos.push(serde_json::json!({
                    "username": peer,
                    "last_message": text,
                    "timestamp": timestamp,
                }));
            }
        }
        return Ok(Json(serde_json::json!({ "conversations": convos })));
    }

    Ok(Json(serde_json::json!({ "conversations": [] })))
}

async fn dm_messages_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_user): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tilmagan" })))),
    };
    let clean_target = target_user.trim_start_matches('@');

    if let Some(ref pool) = state.db {
        let rows = sqlx::query(
            r#"
            SELECT id, sender_username, receiver_username, text, timestamp, is_read, msg_hash
            FROM direct_messages
            WHERE (sender_username = $1 AND receiver_username = $2)
               OR (sender_username = $2 AND receiver_username = $1)
            ORDER BY timestamp ASC
            "#
        )
        .bind(&auth_user.username)
        .bind(clean_target)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let messages: Vec<serde_json::Value> = rows.into_iter().map(|r| {
            let id: Uuid = r.try_get("id").unwrap_or_default();
            let sender: String = r.try_get("sender_username").unwrap_or_default();
            let receiver: String = r.try_get("receiver_username").unwrap_or_default();
            let text: String = r.try_get("text").unwrap_or_default();
            let timestamp: i64 = r.try_get("timestamp").unwrap_or(0);
            let msg_hash: String = r.try_get("msg_hash").ok().flatten().unwrap_or_else(|| {
                compute_dm_hash(&id.to_string(), &sender, &receiver, &text, timestamp as u64)
            });
            serde_json::json!({
                "id": id,
                "sender": sender,
                "receiver": receiver,
                "text": text,
                "timestamp": timestamp,
                "msg_hash": msg_hash,
            })
        }).collect();

        let is_blocked_by_me = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM blocked_users WHERE blocker_username = $1 AND blocked_username = $2)"
        )
        .bind(&auth_user.username)
        .bind(clean_target)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        let is_blocked_by_them = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM blocked_users WHERE blocker_username = $1 AND blocked_username = $2)"
        )
        .bind(clean_target)
        .bind(&auth_user.username)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        return Ok(Json(serde_json::json!({
            "target": clean_target,
            "is_blocked_by_me": is_blocked_by_me,
            "is_blocked_by_them": is_blocked_by_them,
            "messages": messages
        })));
    }

    Ok(Json(serde_json::json!({ "target": clean_target, "is_blocked_by_me": false, "is_blocked_by_them": false, "messages": [] })))
}

async fn dm_send_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_user): Path<String>,
    Json(payload): Json<SendDmPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tilmagan" })))),
    };
    let clean_target = target_user.trim_start_matches('@').to_string();

    let text_trimmed = payload.text.trim();
    if text_trimmed.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Xabar matni bo'sh bo'lishi mumkin emas" }))));
    }
    if text_trimmed.chars().count() > 2000 {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Shaxsiy xabar 2,000 belgidan oshmasligi kerak!" }))));
    }

    let client_ip = get_client_ip(&headers);
    let rate_key = format!("dm:{}:{}", client_ip, auth_user.username);
    if !check_rate_limit(&state.rate_limiter, &rate_key, 15, 10) {
        return Err((StatusCode::TOO_MANY_REQUESTS, Json(serde_json::json!({ "error": "Juda ko'p shaxsiy xabar yuborildi. Iltimos, bir oz kuting (Rate Limit)!" }))));
    }

    // Bloklanganlik tekshiruvi
    if let Some(ref pool) = state.db {
        let is_blocked = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM blocked_users WHERE blocker_username = $1 AND blocked_username = $2)"
        )
        .bind(&clean_target)
        .bind(&auth_user.username)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if is_blocked {
            return Err((StatusCode::FORBIDDEN, Json(serde_json::json!({ "error": "Ushbu foydalanuvchi sizni bloklagan, unga xabar yubora olmaysiz." }))));
        }

        let has_blocked = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM blocked_users WHERE blocker_username = $1 AND blocked_username = $2)"
        )
        .bind(&auth_user.username)
        .bind(&clean_target)
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if has_blocked {
            return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "Siz bu foydalanuvchini bloklagansiz. Xabar yozish uchun avval blokdan chiqaring." }))));
        }
    }

    let id = Uuid::new_v4();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let dm_hash = compute_dm_hash(&id.to_string(), &auth_user.username, &clean_target, &payload.text, now);

    if let Some(ref pool) = state.db {
        if let Err(e) = sqlx::query(
            "INSERT INTO direct_messages (id, sender_username, receiver_username, text, timestamp, is_read, msg_hash) VALUES ($1, $2, $3, $4, $5, FALSE, $6)"
        )
        .bind(id)
        .bind(&auth_user.username)
        .bind(&clean_target)
        .bind(&payload.text)
        .bind(now as i64)
        .bind(&dm_hash)
        .execute(pool)
        .await {
            tracing::error!("PostgreSQL INSERT direct_messages error: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": "Xabarni bazaga yozishda xatolik yuz berdi" }))));
        }
    }

    // Targeted routing of WsEvent::DirectMessage strictly to sender and receiver with PQC integrity hash
    let dm_event = WsEvent::DirectMessage {
        id: id.to_string(),
        sender_username: auth_user.username.clone(),
        receiver_username: clean_target.clone(),
        text: payload.text.clone(),
        timestamp: now,
        msg_hash: Some(dm_hash),
    };

    {
        let mut user_ws = state.user_ws.write().unwrap();
        // Route to receiver if online
        if let Some(senders) = user_ws.get_mut(&clean_target) {
            senders.retain(|s| s.send(dm_event.clone()).is_ok());
        }
        // Route to sender (all tabs)
        if let Some(senders) = user_ws.get_mut(&auth_user.username) {
            senders.retain(|s| s.send(dm_event.clone()).is_ok());
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "id": id,
        "sender": auth_user.username,
        "receiver": clean_target,
        "text": payload.text,
        "timestamp": now
    })))
}

async fn dm_message_delete_handler(
    State(state): State<AppState>,
    Path(id_str): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tilmagan" })))),
    };

    if let Some(ref pool) = state.db {
        let rows_affected = if let Ok(u) = Uuid::parse_str(id_str.trim()) {
            sqlx::query(
                "DELETE FROM direct_messages WHERE id = $1 AND (sender_username = $2 OR receiver_username = $2)"
            )
            .bind(u)
            .bind(&auth_user.username)
            .execute(pool)
            .await
            .map(|r| r.rows_affected())
            .unwrap_or(0)
        } else {
            let prefix = format!("{}%", id_str.trim());
            sqlx::query(
                "DELETE FROM direct_messages WHERE id::text LIKE $1 AND (sender_username = $2 OR receiver_username = $2)"
            )
            .bind(prefix)
            .bind(&auth_user.username)
            .execute(pool)
            .await
            .map(|r| r.rows_affected())
            .unwrap_or(0)
        };

        if rows_affected > 0 {
            info!("🗑️ DM xabar o'chirildi: {} by @{}", id_str, auth_user.username);
            return Ok(Json(serde_json::json!({ "success": true, "deleted_message_id": id_str })));
        } else {
            return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "Xabar topilmadi yoki o'chirishga ruxsat yo'q" }))));
        }
    }

    Ok(Json(serde_json::json!({ "success": true, "deleted_message_id": id_str })))
}

async fn user_block_handler(
    State(state): State<AppState>,
    Path(target_user): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tilmagan" })))),
    };
    let clean_target = target_user.trim_start_matches('@');

    if clean_target == auth_user.username {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "O'zingizni bloklay olmaysiz!" }))));
    }

    if let Some(ref pool) = state.db {
        let _ = sqlx::query(
            "INSERT INTO blocked_users (blocker_username, blocked_username) VALUES ($1, $2) ON CONFLICT DO NOTHING"
        )
        .bind(&auth_user.username)
        .bind(clean_target)
        .execute(pool)
        .await;
    }

    info!("🚫 @{} foydalanuvchisi @{} ni blokladi", auth_user.username, clean_target);
    Ok(Json(serde_json::json!({ "success": true, "blocked": clean_target })))
}

async fn user_unblock_handler(
    State(state): State<AppState>,
    Path(target_user): Path<String>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tilmagan" })))),
    };
    let clean_target = target_user.trim_start_matches('@');

    if let Some(ref pool) = state.db {
        let _ = sqlx::query(
            "DELETE FROM blocked_users WHERE blocker_username = $1 AND blocked_username = $2"
        )
        .bind(&auth_user.username)
        .bind(clean_target)
        .execute(pool)
        .await;
    }

    info!("✅ @{} foydalanuvchisi @{} ni blokdan chiqardi", auth_user.username, clean_target);
    Ok(Json(serde_json::json!({ "success": true, "unblocked": clean_target })))
}

async fn blocked_users_list_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_user = match get_authenticated_user(&headers, &state) {
        Some(u) => u,
        None => return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "Avtorizatsiyadan o'tilmagan" })))),
    };

    if let Some(ref pool) = state.db {
        let rows = sqlx::query(
            "SELECT blocked_username, created_at FROM blocked_users WHERE blocker_username = $1 ORDER BY created_at DESC"
        )
        .bind(&auth_user.username)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let list: Vec<String> = rows.into_iter().map(|r| {
            r.try_get("blocked_username").unwrap_or_default()
        }).collect();

        return Ok(Json(serde_json::json!({ "blocked_users": list })));
    }

    Ok(Json(serde_json::json!({ "blocked_users": [] })))
}

// ==============================================================================
// 9. WEBSOCKET REAL-TIME CHAT & NOTIFICATIONS
// ==============================================================================
#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

async fn ws_community_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    headers: HeaderMap,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let mut auth_user = get_authenticated_user(&headers, &state);
    if auth_user.is_none() {
        if let Some(tok) = query.token {
            let now = Utc::now();
            let sessions = state.sessions.read().unwrap();
            if let Some(user) = sessions.get(tok.trim()) {
                let valid = user.expires_at.map(|exp| now <= exp).unwrap_or(true);
                if valid {
                    auth_user = Some(user.clone());
                }
            }
        }
    }

    ws.on_upgrade(move |socket| handle_socket(socket, state, auth_user))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, auth_user: Option<UserProfile>) {
    {
        let mut count = state.online_count.write().unwrap();
        *count += 1;
    }

    let (user_tx, mut user_rx) = tokio::sync::mpsc::unbounded_channel::<WsEvent>();
    let auth_username = auth_user.as_ref().map(|u| u.username.clone());

    if let Some(ref uname) = auth_username {
        let mut user_ws = state.user_ws.write().unwrap();
        user_ws.entry(uname.clone()).or_default().push(user_tx);
    }

    let mut rx = state.chat_tx.subscribe();

    let history_items: Vec<ChatMessage> = {
        let history = state.chat_history.read().unwrap();
        history.iter().rev().take(30).rev().cloned().collect()
    };

    for msg in history_items {
        let event = WsEvent::CommunityChat(msg);
        if let Ok(json_str) = serde_json::to_string(&event) {
            let _ = socket.send(Message::Text(json_str)).await;
        }
    }

    let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
    ping_interval.reset();

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
            Ok(event) = rx.recv() => {
                if let Ok(json_str) = serde_json::to_string(&event) {
                    if socket.send(Message::Text(json_str)).await.is_err() {
                        break;
                    }
                }
            }
            Some(event) = user_rx.recv() => {
                if let Ok(json_str) = serde_json::to_string(&event) {
                    if socket.send(Message::Text(json_str)).await.is_err() {
                        break;
                    }
                }
            }
            incoming_res = socket.recv() => {
                match incoming_res {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(incoming) = serde_json::from_str::<serde_json::Value>(&text) {
                            let sender_opt = auth_user.clone().or_else(|| {
                                let token = incoming.get("token").and_then(|t| t.as_str());
                                token.and_then(|t| {
                                    let sessions = state.sessions.read().unwrap();
                                    sessions.get(t).cloned()
                                })
                            });

                            if let Some(user) = sender_opt {
                                if let Some(content) = incoming.get("text").and_then(|c| c.as_str()) {
                                    if !content.trim().is_empty() {
                                        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                                        let room_name = incoming.get("room").and_then(|r| r.as_str()).unwrap_or("general").to_string();
                                        let msg_id = incoming.get("id").and_then(|i| i.as_str()).map(|s| s.to_string()).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                                        let prev_hash = {
                                            let hist = state.chat_history.read().unwrap();
                                            hist.iter().rev().find(|m| m.room == room_name).and_then(|m| m.msg_hash.clone())
                                        };

                                        let msg_hash = compute_chat_hash(&msg_id, &room_name, &user.username, content, now, prev_hash.as_deref());

                                        let chat_msg = ChatMessage {
                                            id: msg_id,
                                            room: room_name,
                                            user,
                                            text: content.to_string(),
                                            timestamp: now,
                                            prev_hash: prev_hash.clone(),
                                            msg_hash: Some(msg_hash.clone()),
                                        };

                                        {
                                            let mut hist = state.chat_history.write().unwrap();
                                            hist.push(chat_msg.clone());
                                            if hist.len() > 200 {
                                                hist.remove(0);
                                            }
                                        }

                                        save_db(&state);

                                        if let Some(ref pool) = state.db {
                                            let pool = pool.clone();
                                            let id = Uuid::parse_str(&chat_msg.id).unwrap_or_else(|_| Uuid::new_v4());
                                            let room = chat_msg.room.clone();
                                            let uname = chat_msg.user.username.clone();
                                            let avatar = chat_msg.user.avatar_url.clone();
                                            let text = chat_msg.text.clone();
                                            let ts = chat_msg.timestamp as i64;
                                            let m_hash = msg_hash.clone();
                                            let p_hash = prev_hash.clone();
                                            tokio::spawn(async move {
                                                let _ = sqlx::query(
                                                    "INSERT INTO chat_messages (id, room, author_username, author_avatar, text, timestamp, msg_hash, prev_hash) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
                                                )
                                                .bind(id)
                                                .bind(room)
                                                .bind(uname)
                                                .bind(avatar)
                                                .bind(text)
                                                .bind(ts)
                                                .bind(m_hash)
                                                .bind(p_hash)
                                                .execute(&pool)
                                                .await;
                                            });
                                        }

                                        let _ = state.chat_tx.send(WsEvent::CommunityChat(chat_msg));
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) => break,
                    Some(Err(_)) | None => break,
                    _ => {}
                }
            }
        }
    }

    {
        let mut count = state.online_count.write().unwrap();
        *count = count.saturating_sub(1);
    }

    if let Some(ref uname) = auth_username {
        let mut user_ws = state.user_ws.write().unwrap();
        if let Some(senders) = user_ws.get_mut(uname) {
            senders.retain(|s| !s.is_closed());
            if senders.is_empty() {
                user_ws.remove(uname);
            }
        }
    }
}
