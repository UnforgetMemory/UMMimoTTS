//! Centralized constants for MIMO TTS — voice presets, model registry, defaults.
//!
//! This module is the **single source of truth** for all voice/model configuration.
//! Frontend fetches from `GET /api/v2/config`; backend uses these directly.

use serde::Serialize;

// ── Default Model ──────────────────────────────────────────────────

/// The default MIMO TTS model identifier.
pub const DEFAULT_MODEL: &str = "mimo-v2.5-tts";

/// The default voice identifier (Chinese name used as ID in MIMO API).
pub const DEFAULT_VOICE: &str = "冰糖";

/// Default speech speed multiplier.
pub const DEFAULT_SPEED: f64 = 1.0;

// ── MIMO API Configuration ─────────────────────────────────────────

/// Default MIMO API base URL.
pub const MIMO_BASE_URL_DEFAULT: &str = "https://api.xiaomimimo.com";

/// Rate limit: requests per minute per voice (MIMO enforces ~20 RPM per voice).
pub const MIMO_RPM_PER_VOICE: u32 = 20;

/// Rate limit: requests per minute per application (MIMO allows ~120 RPM per app).
pub const MIMO_RPM_PER_APP: u32 = 120;

/// Default rate limit: API requests per minute (configurable via MIMO_RPM env var).
pub const MIMO_RPM_DEFAULT: u64 = 100;

/// Default token budget: max tokens processed per minute (configurable via MIMO_TOKEN_BUDGET_RPM env var).
pub const MIMO_TOKEN_BUDGET_RPM_DEFAULT: u64 = 10_000_000;

/// Per-provider RPM limit (each provider has independent quota).
pub const MIMO_RPM_PER_PROVIDER: u64 = 100;

/// Per-provider TPM limit (each provider has independent quota).
pub const MIMO_TPM_PER_PROVIDER: u64 = 10_000_000;

/// Default burst capacity per provider (safe burst without triggering per-second limits).
pub const MIMO_BURST_PER_PROVIDER: u64 = 10;

// ── Voice Presets ───────────────────────────────────────────────────

/// A voice preset exposed to the frontend and used for validation.
#[derive(Debug, Clone, Serialize)]
pub struct VoicePreset {
    /// Voice identifier — this is the value sent to the MIMO API.
    pub id: &'static str,
    /// Human-readable display name.
    pub name: &'static str,
    /// Language code or label.
    pub language: &'static str,
    /// Gender label.
    pub gender: &'static str,
    /// Style description.
    pub style: &'static str,
    /// CDN URL for preview audio.
    pub preview_url: &'static str,
}

/// All built-in voice presets. Order matches the UI display order.
pub const VOICE_PRESETS: &[VoicePreset] = &[
    VoicePreset {
        id: "冰糖",
        name: "冰糖",
        language: "中文",
        gender: "女性",
        style: "活泼少女",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/bingtang.wav",
    },
    VoicePreset {
        id: "茉莉",
        name: "茉莉",
        language: "中文",
        gender: "女性",
        style: "知性女声",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/moli.wav",
    },
    VoicePreset {
        id: "苏打",
        name: "苏打",
        language: "中文",
        gender: "男性",
        style: "阳光少年",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/suda.wav",
    },
    VoicePreset {
        id: "白桦",
        name: "白桦",
        language: "中文",
        gender: "男性",
        style: "成熟男声",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/baihua.wav",
    },
    VoicePreset {
        id: "Mia",
        name: "Mia",
        language: "English",
        gender: "Female",
        style: "Lively girl",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/mia.wav",
    },
    VoicePreset {
        id: "Chloe",
        name: "Chloe",
        language: "English",
        gender: "Female",
        style: "Sweet Dreamy",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/chloe.wav",
    },
    VoicePreset {
        id: "Milo",
        name: "Milo",
        language: "English",
        gender: "Male",
        style: "Sunny boy",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/milo.wav",
    },
    VoicePreset {
        id: "Dean",
        name: "Dean",
        language: "English",
        gender: "Male",
        style: "Steady Gentle",
        preview_url: "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/dean.wav",
    },
];

// ── Model Registry ──────────────────────────────────────────────────

/// A model preset exposed to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct ModelPreset {
    /// Model identifier sent to the MIMO API.
    pub id: &'static str,
    /// Human-readable display name.
    pub name: &'static str,
    /// Description of the model's capabilities.
    pub description: &'static str,
}

/// All supported MIMO TTS models.
pub const MODEL_PRESETS: &[ModelPreset] = &[
    ModelPreset {
        id: "mimo-v2.5-tts",
        name: "MiMo TTS v2.5",
        description: "Xiaomi MiMo text-to-speech model v2.5",
    },
];

// ── Helper Functions ────────────────────────────────────────────────

/// Check if a voice ID is a known preset.
pub fn is_valid_voice(voice: &str) -> bool {
    VOICE_PRESETS.iter().any(|v| v.id == voice)
}

/// Check if a model ID is a known preset.
pub fn is_valid_model(model: &str) -> bool {
    MODEL_PRESETS.iter().any(|m| m.id == model)
}

/// Get a voice preset by ID.
pub fn get_voice_preset(id: &str) -> Option<&'static VoicePreset> {
    VOICE_PRESETS.iter().find(|v| v.id == id)
}

/// Get a model preset by ID.
pub fn get_model_preset(id: &str) -> Option<&'static ModelPreset> {
    MODEL_PRESETS.iter().find(|m| m.id == id)
}

// ── Config Response (for GET /api/v2/config) ────────────────────────

/// Full configuration response returned by the config endpoint.
#[derive(Serialize)]
pub struct ConfigResponse {
    pub voices: &'static [VoicePreset],
    pub models: &'static [ModelPreset],
    pub default_voice: &'static str,
    pub default_model: &'static str,
    pub default_speed: f64,
    pub mimo_base_url: String,
}

/// Build the config response using compile-time constants.
pub fn config_response(mimo_base_url: &str) -> ConfigResponse {
    ConfigResponse {
        voices: VOICE_PRESETS,
        models: MODEL_PRESETS,
        default_voice: DEFAULT_VOICE,
        default_model: DEFAULT_MODEL,
        default_speed: DEFAULT_SPEED,
        mimo_base_url: mimo_base_url.to_string(),
    }
}
