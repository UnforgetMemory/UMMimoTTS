use crate::models::response::{VoiceInfo, VoiceListResponse};
use actix_web::{web, HttpResponse, Responder};

// MIMO 官网 CDN 基础 URL
const CDN_BASE_URL: &str = "https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio";

pub async fn list_voices() -> impl Responder {
    let voices = vec![
        VoiceInfo {
            id: "冰糖".to_string(),
            name: "冰糖".to_string(),
            language: "中文".to_string(),
            gender: "女性".to_string(),
            style: "活泼少女".to_string(),
            preview_url: Some(format!("{}/bingtang.wav", CDN_BASE_URL)),
        },
        VoiceInfo {
            id: "茉莉".to_string(),
            name: "茉莉".to_string(),
            language: "中文".to_string(),
            gender: "女性".to_string(),
            style: "知性女声".to_string(),
            preview_url: Some(format!("{}/moli.wav", CDN_BASE_URL)),
        },
        VoiceInfo {
            id: "苏打".to_string(),
            name: "苏打".to_string(),
            language: "中文".to_string(),
            gender: "男性".to_string(),
            style: "阳光少年".to_string(),
            preview_url: Some(format!("{}/suda.wav", CDN_BASE_URL)),
        },
        VoiceInfo {
            id: "白桦".to_string(),
            name: "白桦".to_string(),
            language: "中文".to_string(),
            gender: "男性".to_string(),
            style: "成熟男声".to_string(),
            preview_url: Some(format!("{}/baihua.wav", CDN_BASE_URL)),
        },
        VoiceInfo {
            id: "Mia".to_string(),
            name: "Mia".to_string(),
            language: "English".to_string(),
            gender: "Female".to_string(),
            style: "Lively girl".to_string(),
            preview_url: Some(format!("{}/mia.wav", CDN_BASE_URL)),
        },
        VoiceInfo {
            id: "Chloe".to_string(),
            name: "Chloe".to_string(),
            language: "English".to_string(),
            gender: "Female".to_string(),
            style: "Sweet Dreamy".to_string(),
            preview_url: Some(format!("{}/chloe.wav", CDN_BASE_URL)),
        },
        VoiceInfo {
            id: "Milo".to_string(),
            name: "Milo".to_string(),
            language: "English".to_string(),
            gender: "Male".to_string(),
            style: "Sunny boy".to_string(),
            preview_url: Some(format!("{}/milo.wav", CDN_BASE_URL)),
        },
        VoiceInfo {
            id: "Dean".to_string(),
            name: "Dean".to_string(),
            language: "English".to_string(),
            gender: "Male".to_string(),
            style: "Steady Gentle".to_string(),
            preview_url: Some(format!("{}/dean.wav", CDN_BASE_URL)),
        },
    ];

    HttpResponse::Ok().json(VoiceListResponse { voices })
}

pub async fn preview_voice(path: web::Path<String>) -> impl Responder {
    let voice_id = path.into_inner();
    tracing::info!("请求音色预览: {}", voice_id);

    // 验证音色是否存在
    let valid_voices = vec![
        "冰糖", "茉莉", "苏打", "白桦", "Mia", "Chloe", "Milo", "Dean",
    ];

    if !valid_voices.contains(&voice_id.as_str()) {
        tracing::warn!("无效的音色 ID: {}", voice_id);
        return HttpResponse::NotFound().json(serde_json::json!({
            "error": "音色不存在"
        }));
    }

    // 从 CDN 获取预览音频
    let cdn_url = format!("{}/{}.wav", CDN_BASE_URL, voice_id);
    tracing::info!("从 CDN 获取音频: {}", cdn_url);
    
    // 使用 reqwest 客户端下载音频
    let client = reqwest::Client::new();
    match client.get(&cdn_url).send().await {
        Ok(response) => {
            tracing::info!("CDN 响应状态: {}", response.status());
            if response.status().is_success() {
                match response.bytes().await {
                    Ok(audio_data) => {
                        tracing::info!("成功获取音频数据: {} bytes", audio_data.len());
                        HttpResponse::Ok()
                            .content_type("audio/wav")
                            .insert_header((
                                "Cache-Control",
                                "public, max-age=86400", // 缓存 24 小时
                            ))
                            .body(audio_data.to_vec())
                    },
                    Err(e) => {
                        tracing::error!("读取音频数据失败: {}", e);
                        HttpResponse::InternalServerError().json(serde_json::json!({
                            "error": "音频数据读取失败"
                        }))
                    },
                }
            } else {
                tracing::warn!("CDN 返回错误状态: {}", response.status());
                HttpResponse::NotFound().json(serde_json::json!({
                    "error": "试听音频暂未提供"
                }))
            }
        }
        Err(e) => {
            tracing::error!("CDN 请求失败: {}", e);
            HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "error": "CDN 服务不可用，请稍后重试"
            }))
        },
    }
}
