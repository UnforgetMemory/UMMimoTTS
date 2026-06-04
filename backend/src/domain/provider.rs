//! Provider domain types.
//!
//! A "provider" is a MIMO TTS API endpoint with its own base URL and API key.
//! Providers are pre-configured (seeded in DB) — users only set API keys.

use serde::{Serialize, Deserialize};

/// A pre-configured MIMO provider preset (name + base URL, no key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub base_url: String,
}
