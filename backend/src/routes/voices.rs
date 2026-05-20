use crate::models::response::{VoiceInfo, VoiceListResponse};
use actix_web::{web, HttpResponse, Responder};

pub async fn list_voices() -> impl Responder {
    let voices = vec![
        VoiceInfo {
            id: "冰糖".to_string(),
            name: "冰糖".to_string(),
            language: "中文".to_string(),
            gender: "女性".to_string(),
            style: "活泼少女".to_string(),
        },
        VoiceInfo {
            id: "茉莉".to_string(),
            name: "茉莉".to_string(),
            language: "中文".to_string(),
            gender: "女性".to_string(),
            style: "知性女声".to_string(),
        },
        VoiceInfo {
            id: "苏打".to_string(),
            name: "苏打".to_string(),
            language: "中文".to_string(),
            gender: "男性".to_string(),
            style: "阳光少年".to_string(),
        },
        VoiceInfo {
            id: "白桦".to_string(),
            name: "白桦".to_string(),
            language: "中文".to_string(),
            gender: "男性".to_string(),
            style: "成熟男声".to_string(),
        },
        VoiceInfo {
            id: "Mia".to_string(),
            name: "Mia".to_string(),
            language: "English".to_string(),
            gender: "Female".to_string(),
            style: "Lively girl".to_string(),
        },
        VoiceInfo {
            id: "Chloe".to_string(),
            name: "Chloe".to_string(),
            language: "English".to_string(),
            gender: "Female".to_string(),
            style: "Sweet Dreamy".to_string(),
        },
        VoiceInfo {
            id: "Milo".to_string(),
            name: "Milo".to_string(),
            language: "English".to_string(),
            gender: "Male".to_string(),
            style: "Sunny boy".to_string(),
        },
        VoiceInfo {
            id: "Dean".to_string(),
            name: "Dean".to_string(),
            language: "English".to_string(),
            gender: "Male".to_string(),
            style: "Steady Gentle".to_string(),
        },
    ];

    HttpResponse::Ok().json(VoiceListResponse { voices })
}

pub async fn preview_voice(path: web::Path<String>) -> impl Responder {
    let voice_id = path.into_inner();

    // 验证音色是否存在
    let valid_voices = vec![
        "冰糖", "茉莉", "苏打", "白桦", "Mia", "Chloe", "Milo", "Dean",
    ];

    if !valid_voices.contains(&voice_id.as_str()) {
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "音色不存在"
        }));
    }

    // 返回预录音频文件路径
    // 假设音频文件存放在 backend/public/voices/{voice_id}.mp3
    let audio_path = format!("public/voices/{}.mp3", voice_id);

    // 尝试读取文件，如果不存在则返回错误
    match std::fs::read(&audio_path) {
        Ok(audio_data) => HttpResponse::Ok()
            .content_type("audio/mpeg")
            .body(audio_data),
        Err(_) => HttpResponse::NotFound().json(serde_json::json!({
            "error": "试听音频暂未提供"
        })),
    }
}
