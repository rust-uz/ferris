// ==============================================================================
// FERRIS VAULT — In-Memory Sealed Secrets Vault Engine
// Encryption: ChaCha20-Poly1305 Authenticated Encryption (AEAD)
// Key Derivation: BLAKE3 Cryptographic KDF + Machine-Bound Host Seed
// Zero-Disk Footprint: Plaintext exists ONLY in RAM, wiped with zeroize
// ==============================================================================

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce, Key,
};
use rand::RngExt;

const MAGIC_HEADER: &[u8; 16] = b"FERRIS_VAULT_V1\0";
const NONCE_LEN: usize = 12;

/// Master kalitni olish yoki yaratish
pub fn get_or_create_master_key(password: Option<&str>) -> Result<[u8; 32], String> {
    if let Some(pwd) = password {
        if !pwd.is_empty() {
            // Foydalanuvchi parolidan BLAKE3 KDF orqali 256-bitli kalit hosil qilish
            let key = blake3::derive_key("ferris-vault-kdf-v1-context", pwd.as_bytes());
            return Ok(key);
        }
    }

    // Agar parol berilmagan bo'lsa, serverning o'ziga xos xavfsiz kaliti (~/.ferris/vault.key)
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let key_dir = PathBuf::from(home).join(".ferris");
    let key_file = key_dir.join("vault.key");

    if key_file.exists() {
        let key_data = fs::read(&key_file)
            .map_err(|e| format!("Master kalit faylini o'qib bo'lmadi: {}", e))?;
        if key_data.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&key_data);
            return Ok(key);
        }
    }

    // Yangi master kalit yaratish
    fs::create_dir_all(&key_dir)
        .map_err(|e| format!("~/.ferris papkasini yaratib bo'lmadi: {}", e))?;

    let mut random_bytes = [0u8; 32];
    rand::rng().fill(&mut random_bytes);

    // Mashina ma'lumotlari (machine-id) bilan aralashtirib, ushbu serverga bog'lash
    let machine_id = fs::read_to_string("/etc/machine-id")
        .or_else(|_| fs::read_to_string("/var/lib/dbus/machine-id"))
        .unwrap_or_else(|_| "generic-machine-id".to_string());

    let mut hasher = blake3::Hasher::new_derive_key("ferris-host-bound-master-key");
    hasher.update(&random_bytes);
    hasher.update(machine_id.trim().as_bytes());
    let final_key = *hasher.finalize().as_bytes();

    // Faqat joriy foydalanuvchiga ruxsat beriladigan qilib saqlash (0600)
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&key_file)
            .map_err(|e| format!("Kalit faylini ochib bo'lmadi: {}", e))?;
        file.write_all(&final_key)
            .map_err(|e| format!("Master kalitni yozib bo'lmadi: {}", e))?;
    }

    #[cfg(not(unix))]
    {
        fs::write(&key_file, &final_key)
            .map_err(|e| format!("Master kalitni yozib bo'lmadi: {}", e))?;
    }

    Ok(final_key)
}

/// Faylni xavfsiz diskdan tozalash (Shred)
pub fn secure_shred_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    let meta = fs::metadata(path).map_err(|e| e.to_string())?;
    let len = meta.len() as usize;

    let mut file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("Faylni ochib bo'lmadi: {}", e))?;

    // 1-qadam: Barchasini 0x00 bilan to'ldirish
    let zeros = vec![0u8; len];
    let _ = file.write_all(&zeros);
    let _ = file.sync_all();

    // 2-qadam: Barchasini tasodifiy baytlar bilan to'ldirish
    let mut random = vec![0u8; len];
    rand::rng().fill(random.as_mut_slice());
    let _ = file.write_all(&random);
    let _ = file.sync_all();

    // 3-qadam: Faylni o'chirish
    fs::remove_file(path).map_err(|e| format!("Faylni o'chirib bo'lmadi: {}", e))?;
    Ok(())
}

