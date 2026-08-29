//! Secrets handling (ADR-007).
//!
//! - Provider API keys are sealed with AES-256-GCM under a local master key
//!   (stored in `data/master.key`, engine-managed; OS ACL tightened by the
//!   CLI layer on supported platforms).
//! - API tokens are stored only as SHA-256 hashes.
//! - All key material is zeroized on drop.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const MASTER_KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;
const SEAL_PREFIX: &str = "v1:";

/// Local master key. Zeroized on drop; never serialized.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; MASTER_KEY_LEN]);

impl MasterKey {
    /// Generate a fresh random key (used on first run).
    pub fn generate() -> Self {
        use aes_gcm::aead::rand_core::RngCore;
        let mut k = [0u8; MASTER_KEY_LEN];
        OsRng.fill_bytes(&mut k);
        Self(k)
    }
    pub fn from_bytes(bytes: [u8; MASTER_KEY_LEN]) -> Self {
        Self(bytes)
    }
    pub fn from_hex(hex: &str) -> Option<Self> {
        let bytes = hex::decode(hex).ok()?;
        if bytes.len() != MASTER_KEY_LEN {
            return None;
        }
        let mut k = [0u8; MASTER_KEY_LEN];
        k.copy_from_slice(&bytes);
        Some(Self(k))
    }
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
    pub fn as_bytes(&self) -> &[u8; MASTER_KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MasterKey(•••)")
    }
}

/// AES-256-GCM seal/unseal for at-rest secrets.
pub struct Crypto {
    cipher: Aes256Gcm,
}

impl Crypto {
    pub fn new(key: &MasterKey) -> Self {
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key.as_bytes()));
        Self { cipher }
    }

    /// Seal a plaintext secret → `"v1:<base64(nonce || ciphertext)>"`.
    pub fn seal(&self, plaintext: &str) -> String {
        let nonce_bytes = {
            use aes_gcm::aead::rand_core::RngCore;
            let mut n = [0u8; NONCE_LEN];
            OsRng.fill_bytes(&mut n);
            n
        };
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .expect("AES-GCM encrypt cannot fail for in-memory data");
        let mut payload = Vec::with_capacity(NONCE_LEN + ct.len());
        payload.extend_from_slice(&nonce_bytes);
        payload.extend_from_slice(&ct);
        format!("{SEAL_PREFIX}{}", B64.encode(payload))
    }

    /// Open a sealed secret. Returns `None` on wrong key / corruption.
    pub fn open(&self, sealed: &str) -> Option<String> {
        let body = sealed.strip_prefix(SEAL_PREFIX)?;
        let payload = B64.decode(body).ok()?;
        if payload.len() <= NONCE_LEN {
            return None;
        }
        let (nonce_bytes, ct) = payload.split_at(NONCE_LEN);
        let plain = self
            .cipher
            .decrypt(Nonce::from_slice(nonce_bytes), ct)
            .ok()?;
        Some(String::from_utf8(plain).ok()?)
    }
}

/// SHA-256 token hash (hex) — tokens are stored hashed, never in plaintext.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

// ── scoped URL credentials (umreview option B) ────────────────────────────
//
// Raw bearer tokens must never ride in URLs (history / access logs / Referer).
// Instead the server signs short-lived, scope-bound credentials with the
// master key:
//   scoped:v1:{expiry_unix}:{scope}:{hmac_hex}
// Scope patterns: `audio:{task_id}`, `events:{channel}`, `preview:{voice_id}`.

use hmac::{Hmac, Mac};

type ScopedHmac = Hmac<Sha256>;

fn scoped_material(_key: &MasterKey, expiry: u64, scope: &str) -> String {
    format!("mimotts-scoped|v1|{expiry}|{scope}")
}

