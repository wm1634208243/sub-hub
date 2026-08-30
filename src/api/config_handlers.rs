use crate::api::auth_handlers::{save_user_config_to_disk, save_users_to_disk, AppState};
use crate::engine::compiler::compile_config_to_js;
use crate::engine::crypto::decrypt_user_config_bundle;
use crate::engine::renamer::format_node_name;
use crate::models::{User, UserConfig};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::path::Path as FilePath;

// ── Auth helper ──────────────────────────────────────────────────────────────

async fn get_authenticated_user(state: &AppState, headers: &HeaderMap) -> Result<(String, String), (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let token = auth_header.strip_prefix("Bearer ").unwrap_or_default().trim();

    let sessions = state.sessions.read().await;
    let uname = sessions.get(token).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "未登录或会话已过期" })),
        )
    })?;

    let users = state.users.read().await;
    let user_secret = users.iter()
        .find(|u| u.username.to_lowercase() == uname.to_lowercase())
        .map(|u| u.password_hash.clone())
        .unwrap_or_else(|| "subhub_master_secret_fallback_v1".to_string());

    Ok((uname.clone(), user_secret))
}

// ── Config Handlers ──────────────────────────────────────────────────────────

pub async fn get_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<UserConfig>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, secret) = get_authenticated_user(&state, &headers).await?;
    let cfg = load_user_config(&state.config_dir, &uname, &secret).await;
    Ok(Json(cfg))
}

pub async fn save_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UserConfig>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, _) = get_authenticated_user(&state, &headers).await?;

    save_user_config_to_disk(&state.config_dir, &uname, &payload).await;

    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let ip = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
        .unwrap_or("127.0.0.1");

    record_access_log(&state.config_dir, &uname, ip, ua, "⚙️ 配置保存发布", 200, "保存并即刻热重载分流规则").await;

    // Clear fetcher cache for any modified sub URLs
    for sub in &payload.subscriptions {
        state.fetcher.clear_cache(Some(&sub.url)).await;
    }

    Ok(Json(serde_json::json!({ "success": true, "message": "配置保存成功" })))
}

pub async fn purge_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, _) = get_authenticated_user(&state, &headers).await?;

    let cfg_file = FilePath::new(&state.config_dir).join(format!("user_{}.json", uname.to_lowercase()));
    let _ = tokio::fs::remove_file(cfg_file).await;

    let configs_file = FilePath::new(&state.config_dir).join("configs").join(format!("{}.json", uname));
    let _ = tokio::fs::remove_file(configs_file).await;

    Ok(Json(serde_json::json!({ "success": true, "message": "云端配置数据已彻底物理抹除！" })))
}

// ── Token Management ─────────────────────────────────────────────────────────

pub async fn regenerate_token_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, secret) = get_authenticated_user(&state, &headers).await?;
    let mut cfg = load_user_config(&state.config_dir, &uname, &secret).await;

    use rand::Rng;
    let hex_suffix: String = (0..8).map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>())).collect();
    let new_token = format!("rulehub_{}", hex_suffix);
    cfg.subscription_token = new_token.clone();

    save_user_config_to_disk(&state.config_dir, &uname, &cfg).await;

    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let ip = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
        .unwrap_or("127.0.0.1");

    record_access_log(&state.config_dir, &uname, ip, ua, "🔄 重置直链 Token", 200, "重新生成专属订阅 Token").await;

    Ok(Json(serde_json::json!({ "success": true, "token": new_token })))
}

#[derive(Deserialize)]
pub struct SetExpiryPayload {
    #[serde(alias = "expiresAt", alias = "expires_at")]
    pub expires_at: Option<String>,
}

pub async fn set_token_expiry_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<SetExpiryPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, secret) = get_authenticated_user(&state, &headers).await?;
    let mut cfg = load_user_config(&state.config_dir, &uname, &secret).await;

    cfg.token_expires_at = payload.expires_at.clone();
    save_user_config_to_disk(&state.config_dir, &uname, &cfg).await;

    Ok(Json(serde_json::json!({ "success": true, "tokenExpiresAt": payload.expires_at })))
}

