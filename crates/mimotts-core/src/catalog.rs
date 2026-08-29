//! Static catalog — official MiMo-V2.5-TTS registry.
//!
//! Sources:
//! - voices: https://mimo.mi.com/docs/zh-CN/quick-start/usage-guide/audio/speech-synthesis-v2.5
//! - models: https://mimo.mi.com/docs/zh-CN/quick-start/summary/model
//! `mimo_default` resolves to 冰糖 on CN clusters, Mia elsewhere (official note).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct VoicePreset {
    pub id: &'static str,
    pub name: &'static str,
    pub language: &'static str,
    pub gender: &'static str,
    pub style: &'static str,
    pub preview_url: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelPreset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// 9 official built-in voices (v3 was missing `mimo_default`).
pub const VOICES: &[VoicePreset] = &[
    VoicePreset {
        id: "mimo_default",
        name: "MiMo-默认",
        language: "中文/English",
        gender: "随集群",
        style: "默认精品音色（CN=冰糖 / 其他=Mia）",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/bingtang.wav"),
    },
    VoicePreset {
        id: "冰糖",
        name: "冰糖",
        language: "中文",
        gender: "女性",
        style: "活泼少女",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/bingtang.wav"),
    },
    VoicePreset {
        id: "茉莉",
        name: "茉莉",
        language: "中文",
        gender: "女性",
        style: "知性女声",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/moli.wav"),
    },
    VoicePreset {
        id: "苏打",
        name: "苏打",
        language: "中文",
        gender: "男性",
        style: "阳光少年",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/suda.wav"),
    },
    VoicePreset {
        id: "白桦",
        name: "白桦",
        language: "中文",
        gender: "男性",
        style: "成熟男声",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/baihua.wav"),
    },
    VoicePreset {
        id: "Mia",
        name: "Mia",
        language: "English",
        gender: "Female",
        style: "Lively girl",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/mia.wav"),
    },
    VoicePreset {
        id: "Chloe",
        name: "Chloe",
        language: "English",
        gender: "Female",
        style: "Sweet Dreamy",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/chloe.wav"),
    },
    VoicePreset {
        id: "Milo",
        name: "Milo",
        language: "English",
        gender: "Male",
        style: "Sunny boy",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/milo.wav"),
    },
    VoicePreset {
        id: "Dean",
        name: "Dean",
        language: "English",
        gender: "Male",
        style: "Steady Gentle",
        preview_url: Some("https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/dean.wav"),
    },
];

/// All three TTS models (v3 only registered one).
pub const MODELS: &[ModelPreset] = &[
    ModelPreset {
        id: "mimo-v2.5-tts",
        name: "MiMo TTS v2.5",
        description: "预置精品音色 · 支持唱歌模式 · 风格指令/音频标签",
    },
    ModelPreset {
        id: "mimo-v2.5-tts-voicedesign",
        name: "MiMo TTS v2.5 VoiceDesign",
        description: "文本描述定制音色（user 消息为音色设计描述，必填）",
    },
    ModelPreset {
        id: "mimo-v2.5-tts-voiceclone",
        name: "MiMo TTS v2.5 VoiceClone",
        description: "音频样本复刻音色（audio.voice 传 base64 data-URI，mp3/wav ≤10MB）",
    },
];

pub const DEFAULT_VOICE: &str = "mimo_default";
pub const DEFAULT_MODEL: &str = "mimo-v2.5-tts";

pub fn is_valid_voice(id: &str) -> bool {
    VOICES.iter().any(|v| v.id == id)
}
pub fn is_valid_model(id: &str) -> bool {
    MODELS.iter().any(|m| m.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_ids_unique_and_9() {
        assert_eq!(VOICES.len(), 9);
        let mut ids: Vec<_> = VOICES.iter().map(|v| v.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 9);
        assert!(is_valid_voice("mimo_default"));
        assert!(is_valid_voice("冰糖"));
        assert!(!is_valid_voice("不存在"));
    }

    #[test]
    fn three_models_registered() {
        assert_eq!(MODELS.len(), 3);
        assert!(is_valid_model("mimo-v2.5-tts-voiceclone"));
    }
}
