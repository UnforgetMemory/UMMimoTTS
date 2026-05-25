use crate::models::response::{ErrorResponse, GroupSummary, StatsSummary};
use crate::state::app_state::AppState;
use actix_web::{web, HttpResponse, Responder};

/// GET /api/v1/stats/summary - Return aggregated stats (cached)
pub async fn stats_summary(data: web::Data<AppState>) -> impl Responder {
    let stats: StatsSummary = data.get_stats_summary();
    HttpResponse::Ok().json(stats)
}

/// GET /api/v1/groups/{group_id}/stats - Return per-group stats (on-demand)
pub async fn group_stats(
    path: web::Path<String>,
    data: web::Data<AppState>,
) -> impl Responder {
    let group_id = path.into_inner();

    // Verify group exists
    if data.get_group(&group_id).is_none() {
        return HttpResponse::NotFound().json(ErrorResponse {
            error: "分组不存在".to_string(),
            message: format!("分组 ID: {}", group_id),
            code: Some("GROUP_NOT_FOUND".to_string()),
        });
    }

    let stats = data.get_group_stats(&group_id);
    HttpResponse::Ok().json(stats)
}

/// GET /api/v1/stats/groups - Return all group summaries (for group stats view)
pub async fn stats_groups(data: web::Data<AppState>) -> impl Responder {
    let groups = data.list_groups();
    let summaries: Vec<GroupSummary> = groups.into_iter().map(|g| g.to_summary()).collect();
    HttpResponse::Ok().json(summaries)
}
