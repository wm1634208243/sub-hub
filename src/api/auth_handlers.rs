use crate::models::{PublicUserView, User, UserConfig};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionInfo {
    pub username: String,
    pub created_at: u64,
    pub expires_at: u64,
}

#[derive(Clone)]
pub struct AppState {
    pub config_dir: String,
    pub users: Arc<RwLock<Vec<User>>>,
    pub sessions: Arc<RwLock<std::collections::HashMap<String, SessionInfo>>>, // token -> SessionInfo
    pub fetcher: Arc<crate::engine::SubscriptionFetcher>,
    pub rate_limiter: crate::security::RateLimiter,
}

#[derive(Deserialize)]
pub struct LoginPayload {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct RegisterPayload {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct CreateUserPayload {
    pub username: String,
    pub password: String,
    #[serde(default = "default_role")]
    pub role: String,
}

fn default_role() -> String {
    "user".to_string()
}

#[derive(Deserialize)]
pub struct ChangePwdPayload {
    #[serde(alias = "oldPassword", alias = "old_password")]
    pub old_password: String,
    #[serde(alias = "newPassword", alias = "new_password")]
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct ResetPwdPayload {
    #[serde(alias = "newPassword", alias = "new_password", alias = "password")]
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct UpdateRolePayload {
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateStatusPayload {
    #[serde(default)]
    pub disabled: bool,
    #[serde(alias = "disabledUntil", alias = "disabled_until")]
    pub disabled_until: Option<String>,
    #[serde(alias = "durationMinutes", alias = "duration_minutes")]
    pub duration_minutes: Option<i64>,
    pub reason: Option<String>,
}

// ── Auth Helpers ─────────────────────────────────────────────────────────────

pub async fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<User, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let token = auth_header.strip_prefix("Bearer ").unwrap_or_default().trim();

    let sessions = state.sessions.read().await;
    let session = sessions.get(token).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "未登录或会话已过期" })),
        )
    })?;

    let now = chrono::Utc::now().timestamp() as u64;
    if now > session.expires_at {
        drop(sessions);
        let mut write_sessions = state.sessions.write().await;
        write_sessions.remove(token);
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "会话已过期，请重新登录" })),
        ));
    }
    let uname = session.username.clone();
    drop(sessions);

    let users = state.users.read().await;
    let user = users.iter().find(|u| u.username.to_lowercase() == uname.to_lowercase()).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "用户不存在" })),
        )
    })?;

    Ok(user.clone())
}

pub async fn check_admin(state: &AppState, headers: &HeaderMap) -> Result<User, (StatusCode, Json<serde_json::Value>)> {
    let user = check_auth(state, headers).await?;
    if user.role != "admin" {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "权限不足，仅管理员可操作" })),
        ));
    }
    Ok(user)
}

// ── Auth APIs ────────────────────────────────────────────────────────────────

