use crate::api::auth_handlers::AppState;
use crate::api::config_handlers::{load_user_config, record_access_log};
use crate::engine::aggregator::aggregate_clash_yaml;
use crate::engine::format_converter::{convert_to_base64, convert_to_singbox_json, convert_to_surge_list, detect_client_target};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct SubQuery {
    pub token: Option<String>,
    pub target: Option<String>,
}

pub async fn unified_sub_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SubQuery>,
) -> Response {
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let ip = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or(s).trim())
        .or_else(|| headers.get("x-real-ip").and_then(|v| v.to_str().ok()))
        .unwrap_or("127.0.0.1");

    let token = query.token.or_else(|| {
        headers.get("authorization")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.strip_prefix("Bearer ").unwrap_or(s).trim().to_string())
    });

    let token = match token {
        Some(t) if !t.is_empty() => t,
        _ => {
            record_access_log(&state.config_dir, "admin", ip, ua, "🌐 订阅请求", 401, "Token 缺失被拦截").await;
            return (StatusCode::UNAUTHORIZED, "Token 缺失").into_response();
        }
    };

    let users = state.users.read().await;
    let mut matched_user = None;
    let mut matched_cfg = None;

    for u in users.iter() {
        let cfg = load_user_config(&state.config_dir, &u.username, &u.password_hash).await;
        if cfg.subscription_token == token {
            matched_user = Some(u.clone());
            matched_cfg = Some(cfg);
            break;
        }
    }

    // If not matched in memory users, search through all json files in config and data directory!
    if matched_cfg.is_none() {
        let dirs_to_check = [
            format!("{}/configs", state.config_dir),
            state.config_dir.clone(),
            "data/configs".to_string(),
            "data".to_string(),
        ];

        for d in dirs_to_check {
            if let Ok(mut entries) = tokio::fs::read_dir(&d).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let fname = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                        let uname = fname.strip_prefix("user_").unwrap_or(fname);
                        let cfg = load_user_config(&state.config_dir, uname, "subhub_master_secret_fallback_v1").await;
                        if cfg.subscription_token == token {
                            matched_cfg = Some(cfg);
                            break;
                        }
                    }
                }
            }
            if matched_cfg.is_some() {
                break;
            }
        }
    }

    let cfg = match matched_cfg {
        Some(c) => c,
        None => {
            record_access_log(&state.config_dir, "admin", ip, ua, "🌐 订阅请求", 401, "无效 Token 拒绝访问").await;
            return (StatusCode::UNAUTHORIZED, "无效的订阅 Token").into_response();
        }
    };

    let username = matched_user.as_ref().map(|u| u.username.as_str()).unwrap_or("admin");

    if let Some(user) = &matched_user {
        if user.disabled.unwrap_or(false) {
            record_access_log(&state.config_dir, username, ip, ua, "🌐 订阅请求", 403, "账号已被禁用，暂停下发").await;
            return (StatusCode::FORBIDDEN, "该账号已被禁用，订阅已暂停下发").into_response();
        }
    }

    // Determine target format
    let target = query.target.as_deref().unwrap_or_else(|| detect_client_target(ua));

    match aggregate_clash_yaml(&cfg, &state.fetcher).await {
        Ok(agg) => {
            let mut res = match target {
                "singbox" => {
                    let json_str = convert_to_singbox_json(&agg.proxies);
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
                        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"sing-box.json\"")
                        .header("profile-update-interval", "24")
                        .body(axum::body::Body::from(json_str))
                        .unwrap()
                }
                "surge" => {
                    let list_str = convert_to_surge_list(&agg.proxies);
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"surge.list\"")
                        .header("profile-update-interval", "24")
                        .body(axum::body::Body::from(list_str))
                        .unwrap()
                }
                "base64" => {
                    let b64_str = convert_to_base64(&agg.proxies);
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"nodes.txt\"")
                        .header("profile-update-interval", "24")
                        .body(axum::body::Body::from(b64_str))
                        .unwrap()
                }
                _ => {
                    // Default Clash YAML
                    Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/yaml; charset=utf-8")
                        .header(header::CONTENT_DISPOSITION, "attachment; filename=\"config.yaml\"")
                        .header("profile-update-interval", "24")
                        .header("profile-web-page-url", "https://github.com/wm1634208243/sub-hub")
                        .body(axum::body::Body::from(agg.yaml))
                        .unwrap()
                }
            };

            if let Some(ui) = agg.user_info {
                if let Ok(val) = HeaderValue::from_str(&ui) {
                    res.headers_mut().insert("subscription-userinfo", val);
                }
            }

            // Record successful access log
            let type_label = match target {
                "singbox" => "📦 Sing-Box 原生 JSON",
                "surge" => "⚡ Surge 策略列表",
                "base64" => "🔗 Base64 单节点列表",
                _ => "🌟 Clash YAML 订阅",
            };

            record_access_log(
                &state.config_dir,
                username,
                ip,
                ua,
                type_label,
                200,
                &format!("成功下发 {} 个聚合节点", agg.total_nodes),
            ).await;

            res
        }
        Err(e) => {
            record_access_log(&state.config_dir, username, ip, ua, "🌐 订阅构建", 500, &format!("生成订阅失败: {}", e)).await;
            (StatusCode::INTERNAL_SERVER_ERROR, format!("生成订阅失败: {}", e)).into_response()
        }
    }
}
