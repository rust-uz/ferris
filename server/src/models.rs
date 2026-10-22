use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 1. Foydalanuvchi modeli (User)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: Option<String>,
    pub display_name: String,
    pub provider: String, // "telegram" | "github" | "local"
    pub provider_id: Option<String>,
    pub telegram_username: Option<String>,
    pub github_username: Option<String>,
    pub avatar_url: String,
    pub interests: Vec<String>,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 2. Dasturchi portfeli loyihasi (UserProject)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UserProject {
    pub id: Uuid,
    pub user_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub project_url: Option<String>,
    pub github_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 3. Jamiyat (Community / Guruh) modeli
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Jamiyat {
    pub id: Uuid,
    pub slug: String, // groups.ferris.uz/f/slug
    pub name: String,
    pub description: Option<String>,
    pub avatar_url: String,
    pub owner_id: Option<Uuid>,
    pub members_count: i32,
    pub empty_since: Option<DateTime<Utc>>, // 24h TTL uchun
    pub is_bot_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 4. Jamiyat a'zosi roli
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MemberRole {
    Owner,
    Admin,
    Member,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JamiyatMember {
    pub jamiyat_id: Uuid,
    pub user_id: Uuid,
    pub role: MemberRole,
    pub joined_at: DateTime<Utc>,
}

/// 5. Jamiyatning ochiq loyihalari (JamiyatProject)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JamiyatProject {
    pub id: Uuid,
    pub jamiyat_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub project_url: Option<String>,
    pub github_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 6. Kategoriya / Mavzu (Topic — topics.ferris.uz/f/mavzu)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Topic {
    pub id: Uuid,
    pub slug: String, // "f/rust", "f/async"
    pub title: String,
    pub description: Option<String>,
    pub icon: String,
    pub created_at: DateTime<Utc>,
}

/// 7. Post / Muhokama (Post)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Post {
    pub id: Uuid,
    pub topic_id: Option<Uuid>,
    pub jamiyat_id: Option<Uuid>,
    pub author_id: Uuid,
    pub author_name: Option<String>,
    pub author_avatar: Option<String>,
    pub title: String,
    pub content: String,
    pub views_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 8. Jonli chat xabarlari (ChatMessage)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessageModel {
    pub id: Uuid,
    pub jamiyat_id: Uuid,
    pub user_id: Uuid,
    pub text: String,
    pub msg_hash: Option<String>,
    pub image_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// 9. CLI va AdminCLI tokenlari
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub name: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}


/// 10. Muhokama postlariga javoblar / sharhlar (Comment)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Comment {
    pub id: Uuid,
    pub post_id: Uuid,
    pub author_name: String,
    pub author_avatar: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// 11. Shaxsiy xabarlar (DirectMessage)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectMessage {
    pub id: Uuid,
    pub sender_username: String,
    pub receiver_username: String,
    pub text: String,
    pub is_read: bool,
    pub created_at: DateTime<Utc>,
}