pub async fn login_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let client_ip = crate::security::extract_client_ip(&headers);
    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let ip_key = format!("login_ip:{}", client_ip);
    let uname = payload.username.trim().to_lowercase();
    let user_key = format!("login_user:{}", uname);

    // 1. Check IP and Username Rate Limits (Anti-Brute Force)
    if let Err(remaining) = state.rate_limiter.check(&ip_key).await {
        let secs = remaining.as_secs().max(1);
        crate::api::config_handlers::record_access_log(
            &state.config_dir,
            &uname,
            &client_ip,
            ua,
            "🚫 登录爆破拦截",
            429,
            &format!("该 IP 密码错误次数过多，触发系统防护，剩余锁定 {} 秒", secs),
        ).await;

        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": format!("密码错误次数过多，IP已被安全锁定！请等待 {} 秒后再试", secs)
            })),
        ));
    }

    if let Err(remaining) = state.rate_limiter.check(&user_key).await {
        let secs = remaining.as_secs().max(1);
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": format!("该账号已被防爆破临时锁定！请等待 {} 秒后再试", secs)
            })),
        ));
    }

    // 2. Lookup User
    let users = state.users.read().await;
    let user = users.iter().find(|u| u.username.to_lowercase() == uname);

    let (is_valid, user_data) = match user {
        Some(u) => {
            if u.disabled.unwrap_or(false) {
                let msg = u.disabled_reason.clone().unwrap_or_else(|| "账号已被封禁".into());
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(serde_json::json!({ "error": format!("该账号已被禁用: {}", msg) })),
                ));
            }
            let valid = bcrypt::verify(&payload.password, &u.password_hash).unwrap_or(false);
            (valid, Some(u.clone()))
        }
        None => {
            // Timing-attack mitigation: execute dummy bcrypt verify so response time is identical
            crate::security::dummy_bcrypt_verify(&payload.password);
            (false, None)
        }
    };

    // 3. Handle Failure (5 failed attempts within 15 min -> 15 min lock)
    if !is_valid || user_data.is_none() {
        use std::time::Duration;
        let lock_ip = state.rate_limiter.record_failure(
            &ip_key,
            5,
            Duration::from_secs(900),
            Duration::from_secs(900),
        ).await;

        let _ = state.rate_limiter.record_failure(
            &user_key,
            5,
            Duration::from_secs(900),
            Duration::from_secs(900),
        ).await;

        let detail = if let Some(lock_dur) = lock_ip {
            format!("连续 5 次密码错误，已自动触发 IP 封锁 {} 秒", lock_dur.as_secs())
        } else {
            "用户名或密码错误".to_string()
        };

        crate::api::config_handlers::record_access_log(
            &state.config_dir,
            &uname,
            &client_ip,
            ua,
            "❌ 登录失败",
            401,
            &detail,
        ).await;

        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "用户名或密码错误" })),
        ));
    }

    let user = user_data.unwrap();

    // 4. Success -> Reset failed counters
    state.rate_limiter.record_success(&ip_key).await;
    state.rate_limiter.record_success(&user_key).await;

    use rand::Rng;
    let token: String = (0..32)
        .map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>()))
        .collect();

    let now = chrono::Utc::now().timestamp() as u64;
    let expires_at = now + 7 * 86400; // 7-day session lifetime

    let mut sessions = state.sessions.write().await;
    sessions.insert(token.clone(), SessionInfo {
        username: user.username.clone(),
        created_at: now,
        expires_at,
    });

    crate::api::config_handlers::record_access_log(
        &state.config_dir,
        &user.username,
        &client_ip,
        ua,
        "🔑 账号登录成功",
        200,
        "身份鉴权通过，颁发安全 Session 令牌",
    ).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "token": token,
        "username": user.username,
        "role": user.role
    })))
}

pub async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<RegisterPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let uname = payload.username.trim().to_string();
    if let Err(msg) = crate::security::validate_username(&uname) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        ));
    }
    if uname.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "用户名至少 3 个字符" })),
        ));
    }
    if payload.password.len() < 4 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "密码至少 4 个字符" })),
        ));
    }

    let mut users = state.users.write().await;
    if users.iter().any(|u| u.username.to_lowercase() == uname.to_lowercase()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "用户名已存在" })),
        ));
    }

    let is_first_user = users.is_empty();
    let role = if is_first_user { "admin" } else { "user" };

    let password_hash = bcrypt::hash(&payload.password, 10).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("密码哈希失败: {}", e) })),
        )
    })?;

    let new_user = User {
        username: uname.clone(),
        password_hash,
        role: role.to_string(),
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        disabled: Some(false),
        disabled_until: None,
        disabled_reason: None,
    };

    users.push(new_user);
    save_users_to_disk(&state.config_dir, &users).await;

    let user_cfg = UserConfig::default();
    save_user_config_to_disk(&state.config_dir, &uname, &user_cfg).await;

    use rand::Rng;
    let token: String = (0..32)
        .map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>()))
        .collect();

    let now = chrono::Utc::now().timestamp() as u64;
    let expires_at = now + 7 * 86400; // 7-day session lifetime

    let mut sessions = state.sessions.write().await;
    sessions.insert(token.clone(), SessionInfo {
        username: uname.clone(),
        created_at: now,
        expires_at,
    });

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "注册成功并已自动登录",
        "token": token,
        "username": uname,
        "role": role
    })))
}