// ── Subscription Node Inspection & Refresh ────────────────────────────────────

pub async fn inspect_nodes_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(sub_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, secret) = get_authenticated_user(&state, &headers).await?;

    let mut cfg = load_user_config(&state.config_dir, &uname, &secret).await;
    let sub = cfg.subscriptions.iter_mut().find(|s| s.id == sub_id).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "订阅源不存在" })),
        )
    })?;

    let prefix = sub.prefix.as_deref().or(Some(&sub.name)).unwrap_or_default();
    let res = state.fetcher.fetch(&sub.url, prefix, true).await.map_err(|e| {
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
    sub.user_info = res.user_info.clone();
    sub.source_type = Some(res.source_type.clone());
    save_user_config_to_disk(&state.config_dir, &uname, &cfg).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "nodesCount": nodes_json.len(),
        "nodes": nodes_json,
        "sourceType": res.source_type,
        "userInfo": res.user_info
    })))
}

pub async fn refresh_subscriptions_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, secret) = get_authenticated_user(&state, &headers).await?;
    let mut cfg = load_user_config(&state.config_dir, &uname, &secret).await;

    for sub in &mut cfg.subscriptions {
        if !sub.url.is_empty() {
            let prefix = sub.prefix.as_deref().or(Some(&sub.name)).unwrap_or_default();
            if let Ok(res) = state.fetcher.fetch(&sub.url, prefix, true).await {
                sub.nodes_count = Some(res.nodes.len());
                sub.user_info = res.user_info;
                sub.source_type = Some(res.source_type);
                sub.status = Some("online".into());
                sub.updated_at = Some(res.updated_at);
            }
        }
    }

    save_user_config_to_disk(&state.config_dir, &uname, &cfg).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "所有订阅已成功刷新！",
        "subscriptions": cfg.subscriptions
    })))
}

#[derive(Deserialize)]
pub struct TestSubPayload {
    pub url: String,
    pub prefix: Option<String>,
}

pub async fn test_subscription_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<TestSubPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;

    let prefix = payload.prefix.as_deref().unwrap_or_default();
    let res = state.fetcher.fetch(&payload.url, prefix, true).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": format!("测试拉取失败: {}", e) })),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "nodesCount": res.nodes.len(),
        "sourceType": res.source_type,
        "userInfo": res.user_info
    })))
}

// ── Node Renaming Preview & Health Checks ─────────────────────────────────────

#[derive(Deserialize)]
pub struct PreviewRenamePayload {
    #[serde(default, alias = "sampleNodes")]
    pub sample_nodes: Vec<serde_json::Value>,
    #[serde(default, alias = "sampleNames")]
    pub sample_names: Vec<String>,
    #[serde(default = "default_true", alias = "enableAutoFlags")]
    pub enable_auto_flags: bool,
    #[serde(default = "default_true", alias = "enableCleanAdAndRate")]
    pub enable_clean_ad_and_rate: bool,
    #[serde(default, alias = "customRenameRules")]
    pub custom_rename_rules: Vec<crate::models::CustomRenameRule>,
    #[serde(default, alias = "defaultRegion")]
    pub default_region: String,
}

fn default_true() -> bool {
    true
}

pub async fn preview_rename_handler(
    Json(payload): Json<PreviewRenamePayload>,
) -> Json<serde_json::Value> {
    let mut names = payload.sample_names;
    if names.is_empty() {
        for node in payload.sample_nodes {
            if let Some(n) = node.get("name").and_then(|s| s.as_str()) {
                names.push(n.to_string());
            }
        }
    }

    let mut results = Vec::new();
    for name in names {
        let formatted = format_node_name(
            &name,
            "",
            payload.enable_auto_flags,
            payload.enable_clean_ad_and_rate,
            &payload.custom_rename_rules,
            if payload.default_region.is_empty() { None } else { Some(&payload.default_region) },
        );
        results.push(serde_json::json!({
            "original": name,
            "formatted": formatted
        }));
    }

    Json(serde_json::json!({ "success": true, "results": results }))
}