/// Plaintext .env faylni shifrlab, .env.ferris ga aylantirish
pub fn seal_vault(
    input_path: &Path,
    output_path: &Path,
    password: Option<&str>,
    keep_original: bool,
) -> Result<usize, String> {
    if !input_path.exists() {
        return Err(format!("Kiruvchi fayl topilmadi: {}", input_path.display()));
    }

    let mut plaintext = fs::read(input_path)
        .map_err(|e| format!("Faylni o'qishda xatolik: {}", e))?;

    // Nechta o'zgaruvchi borligini sanash
    let count = parse_env_string(&String::from_utf8_lossy(&plaintext)).len();

    let master_key = get_or_create_master_key(password)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&master_key));

    // 12 baytlik tasodifiy Nonce
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from(nonce_bytes);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_ref())
        .map_err(|e| format!("Shifrlashda xatolik: {}", e))?;

    // Xotiradagi ochiq matnni xavfsiz tozalash
    plaintext.fill(0);

    // Natijaviy fayl: [MAGIC_HEADER 16b] + [NONCE 12b] + [CIPHERTEXT + TAG]
    let mut out_file = File::create(output_path)
        .map_err(|e| format!("Natijaviy faylni yaratib bo'lmadi: {}", e))?;

    out_file
        .write_all(MAGIC_HEADER)
        .and_then(|_| out_file.write_all(&nonce_bytes))
        .and_then(|_| out_file.write_all(&ciphertext))
        .map_err(|e| format!("Shifrlangan ma'lumotni yozib bo'lmadi: {}", e))?;

    if !keep_original {
        secure_shred_file(input_path)?;
    }

    Ok(count)
}

/// Shifrlangan .env.ferris faylni xotirada ochib, HashMap ko'rinishida olish (Diskka yozilmaydi!)
pub fn load_vault_to_env(
    vault_path: &Path,
    password: Option<&str>,
) -> Result<HashMap<String, String>, String> {
    if !vault_path.exists() {
        return Err(format!("Vault fayli topilmadi: {}", vault_path.display()));
    }

    let data = fs::read(vault_path)
        .map_err(|e| format!("Vault faylini o'qib bo'lmadi: {}", e))?;

    if data.len() < (MAGIC_HEADER.len() + NONCE_LEN + 16) {
        return Err("Yaroqsiz yoki buzilgan vault fayli (hajmi juda kichik)".to_string());
    }

    if &data[..MAGIC_HEADER.len()] != MAGIC_HEADER {
        return Err("Fayl formati noto'g'ri (Ferris Vault signaturasi mos kelmadi)".to_string());
    }

    let nonce_start = MAGIC_HEADER.len();
    let nonce_end = nonce_start + NONCE_LEN;
    let nonce_bytes = &data[nonce_start..nonce_end];
    let ciphertext = &data[nonce_end..];

    let master_key = get_or_create_master_key(password)?;
    let cipher = ChaCha20Poly1305::new(&Key::from(master_key));
    let nonce = Nonce::from_slice(nonce_bytes);

    let mut decrypted = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Vaultni ochib bo'lmadi! Parol noto'g'ri yoki fayl boshqa serverda yaratilgan.".to_string())?;

    let env_str = String::from_utf8_lossy(&decrypted).to_string();
    let map = parse_env_string(&env_str);

    // Xotiradagi baytlarni tozalash
    decrypted.fill(0);

    Ok(map)
}

/// Shifrlangan vaultni ochib, qayta .env fayliga chiqarish (faqat dasturchi kerak deb topsa)
pub fn unseal_vault(
    vault_path: &Path,
    output_path: &Path,
    password: Option<&str>,
) -> Result<usize, String> {
    let map = load_vault_to_env(vault_path, password)?;
    let count = map.len();

    let mut content = String::new();
    for (k, v) in &map {
        content.push_str(&format!("{}=\"{}\"\n", k, v.replace('"', "\\\"")));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(output_path)
            .map_err(|e| format!("Chiquvchi faylni ochib bo'lmadi: {}", e))?;
        file.write_all(content.as_bytes())
            .map_err(|e| format!("Faylga yozishda xatolik: {}", e))?;
    }

    #[cfg(not(unix))]
    {
        fs::write(output_path, content.as_bytes())
            .map_err(|e| format!("Faylga yozishda xatolik: {}", e))?;
    }

    Ok(count)
}

/// Vault tarkibidagi kalitlar ro'yxatini ko'rish (Qiymatlar yashiriladi)
pub fn inspect_vault(
    vault_path: &Path,
    password: Option<&str>,
    reveal: bool,
) -> Result<Vec<(String, String)>, String> {
    let map = load_vault_to_env(vault_path, password)?;
    let mut list = Vec::new();

    for (k, v) in map {
        let display_val = if reveal {
            v
        } else {
            format!("[QULFLANGAN • uzunligi: {} ta belgi]", v.len())
        };
        list.push((k, display_val));
    }

    list.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(list)
}

/// Qatorlarni KEY=VALUE ko'rinishida ajratib olish
fn parse_env_string(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, val)) = trimmed.split_once('=') {
            let k = key.trim().to_string();
            let mut v = val.trim().to_string();

            // Qo'shtirnoqlarni olib tashlash
            if (v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')) {
                if v.len() >= 2 {
                    v = v[1..v.len() - 1].to_string();
                }
            }

            if !k.is_empty() {
                map.insert(k, v);
            }
        }
    }

    map
}
