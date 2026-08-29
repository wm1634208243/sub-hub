use crate::models::{User, UserConfig};
use axum::{
    extract::State,
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
pub struct ChangePwdPayload {
    pub old_password: String,
    pub new_password: String,
}

pub async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginPayload>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let uname = payload.username.trim().to_lowercase();
    let users = state.users.read().await;

    let user = users.iter().find(|u| u.username.to_lowercase() == uname);
    let user = match user {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "用户名或密码错误" })),
            ));
        }
    };

    if user.disabled.unwrap_or(false) {
        let msg = user.disabled_reason.clone().unwrap_or_else(|| "账号已被封禁".into());
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": format!("该账号已被禁用: {}", msg) })),
        ));
    }

    let is_valid = bcrypt::verify(&payload.password, &user.password_hash).unwrap_or(false);
    if !is_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "用户名或密码错误" })),
        ));
    }

    use rand::Rng;
    let token: String = (0..32)
        .map(|_| format!("{:02x}", rand::thread_rng().gen::<u8>()))
        .collect();

    let mut sessions = state.sessions.write().await;
    sessions.insert(token.clone(), user.username.clone());

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
    if uname.len() < 3 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "用户名至少 3 个字符" })),
        ));
    }
    if payload.password.len() < 6 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "密码至少 6 个字符" })),
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
    let user = users.iter().find(|u| u.username == *uname).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "用户不存在" })),
        )
    })?;

    Ok(Json(serde_json::json!({
        "success": true,
        "username": user.username,
        "role": user.role
    })))
}

pub async fn change_password_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ChangePwdPayload>,
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

    let mut users = state.users.write().await;
    let user = users.iter_mut().find(|u| u.username == *uname).ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "用户不存在" })),
        )
    })?;

    let is_valid = bcrypt::verify(&payload.old_password, &user.password_hash).unwrap_or(false);
    if !is_valid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "原密码错误" })),
        ));
    }

    if payload.new_password.len() < 6 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "新密码至少 6 个字符" })),
        ));
    }

    user.password_hash = bcrypt::hash(&payload.new_password, 10).unwrap();
    save_users_to_disk(&state.config_dir, &users).await;

    Ok(Json(serde_json::json!({ "success": true, "message": "密码修改成功" })))
}

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
}