#[derive(Deserialize)]
pub struct HealthTestPayload {
    #[serde(default = "default_timeout", alias = "timeoutMs")]
    pub timeout_ms: u64,
}

fn default_timeout() -> u64 {
    2000
}

pub async fn nodes_health_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<HealthTestPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, secret) = get_authenticated_user(&state, &headers).await?;
    let cfg = load_user_config(&state.config_dir, &uname, &secret).await;

    let mut all_proxies = Vec::new();
    for sub in &cfg.subscriptions {
        if sub.enabled {
            let prefix = sub.prefix.as_deref().or(Some(&sub.name)).unwrap_or_default();
            if let Ok(res) = state.fetcher.fetch(&sub.url, prefix, false).await {
                all_proxies.extend(res.nodes);
            }
        }
    }

    let results = crate::engine::aggregator::batch_test_proxies_health(&all_proxies, payload.timeout_ms).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "total": all_proxies.len(),
        "aliveCount": results.iter().filter(|r| r.get("alive").and_then(|v| v.as_bool()) == Some(true)).count(),
        "results": results
    })))
}

// ── Access Logs ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessLogEntry {
    pub id: String,
    pub time: String,
    pub ip: String,
    pub ua: String,
    #[serde(rename = "type")]
    pub log_type: String,
    pub status: u16,
    pub detail: String,
}

pub async fn record_access_log(
    config_dir: &str,
    username: &str,
    ip: &str,
    ua: &str,
    log_type: &str,
    status: u16,
    detail: &str,
) {
    let file = std::path::Path::new(config_dir).join("access_logs.json");
    let mut all_logs: std::collections::HashMap<String, Vec<AccessLogEntry>> = if file.exists() {
        tokio::fs::read_to_string(&file).await
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };

    let user_list = all_logs.entry(username.to_string()).or_default();
    let entry = AccessLogEntry {
        id: format!("log_{}_{:x}", chrono::Utc::now().timestamp_millis(), rand::random::<u32>()),
        time: chrono::Utc::now().to_rfc3339(),
        ip: if ip.is_empty() { "127.0.0.1".to_string() } else { ip.to_string() },
        ua: if ua.is_empty() { "Direct / Unknown UA".to_string() } else { ua.to_string() },
        log_type: log_type.to_string(),
        status,
        detail: detail.to_string(),
    };
    user_list.insert(0, entry);
    if user_list.len() > 100 {
        user_list.truncate(100);
    }

    if let Ok(json) = serde_json::to_string_pretty(&all_logs) {
        let _ = tokio::fs::write(&file, json).await;
    }
}

pub async fn get_access_logs_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, _) = get_authenticated_user(&state, &headers).await?;
    let file = FilePath::new(&state.config_dir).join("access_logs.json");
    if file.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&file).await {
            if let Ok(all_logs) = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&content) {
                if let Some(list) = all_logs.get(&uname) {
                    return Ok(Json(list.clone()));
                }
            }
        }
    }
    Ok(Json(serde_json::json!([])))
}

pub async fn clear_access_logs_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, _) = get_authenticated_user(&state, &headers).await?;
    let file = FilePath::new(&state.config_dir).join("access_logs.json");
    if file.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&file).await {
            if let Ok(mut all_logs) = serde_json::from_str::<std::collections::HashMap<String, serde_json::Value>>(&content) {
                all_logs.insert(uname, serde_json::json!([]));
                if let Ok(json) = serde_json::to_string_pretty(&all_logs) {
                    let _ = tokio::fs::write(&file, json).await;
                }
            }
        }
    }
    Ok(Json(serde_json::json!({ "success": true, "message": "日志已清空" })))
}

// ── Transient JS Compilation & Previews ──────────────────────────────────────

pub async fn compile_transient_handler(
    headers: HeaderMap,
    Json(payload): Json<UserConfig>,
) -> Json<serde_json::Value> {
    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let js = compile_config_to_js(&payload, ua);
    Json(serde_json::json!({ "success": true, "js": js }))
}

