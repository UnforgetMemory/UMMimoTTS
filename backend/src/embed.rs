use actix_web::{web, HttpRequest, HttpResponse, Responder};
use rust_embed::Embed;
use mime_guess::from_path;

#[derive(Embed)]
#[folder = "../frontend/dist/"]
struct FrontendAssets;

/// 服务嵌入的前端静态文件
pub async fn serve_embedded(req: HttpRequest) -> impl Responder {
    let path = req.path();
    
    // 去掉开头的 /
    let path = if path.starts_with('/') {
        &path[1..]
    } else {
        path
    };

    // 尝试获取嵌入的文件
    if let Some(content) = FrontendAssets::get(path) {
        let mime = from_path(path).first_or_octet_stream();
        return HttpResponse::Ok()
            .content_type(mime.as_ref())
            .body(content.data.to_vec());
    }

    // 对于 SPA 路由，返回 index.html
    if let Some(index) = FrontendAssets::get("index.html") {
        return HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(index.data.to_vec());
    }

    HttpResponse::NotFound().finish()
}

/// 配置嵌入的前端静态文件服务
pub fn config_embedded(cfg: &mut web::ServiceConfig) {
    // 根路径直接返回 index.html
    cfg.service(
        web::resource("/")
            .route(web::get().to(serve_index))
    );
    
    // 其他路径匹配静态文件
    cfg.service(
        web::resource("/{path:.*}")
            .route(web::get().to(serve_embedded))
    );
}

/// 服务 index.html
async fn serve_index() -> impl Responder {
    if let Some(index) = FrontendAssets::get("index.html") {
        HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(index.data.to_vec())
    } else {
        HttpResponse::NotFound().finish()
    }
}
