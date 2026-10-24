-- =============================================================================
-- FERRIS.UZ — Database Architecture Schema (PostgreSQL)
-- Moslik: Cloudflare Universal SSL (1-darajali subdomenlar), CLI & Web
-- =============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- 1. USERS — Foydalanuvchilar jadvali
CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    username VARCHAR(50) UNIQUE,                          -- profile.ferris.uz/@username
    display_name VARCHAR(100) NOT NULL,                   -- Ism + Familiya
    provider VARCHAR(20) NOT NULL,                        -- 'telegram' | 'github' | 'local'
    provider_id VARCHAR(100),                             -- OAuth ID
    telegram_username VARCHAR(100),
    github_username VARCHAR(100),
    avatar_url TEXT DEFAULT '/static/default_avatar.png', -- default image
    interests TEXT[] DEFAULT '{}',                        -- qiziqishlar / dasturlash yo'nalishlari
    is_admin BOOLEAN DEFAULT FALSE,                       -- admincli uchun
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 2. USER PROJECTS — Foydalanuvchi loyihalari (Portfolio)
CREATE TABLE IF NOT EXISTS user_projects (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(150) NOT NULL,
    description TEXT,
    project_url TEXT,
    github_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 3. JAMIYATLAR (Communities) — Guruhlar jadvali
CREATE TABLE IF NOT EXISTS jamiyatlar (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    slug VARCHAR(50) UNIQUE NOT NULL,                     -- unique name: groups.ferris.uz/f/slug
    name VARCHAR(100) NOT NULL,                           -- Jamiyat nomi
    description TEXT,                                     -- Tavsifi
    avatar_url TEXT DEFAULT '/static/default_group.png',  -- default image
    owner_id UUID REFERENCES users(id) ON DELETE SET NULL,-- admin / owner
    members_count INT NOT NULL DEFAULT 1,                 -- A'zolar soni
    empty_since TIMESTAMPTZ,                              -- A'zosi 0 bo'lgan payt (24h TTL uchun)
    is_bot_enabled BOOLEAN NOT NULL DEFAULT FALSE,        -- user bot
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 4. JAMIYAT MEMBERS — Jamiyat a'zolari (Many-to-Many)
CREATE TABLE IF NOT EXISTS jamiyat_members (
    jamiyat_id UUID NOT NULL REFERENCES jamiyatlar(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role VARCHAR(20) NOT NULL DEFAULT 'member',           -- 'owner' | 'admin' | 'member'
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (jamiyat_id, user_id)
);

-- 5. JAMIYAT PROJECTS — Jamiyat ochiq loyihalari
CREATE TABLE IF NOT EXISTS jamiyat_projects (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    jamiyat_id UUID NOT NULL REFERENCES jamiyatlar(id) ON DELETE CASCADE,
    title VARCHAR(150) NOT NULL,
    description TEXT,
    project_url TEXT,
    github_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 6. TOPICS — Mavzular va Kategoriyalar (topics.ferris.uz/f/mavzu)
CREATE TABLE IF NOT EXISTS topics (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    slug VARCHAR(50) UNIQUE NOT NULL,                     -- 'f/rust', 'f/async', 'f/systems'
    title VARCHAR(100) NOT NULL,                          -- Mavzu nomi
    description TEXT,
    icon VARCHAR(50) DEFAULT 'code',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 7. POSTS — Mavzular yoki Jamiyatlar ichidagi muhokama postlari
CREATE TABLE IF NOT EXISTS posts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    topic_id UUID REFERENCES topics(id) ON DELETE SET NULL,
    jamiyat_id UUID REFERENCES jamiyatlar(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,                                -- Markdown format
    views_count INT DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 8. CHAT MESSAGES — Jamiyat chat xabarlari (WebSockets & Hash)
CREATE TABLE IF NOT EXISTS chat_messages (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    jamiyat_id UUID NOT NULL REFERENCES jamiyatlar(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    text TEXT NOT NULL,
    msg_hash VARCHAR(64),                                 -- Xabarlar xeshi (sha256)
    image_url TEXT,                                       -- Ixtiyoriy rasm
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 9. CLI TOKENS — ferris-cli va admincli autentifikatsiyasi uchun
CREATE TABLE IF NOT EXISTS api_tokens (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(50) DEFAULT 'ferris-cli',
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- =============================================================================
-- AVTOMATIK HISOBLAGICH VA 24 SOATLIK TOZALASH (TRIGGERS & PROCEDURES)
-- =============================================================================

-- A'zolar soni o'zgarganda 'members_count' va 'empty_since' ni yangilovchi funksiya:
CREATE OR REPLACE FUNCTION update_jamiyat_members_stats()
RETURNS TRIGGER AS $$
DECLARE
    cur_count INT;
    t_jamiyat_id UUID;
BEGIN
    IF TG_OP = 'DELETE' THEN
        t_jamiyat_id := OLD.jamiyat_id;
    ELSE
        t_jamiyat_id := NEW.jamiyat_id;
    END IF;

    SELECT COUNT(*) INTO cur_count FROM jamiyat_members WHERE jamiyat_id = t_jamiyat_id;

    IF cur_count = 0 THEN
        UPDATE jamiyatlar 
        SET members_count = 0, empty_since = NOW(), updated_at = NOW() 
        WHERE id = t_jamiyat_id;
    ELSE
        UPDATE jamiyatlar 
        SET members_count = cur_count, empty_since = NULL, updated_at = NOW() 
        WHERE id = t_jamiyat_id;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER trg_jamiyat_members_stats
AFTER INSERT OR DELETE ON jamiyat_members
FOR EACH ROW EXECUTE FUNCTION update_jamiyat_members_stats();

-- 24 soat kimsasiz qolgan jamiyatlarni o'chirish uchun SQL buyrug'i (Cron/Worker orqali chaqiriladi):
-- DELETE FROM jamiyatlar WHERE members_count = 0 AND empty_since < NOW() - INTERVAL '24 HOURS';

-- =============================================================================
-- INDEXLAR (Tezkor qidiruv va routing uchun)
-- =============================================================================
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_lower ON users (LOWER(username));
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_jamiyatlar_slug ON jamiyatlar(slug);
CREATE INDEX IF NOT EXISTS idx_topics_slug ON topics(slug);
CREATE INDEX IF NOT EXISTS idx_posts_topic_id ON posts(topic_id);
CREATE INDEX IF NOT EXISTS idx_posts_jamiyat_id ON posts(jamiyat_id);
CREATE INDEX IF NOT EXISTS idx_chat_messages_jamiyat_id ON chat_messages(jamiyat_id, created_at);
CREATE INDEX IF NOT EXISTS idx_jamiyatlar_empty_cleanup ON jamiyatlar(members_count, empty_since) WHERE members_count = 0;

-- =============================================================================
-- 10. MULTI-PROVIDER USER IDENTITIES (Yagona Shaxs / Unified Identity)
-- =============================================================================
CREATE TABLE IF NOT EXISTS user_identities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id VARCHAR(64) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider VARCHAR(30) NOT NULL,
    provider_id VARCHAR(100) NOT NULL,
    provider_username VARCHAR(100),
    avatar_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(provider, provider_id)
);
CREATE INDEX IF NOT EXISTS idx_user_identities_lookup ON user_identities(provider, provider_id);
CREATE INDEX IF NOT EXISTS idx_user_identities_user_id ON user_identities(user_id);

-- =============================================================================
-- 11. UNIVERSAL SSO & DEVICE FLOW (RFC 8628) & PAT
-- =============================================================================
CREATE TABLE IF NOT EXISTS oauth_clients (
    client_id VARCHAR(50) PRIMARY KEY,
    client_name VARCHAR(100) NOT NULL,
    allowed_scopes TEXT[] DEFAULT '{"read:profile"}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO oauth_clients (client_id, client_name, allowed_scopes)
VALUES 
    ('ferris-cli', 'Ferris CLI', '{"read:profile", "write:posts", "write:groups"}'),
    ('hikaku-cli', 'Hikaku CLI', '{"read:profile", "hikaku:deploy"}')
ON CONFLICT (client_id) DO NOTHING;

CREATE TABLE IF NOT EXISTS oauth_device_codes (
    device_code VARCHAR(64) PRIMARY KEY,
    user_code VARCHAR(16) UNIQUE NOT NULL,
    client_id VARCHAR(50) NOT NULL REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
    user_id VARCHAR(64) REFERENCES users(id) ON DELETE CASCADE,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    expires_at TIMESTAMPTZ NOT NULL,
    last_poll_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_device_user_code ON oauth_device_codes(user_code);

CREATE TABLE IF NOT EXISTS personal_access_tokens (
    id VARCHAR(64) PRIMARY KEY,
    user_id VARCHAR(64) NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id VARCHAR(50) DEFAULT 'ferris-cli',
    token_hash VARCHAR(64) UNIQUE NOT NULL,
    token_prefix VARCHAR(16) NOT NULL,
    name VARCHAR(100) NOT NULL DEFAULT 'CLI Token',
    scopes TEXT[] DEFAULT '{"full"}',
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_pat_token_hash ON personal_access_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_pat_user_id ON personal_access_tokens(user_id);

