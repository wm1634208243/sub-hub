use crate::models::{PublicUserView, User, UserConfig};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub config_dir: String,
    pub users: Arc<RwLock<Vec<User>>>,
    pub sessions: Arc<RwLock<std::collections::HashMap<String, String>>>, // token -> username
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

async fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<User, (StatusCode, Json<serde_json::Value>)> {
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
    let user = users.iter().find(|u| u.username.to_lowercase() == uname.to_lowercase()).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "用户不存在" })),
        )
    })?;

    Ok(user.clone())
}

async fn check_admin(state: &AppState, headers: &HeaderMap) -> Result<User, (StatusCode, Json<serde_json::Value>)> {
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

    let mut sessions = state.sessions.write().await;
    sessions.insert(token.clone(), user.username.clone());

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

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "注册成功，请登录"
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
    sessions.retain(|_, u| u.to_lowercase() != curr_user.username.to_lowercase());

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

    // Delete user config file if exists
    let cfg_file = std::path::Path::new(&state.config_dir).join(format!("user_{}.json", target_username.to_lowercase()));
    let _ = tokio::fs::remove_file(cfg_file).await;

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

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("用户【{}】密码已重置", target_username)
    })))
}

pub async fn admin_get_system_settings_handler(
    State(_state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&_state, &headers).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "customDomain": "",
        "enableHttpsRedirect": false
    })))
}

pub async fn admin_save_system_settings_handler(
    State(_state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    check_admin(&_state, &headers).await?;
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "系统设置已更新",
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
    let file = std::path::Path::new(config_dir).join(format!("user_{}.json", username.to_lowercase()));
    if let Ok(content) = serde_json::to_string_pretty(config) {
        let _ = tokio::fs::write(&file, content).await;
    }
    let configs_file = std::path::Path::new(config_dir).join("configs").join(format!("{}.json", username));
    let _ = tokio::fs::create_dir_all(std::path::Path::new(config_dir).join("configs")).await;
    if let Ok(content) = serde_json::to_string_pretty(config) {
        let _ = tokio::fs::write(&configs_file, content).await;
    }
}