pub async fn preview_config_handler(
    headers: HeaderMap,
    Json(payload): Json<UserConfig>,
) -> Json<serde_json::Value> {
    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let js = compile_config_to_js(&payload, ua);
    Json(serde_json::json!({ "success": true, "js": js }))
}

pub async fn serve_rules_js_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    let uname = headers.get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "admin".into());

    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let cfg = load_user_config(&state.config_dir, &uname, "subhub_master_secret_fallback_v1").await;
    let js = compile_config_to_js(&cfg, ua);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/javascript; charset=utf-8")
        .body(axum::body::Body::from(js))
        .unwrap()
}

// ── Backup Export & Restore ──────────────────────────────────────────────────

pub async fn admin_backup_export_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let (uname, _) = get_authenticated_user(&state, &headers).await?;

    let users = state.users.read().await;
    let mut configs = serde_json::Map::new();

    for u in users.iter() {
        let cfg = load_user_config(&state.config_dir, &u.username, &u.password_hash).await;
        configs.insert(u.username.clone(), serde_json::to_value(cfg).unwrap_or(serde_json::Value::Null));
    }

    Ok(Json(serde_json::json!({
        "version": "2.0.0",
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "exportedBy": uname,
        "users": *users,
        "configs": configs
    })))
}

pub async fn admin_backup_restore_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;

    if let Some(users_val) = payload.get("users") {
        if let Ok(new_users) = serde_json::from_value::<Vec<User>>(users_val.clone()) {
            let mut users = state.users.write().await;
            *users = new_users.clone();
            save_users_to_disk(&state.config_dir, &new_users).await;
        }
    }

    if let Some(configs_val) = payload.get("configs").and_then(|v| v.as_object()) {
        for (uname, cfg_json) in configs_val {
            if let Ok(cfg) = serde_json::from_value::<UserConfig>(cfg_json.clone()) {
                save_user_config_to_disk(&state.config_dir, uname, &cfg).await;
            }
        }
    }

    Ok(Json(serde_json::json!({ "success": true, "message": "系统数据已成功从备份快照完全还原！" })))
}

pub async fn admin_get_backups_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    let settings = crate::backup::load_backup_settings(&state.config_dir).await;
    let archives = crate::backup::list_backup_archives(&state.config_dir).await;
    Ok(Json(serde_json::json!({
        "settings": settings,
        "archives": archives
    })))
}

pub async fn admin_save_backup_settings_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<crate::backup::BackupSettings>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    crate::backup::save_backup_settings(&state.config_dir, &payload).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e }))))?;
    Ok(Json(serde_json::json!({ "success": true, "settings": payload })))
}

pub async fn admin_create_backup_archive_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    let info = crate::backup::create_backup_archive(&state.config_dir, &state, "manual").await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e }))))?;
    Ok(Json(serde_json::json!({ "success": true, "archive": info })))
}

pub async fn admin_restore_backup_archive_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    let filename = payload.get("filename").and_then(|v| v.as_str())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "缺少 filename 参数" }))))?;

    crate::backup::restore_backup_archive(&state.config_dir, &state, filename).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e }))))?;

    Ok(Json(serde_json::json!({ "success": true, "message": format!("已成功从快照 {} 完整还原系统数据！", filename) })))
}

pub async fn admin_delete_backup_archive_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(filename): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    crate::backup::delete_backup_archive(&state.config_dir, &filename).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e }))))?;
    Ok(Json(serde_json::json!({ "success": true, "message": "备份快照已成功删除" })))
}

#[derive(Debug, Deserialize)]
pub struct BatchDeleteBackupsPayload {
    #[serde(default)]
    pub filenames: Vec<String>,
}

pub async fn admin_batch_delete_backups_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<BatchDeleteBackupsPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    let mut count = 0;
    for filename in &payload.filenames {
        if crate::backup::delete_backup_archive(&state.config_dir, filename).await.is_ok() {
            count += 1;
        }
    }
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("已成功批量删除 {} 份快照", count),
        "deletedCount": count
    })))
}