pub async fn me_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let user = check_auth(&state, &headers).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "username": user.username,
        "role": user.role
    })))
}

pub async fn logout_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let token = auth_header.strip_prefix("Bearer ").unwrap_or_default().trim();
    if !token.is_empty() {
        let mut sessions = state.sessions.write().await;
        sessions.remove(token);
    }
    Ok(Json(serde_json::json!({ "success": true, "message": "已成功退出登录" })))
}

pub async fn change_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePwdPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let curr_user = check_auth(&state, &headers).await?;

    let is_valid = bcrypt::verify(&payload.old_password, &curr_user.password_hash).unwrap_or(false);
    if !is_valid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "原密码错误" })),
        ));
    }

    if payload.new_password.len() < 4 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "新密码至少 4 个字符" })),
        ));
    }

    // 1. Load user config with the OLD secret BEFORE updating hash!
    let user_cfg = crate::api::config_handlers::load_user_config(&state.config_dir, &curr_user.username, &curr_user.password_hash).await;

    let new_hash = bcrypt::hash(&payload.new_password, 10).unwrap();

    let mut users = state.users.write().await;
    if let Some(u) = users.iter_mut().find(|u| u.username.to_lowercase() == curr_user.username.to_lowercase()) {
        u.password_hash = new_hash;
    }
    save_users_to_disk(&state.config_dir, &users).await;

    // 2. Persist the user config immediately so it's safely updated and plaintext!
    save_user_config_to_disk(&state.config_dir, &curr_user.username, &user_cfg).await;

    // 3. Invalidate existing sessions for this user
    let mut sessions = state.sessions.write().await;
    sessions.retain(|_, u| u.username.to_lowercase() != curr_user.username.to_lowercase());

    Ok(Json(serde_json::json!({ "success": true, "message": "密码修改成功" })))
}

// ── Admin User Management APIs ────────────────────────────────────────────────

pub async fn list_users_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<PublicUserView>>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&state, &headers).await?;

    let users = state.users.read().await;
    let list = users.iter().map(|u| PublicUserView {
        username: u.username.clone(),
        role: u.role.clone(),
        created_at: u.created_at.clone(),
        disabled: u.disabled.unwrap_or(false),
        disabled_until: u.disabled_until.clone(),
        disabled_reason: u.disabled_reason.clone(),
    }).collect();

    Ok(Json(list))
}

pub async fn create_user_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CreateUserPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&state, &headers).await?;

    let uname = payload.username.trim().to_string();
    if let Err(msg) = crate::security::validate_username(&uname) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        ));
    }
    if uname.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "用户名至少 3 个字符" })),
        ));
    }
    if payload.password.len() < 4 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "密码至少 4 个字符" })),
        ));
    }

    let mut users = state.users.write().await;
    if users.iter().any(|u| u.username.to_lowercase() == uname.to_lowercase()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "用户名已存在" })),
        ));
    }

    let password_hash = bcrypt::hash(&payload.password, 10).unwrap();
    let new_user = User {
        username: uname.clone(),
        password_hash,
        role: if payload.role == "admin" { "admin".into() } else { "user".into() },
        created_at: Some(chrono::Utc::now().to_rfc3339()),
        disabled: Some(false),
        disabled_until: None,
        disabled_reason: None,
    };

    users.push(new_user);
    save_users_to_disk(&state.config_dir, &users).await;

    let user_cfg = UserConfig::default();
    save_user_config_to_disk(&state.config_dir, &uname, &user_cfg).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("用户 {} 创建成功", uname)
    })))
}

