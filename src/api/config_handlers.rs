use crate::api::auth_handlers::{save_user_config_to_disk, AppState};
use crate::models::UserConfig;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use std::path::Path as FilePath;

pub async fn get_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserConfig>, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let token = auth_header.strip_prefix("Bearer ").unwrap_or_default().trim();

    let sessions = state.sessions.read().await;
    let uname = sessions.get(token).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "未登录" })),
        )
    })?;

    let cfg = load_user_config(&state.config_dir, uname).await;
    Ok(Json(cfg))
}

pub async fn save_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UserConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let token = auth_header.strip_prefix("Bearer ").unwrap_or_default().trim();

    let sessions = state.sessions.read().await;
    let uname = sessions.get(token).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "未登录" })),
        )
    })?;

    save_user_config_to_disk(&state.config_dir, uname, &payload).await;

    // Clear fetcher cache for any modified sub URLs
    for sub in &payload.subscriptions {
        state.fetcher.clear_cache(Some(&sub.url)).await;
    }

    Ok(Json(serde_json::json!({ "success": true, "message": "配置保存成功" })))
}

pub async fn inspect_nodes_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sub_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let token = auth_header.strip_prefix("Bearer ").unwrap_or_default().trim();

    let sessions = state.sessions.read().await;
    let uname = sessions.get(token).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "未登录" })),
        )
    })?;

    let mut cfg = load_user_config(&state.config_dir, uname).await;
    let sub = cfg.subscriptions.iter_mut().find(|s| s.id == sub_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "订阅源不存在" })),
        )
    })?;

    let prefix = sub.prefix.as_deref().or(Some(&sub.name)).unwrap_or_default();
    let res = state.fetcher.fetch(&sub.url, prefix, false).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
    })?;

    let mut nodes_json = Vec::new();
    for node in &res.nodes {
        let region = crate::engine::renamer::detect_node_primary_region(&node.name, &node.server, sub.default_region.as_deref());
        let (country_code, country_name, country_flag) = match region {
            Some(r) => (r.code.to_string(), r.name.to_string(), r.flag.to_string()),
            None => ("UN".into(), "其它".into(), "🌐".into()),
        };

        nodes_json.push(serde_json::json!({
            "name": node.name,
            "type": node.node_type.to_uppercase(),
            "server": node.server,
            "port": node.port,
            "countryCode": country_code,
            "countryName": country_name,
            "countryFlag": country_flag,
            "rawName": node.raw_name.clone().unwrap_or_else(|| node.name.clone())
        }));
    }

    sub.nodes_count = Some(nodes_json.len());
    save_user_config_to_disk(&state.config_dir, uname, &cfg).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "nodesCount": nodes_json.len(),
        "nodes": nodes_json,
        "sourceType": res.source_type,
        "userInfo": res.user_info
    })))
}

pub async fn load_user_config(config_dir: &str, username: &str) -> UserConfig {
    let file = FilePath::new(config_dir).join(format!("user_{}.json", username.to_lowercase()));
    if file.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&file).await {
            if let Ok(cfg) = serde_json::from_str::<UserConfig>(&content) {
                return cfg;
            }
        }
    }
    UserConfig::default()
}