/// Sign a short-lived scoped credential.
pub fn sign_scoped(key: &MasterKey, scope: &str, ttl_secs: u64) -> String {
    let expiry = chrono::Utc::now().timestamp() as u64 + ttl_secs;
    let mut mac = <ScopedHmac as Mac>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(scoped_material(key, expiry, scope).as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    format!("scoped:v1:{expiry}:{scope}:{sig}")
}

/// Verify a scoped credential: signature + scope + expiry must all match.
/// Comparison is constant-time via `verify_slice` (no token timing channel).
pub fn verify_scoped(key: &MasterKey, token: &str, expected_scope: &str) -> bool {
    let rest = match token.strip_prefix("scoped:v1:") {
        Some(r) => r,
        None => return false,
    };
    // `{expiry}:{scope}:{sig}` — scope itself may contain ':' (events:task:x),
    // so split from the right.
    let (head, sig) = match rest.rsplit_once(':') {
        Some(p) => p,
        None => return false,
    };
    let (expiry_str, scope) = match head.split_once(':') {
        Some(p) => p,
        None => return false,
    };
    if scope != expected_scope {
        return false;
    }
    let Ok(expiry) = expiry_str.parse::<u64>() else {
        return false;
    };
    if chrono::Utc::now().timestamp() as u64 >= expiry {
        return false;
    }
    let mut mac = <ScopedHmac as Mac>::new_from_slice(key.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(scoped_material(key, expiry, scope).as_bytes());
    match hex::decode(sig) {
        Ok(sig_bytes) => mac.verify_slice(&sig_bytes).is_ok(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let key = MasterKey::generate();
        let crypto = Crypto::new(&key);
        let sealed = crypto.seal("sk-live-very-secret");
        assert!(sealed.starts_with("v1:"));
        assert!(!sealed.contains("very-secret"));
        assert_eq!(crypto.open(&sealed).as_deref(), Some("sk-live-very-secret"));
    }

    #[test]
    fn wrong_key_fails() {
        let sealed = Crypto::new(&MasterKey::generate()).seal("secret");
        assert_eq!(Crypto::new(&MasterKey::generate()).open(&sealed), None);
    }

    #[test]
    fn corrupted_seal_fails() {
        let key = MasterKey::generate();
        let sealed = Crypto::new(&key).seal("secret");
        let mut tampered = sealed.clone();
        tampered.pop();
        assert_eq!(Crypto::new(&key).open(&tampered), None);
    }

    #[test]
    fn unicode_roundtrip() {
        let key = MasterKey::generate();
        let crypto = Crypto::new(&key);
        let sealed = crypto.seal("密钥内容🔑");
        assert_eq!(crypto.open(&sealed).as_deref(), Some("密钥内容🔑"));
    }

    #[test]
    fn token_hash_stable() {
        assert_eq!(
            hash_token("tok-123"),
            hash_token("tok-123"),
            "hashing must be deterministic"
        );
        assert_ne!(hash_token("tok-123"), hash_token("tok-124"));
        assert_eq!(hash_token("tok-123").len(), 64);
    }

    #[test]
    fn scoped_roundtrip_and_scope_binding() {
        let key = MasterKey::generate();
        let token = sign_scoped(&key, "audio:abc", 300);
        assert!(token.starts_with("scoped:v1:"));
        assert!(verify_scoped(&key, &token, "audio:abc"));
        // wrong scope → rejected
        assert!(!verify_scoped(&key, &token, "audio:xyz"));
        assert!(!verify_scoped(&key, &token, "events:audio:abc"));
        // wrong key → rejected
        assert!(!verify_scoped(&MasterKey::generate(), &token, "audio:abc"));
    }

    #[test]
    fn scoped_expiry_enforced() {
        let key = MasterKey::generate();
        let token = sign_scoped(&key, "preview:冰糖", 0); // already expired
        assert!(!verify_scoped(&key, &token, "preview:冰糖"));
    }

    #[test]
    fn scoped_colon_scope_roundtrip() {
        let key = MasterKey::generate();
        let token = sign_scoped(&key, "events:task:uuid-123", 300);
        assert!(verify_scoped(&key, &token, "events:task:uuid-123"));
    }

    #[test]
    fn scoped_malformed_rejected() {
        let key = MasterKey::generate();
        assert!(!verify_scoped(&key, "not-a-token", "audio:x"));
        assert!(!verify_scoped(&key, "scoped:v1:", "audio:x"));
        assert!(!verify_scoped(&key, "scoped:v1:bad:sig", "audio:x"));
    }

    #[test]
    fn master_key_hex_roundtrip() {
        let key = MasterKey::generate();
        let restored = MasterKey::from_hex(&key.to_hex()).unwrap();
        assert_eq!(key.as_bytes(), restored.as_bytes());
        assert!(MasterKey::from_hex("short").is_none());
    }
}