pub async fn delete_user_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_username): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let curr_user = check_admin(&state, &headers).await?;

    if let Err(msg) = crate::security::validate_username(&target_username) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        ));
    }

    if curr_user.username.to_lowercase() == target_username.to_lowercase() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "无法删除当前登录的管理员自身" })),
        ));
    }

    let mut users = state.users.write().await;
    let initial_len = users.len();
    users.retain(|u| u.username.to_lowercase() != target_username.to_lowercase());

    if users.len() == initial_len {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "用户不存在" })),
        ));
    }

    save_users_to_disk(&state.config_dir, &users).await;

    // Delete user config files across all candidate directories
    let uname_lower = target_username.to_lowercase();
    let _ = tokio::fs::remove_file(std::path::Path::new(&state.config_dir).join(format!("user_{}.json", uname_lower))).await;
    let _ = tokio::fs::remove_file(std::path::Path::new(&state.config_dir).join("configs").join(format!("{}.json", uname_lower))).await;
    let _ = tokio::fs::remove_file(std::path::Path::new(&state.config_dir).join(format!("{}.json", uname_lower))).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("用户 {} 已删除", target_username)
    })))
}

pub async fn user_status_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_username): Path<String>,
    Json(payload): Json<UpdateStatusPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let curr_user = check_admin(&state, &headers).await?;

    if let Err(msg) = crate::security::validate_username(&target_username) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        ));
    }

    if curr_user.username.to_lowercase() == target_username.to_lowercase() && payload.disabled {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "无法禁用当前管理员账号" })),
        ));
    }

    let mut users = state.users.write().await;
    let user = users.iter_mut().find(|u| u.username.to_lowercase() == target_username.to_lowercase()).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "用户不存在" })),
        )
    })?;

    user.disabled = Some(payload.disabled);

    if payload.disabled {
        if let Some(until) = payload.disabled_until {
            user.disabled_until = Some(until);
        } else if let Some(mins) = payload.duration_minutes {
            let until = chrono::Utc::now() + chrono::Duration::minutes(mins);
            user.disabled_until = Some(until.to_rfc3339());
        } else {
            user.disabled_until = None; // permanent
        }
        user.disabled_reason = payload.reason;
    } else {
        user.disabled_until = None;
        user.disabled_reason = None;
    }

    save_users_to_disk(&state.config_dir, &users).await;

    // Invalidate sessions for banned user
    if payload.disabled {
        let mut sessions = state.sessions.write().await;
        sessions.retain(|_, s| s.username.to_lowercase() != target_username.to_lowercase());
    }

    let msg = if payload.disabled {
        format!("已成功禁用用户【{}】", target_username)
    } else {
        format!("已成功解禁用户【{}】", target_username)
    };

    Ok(Json(serde_json::json!({ "success": true, "message": msg })))
}

pub async fn user_role_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_username): Path<String>,
    Json(payload): Json<UpdateRolePayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let curr_user = check_admin(&state, &headers).await?;

    if let Err(msg) = crate::security::validate_username(&target_username) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        ));
    }

    if curr_user.username.to_lowercase() == target_username.to_lowercase() && payload.role != "admin" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "无法取消当前登录管理员的 admin 身份" })),
        ));
    }

    let new_role = if payload.role == "admin" { "admin".to_string() } else { "user".to_string() };

    let mut users = state.users.write().await;
    let user = users.iter_mut().find(|u| u.username.to_lowercase() == target_username.to_lowercase()).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "用户不存在" })),
        )
    })?;

    user.role = new_role.clone();
    save_users_to_disk(&state.config_dir, &users).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("已成功更新【{}】权限为 {}", target_username, new_role)
    })))
}