pub async fn admin_clear_all_backups_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    let list = crate::backup::list_backup_archives(&state.config_dir).await;
    let total = list.len();
    for b in list {
        let _ = crate::backup::delete_backup_archive(&state.config_dir, &b.filename).await;
    }
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("已清空全部 {} 份快照文件", total),
        "clearedCount": total
    })))
}

pub async fn admin_download_backup_archive_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(filename): Path<String>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let _ = get_authenticated_user(&state, &headers).await?;
    let clean = filename.trim();
    if clean.contains("..") || clean.contains('/') || clean.contains('\\') || !clean.ends_with(".json") {
        return Err((StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": "非法文件名" }))));
    }
    let target = FilePath::new(&state.config_dir).join("backups").join(clean);
    if !target.exists() {
        return Err((StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "文件不存在" }))));
    }
    let bytes = tokio::fs::read(&target).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    let res = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", clean))
        .body(axum::body::Body::from(bytes))
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))))?;

    Ok(res)
}

// ── Multi-Path User Config Loader ─────────────────────────────────────────────

pub async fn load_user_config(config_dir: &str, username: &str, user_secret: &str) -> UserConfig {
    let mut candidate_secrets = vec![
        user_secret.to_string(),
        "subhub_master_secret_fallback_v1".to_string(),
        "admin".to_string(),
    ];

    // Collect all known user hashes from users.json files
    let users_files = [
        FilePath::new(config_dir).join("users.json"),
        FilePath::new(config_dir).join("../data/users.json"),
        FilePath::new("data/users.json").to_path_buf(),
    ];
    for uf in users_files {
        if let Ok(content) = tokio::fs::read_to_string(&uf).await {
            if let Ok(users) = serde_json::from_str::<Vec<User>>(&content) {
                for u in users {
                    if !candidate_secrets.contains(&u.password_hash) {
                        candidate_secrets.push(u.password_hash);
                    }
                }
            }
        }
    }

    let candidates = [
        FilePath::new(config_dir).join(format!("user_{}.json", username.to_lowercase())),
        FilePath::new(config_dir).join("configs").join(format!("{}.json", username)),
        FilePath::new(config_dir).join(format!("{}.json", username)),
        FilePath::new(config_dir).join("user_admin.json"),
        FilePath::new(config_dir).join("configs/admin.json"),
        FilePath::new(config_dir).join("admin.json"),
        FilePath::new(config_dir).join("../data/configs").join(format!("{}.json", username)),
        FilePath::new(config_dir).join("../data/configs/admin.json"),
        FilePath::new(config_dir).join("../data/config.json"),
        FilePath::new(config_dir).join("config.json"),
        FilePath::new("data/config.json").to_path_buf(),
        FilePath::new("data/configs/admin.json").to_path_buf(),
    ];

    let mut best_fallback: Option<UserConfig> = None;

    for file in candidates {
        if file.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&file).await {
                if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Check if it's already an unencrypted UserConfig
                    if json_val.get("_encrypted").and_then(|v| v.as_bool()) != Some(true) {
                        if let Ok(cfg) = serde_json::from_value::<UserConfig>(json_val.clone()) {
                            if !cfg.subscriptions.is_empty() {
                                return cfg;
                            }
                            if best_fallback.is_none() {
                                best_fallback = Some(cfg);
                            }
                        }
                    } else {
                        // It is encrypted, try all candidate secrets
                        for sec in &candidate_secrets {
                            if let Ok(decrypted_val) = decrypt_user_config_bundle(&json_val, sec, username)
                                .or_else(|_| decrypt_user_config_bundle(&json_val, sec, "admin"))
                            {
                                if let Ok(cfg) = serde_json::from_value::<UserConfig>(decrypted_val) {
                                    if !cfg.subscriptions.is_empty() {
                                        // Save unencrypted copy for future smooth access
                                        save_user_config_to_disk(config_dir, username, &cfg).await;
                                        return cfg;
                                    }
                                    if best_fallback.is_none() {
                                        best_fallback = Some(cfg);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    best_fallback.unwrap_or_default()
}