pub async fn reset_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_username): Path<String>,
    Json(payload): Json<ResetPwdPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&state, &headers).await?;

    if let Err(msg) = crate::security::validate_username(&target_username) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        ));
    }

    if payload.new_password.len() < 4 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "新密码至少 4 个字符" })),
        ));
    }

    let mut users = state.users.write().await;
    let user = users.iter_mut().find(|u| u.username.to_lowercase() == target_username.to_lowercase()).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "用户不存在" })),
        )
    })?;

    let old_hash = user.password_hash.clone();
    let user_cfg = crate::api::config_handlers::load_user_config(&state.config_dir, &target_username, &old_hash).await;

    user.password_hash = bcrypt::hash(&payload.new_password, 10).unwrap();
    save_users_to_disk(&state.config_dir, &users).await;

    save_user_config_to_disk(&state.config_dir, &target_username, &user_cfg).await;

    // Invalidate existing sessions for this user
    let mut sessions = state.sessions.write().await;
    sessions.retain(|_, s| s.username.to_lowercase() != target_username.to_lowercase());

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("用户【{}】密码已重置", target_username)
    })))
}

pub async fn admin_reset_user_config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(target_username): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&state, &headers).await?;

    let clean_uname = target_username.trim().to_string();
    let mut empty_cfg = UserConfig::default();
    empty_cfg.subscriptions = Vec::new();
    empty_cfg.subscription_token = crate::models::default_token();
    save_user_config_to_disk(&state.config_dir, &clean_uname, &empty_cfg).await;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("用户【{}】订阅与节点配置已成功清空归零！", clean_uname)
    })))
}

pub async fn admin_get_system_settings_handler(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&_state, &headers).await?;
    let settings = crate::models::SystemSettings::default();
    Ok(Json(serde_json::to_value(settings).unwrap_or(serde_json::Value::Null)))
}

pub async fn admin_save_system_settings_handler(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<crate::models::SystemSettings>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&_state, &headers).await?;
    let path = std::path::Path::new(&_state.config_dir).join("system_settings.json");
    if let Ok(content) = serde_json::to_string_pretty(&payload) {
        let _ = tokio::fs::write(&path, content).await;
    }
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "系统设置保存成功",
        "settings": payload
    })))
}

pub async fn public_system_settings_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "success": true,
        "customDomain": "",
        "enableHttpsRedirect": false
    }))
}

// ── Persistence Helpers ──────────────────────────────────────────────────────

pub async fn save_users_to_disk(config_dir: &str, users: &[User]) {
    let file = std::path::Path::new(config_dir).join("users.json");
    if let Ok(content) = serde_json::to_string_pretty(users) {
        let _ = tokio::fs::write(&file, content).await;
    }
}

pub async fn save_user_config_to_disk(config_dir: &str, username: &str, config: &UserConfig) {
    let uname_lower = username.trim().to_lowercase();
    let users_file = std::path::Path::new(config_dir).join("users.json");
    let mut user_secret = None;
    if let Ok(content) = tokio::fs::read_to_string(&users_file).await {
        if let Ok(users) = serde_json::from_str::<Vec<User>>(&content) {
            if let Some(u) = users.iter().find(|u| u.username.to_lowercase() == uname_lower) {
                user_secret = Some(u.password_hash.clone());
            }
        }
    }

    let config_val = serde_json::to_value(config).unwrap_or(serde_json::Value::Null);
    let to_write = if let Some(secret) = user_secret {
        if let Ok(encrypted_bundle) = crate::engine::crypto::encrypt_user_config_bundle(&config_val, &secret, &uname_lower) {
            serde_json::to_string_pretty(&encrypted_bundle).unwrap_or_else(|_| serde_json::to_string_pretty(config).unwrap_or_default())
        } else {
            serde_json::to_string_pretty(config).unwrap_or_default()
        }
    } else {
        serde_json::to_string_pretty(config).unwrap_or_default()
    };

    let file = std::path::Path::new(config_dir).join(format!("user_{}.json", uname_lower));
    let _ = tokio::fs::write(&file, &to_write).await;

    let configs_dir = std::path::Path::new(config_dir).join("configs");
    let _ = tokio::fs::create_dir_all(&configs_dir).await;
    let configs_file = configs_dir.join(format!("{}.json", username));
    let _ = tokio::fs::write(&configs_file, &to_write).await;
}
