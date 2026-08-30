mod api;
pub mod backup;
mod engine;
mod models;
pub mod security;
mod static_files;

use api::auth_handlers::{
    admin_get_system_settings_handler, admin_save_system_settings_handler, change_password_handler,
    create_user_handler, delete_user_handler, list_users_handler, login_handler, logout_handler, me_handler,
    public_system_settings_handler, register_handler, reset_password_handler, user_role_handler,
    user_status_handler, AppState,
};
use api::config_handlers::{
    admin_backup_export_handler, admin_backup_restore_handler, admin_create_backup_archive_handler,
    admin_delete_backup_archive_handler, admin_download_backup_archive_handler, admin_get_backups_handler,
    admin_restore_backup_archive_handler, admin_save_backup_settings_handler, clear_access_logs_handler,
    compile_transient_handler, get_access_logs_handler, get_config_handler, inspect_nodes_handler,
    nodes_health_handler, preview_config_handler, preview_rename_handler, purge_config_handler,
    refresh_subscriptions_handler, regenerate_token_handler, save_config_handler, serve_rules_js_handler,
    set_token_expiry_handler, test_subscription_handler,
};
use api::sub_handlers::unified_sub_handler;
use api::system_handlers::{
    domain_test_handler, get_system_settings_handler, get_versions_handler, ssl_provision_handler,
    system_update_handler,
};
use axum::{
    response::Json,
    routing::{delete, get, post},
    Router,
};
use clap::Parser;
use engine::SubscriptionFetcher;
use models::User;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = 3000)]
    port: u16,

    #[arg(short, long, default_value = "./config")]
    config_dir: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "subhub=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    tokio::fs::create_dir_all(&args.config_dir).await?;

    // Load or initialize users (support multiple migration paths)
    let users_file = Path::new(&args.config_dir).join("users.json");
    let old_data_users = Path::new(&args.config_dir).join("../data/users.json");
    let old_root_data_users = Path::new("data/users.json");

    let mut initial_users = Vec::new();

    if users_file.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&users_file).await {
            if let Ok(parsed) = serde_json::from_str::<Vec<User>>(&content) {
                initial_users = parsed;
            }
        }
    }

    if initial_users.is_empty() && old_data_users.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&old_data_users).await {
            if let Ok(parsed) = serde_json::from_str::<Vec<User>>(&content) {
                initial_users = parsed;
                let _ = tokio::fs::write(&users_file, &content).await;
            }
        }
    }

    if initial_users.is_empty() && old_root_data_users.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&old_root_data_users).await {
            if let Ok(parsed) = serde_json::from_str::<Vec<User>>(&content) {
                initial_users = parsed;
                let _ = tokio::fs::write(&users_file, &content).await;
            }
        }
    }

    if initial_users.is_empty() {
        tracing::info!("Initializing default admin account (admin / admin)...");
        let hash = bcrypt::hash("admin", 10).unwrap();
        initial_users.push(User {
            username: "admin".into(),
            password_hash: hash,
            role: "admin".into(),
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            disabled: Some(false),
            disabled_until: None,
            disabled_reason: None,
        });
        let _ = tokio::fs::write(&users_file, serde_json::to_string_pretty(&initial_users)?).await;
    }

    let rate_limiter = security::RateLimiter::new();

    // Spawn background task to periodically clean up expired rate limiter entries
    let rl_clone = rate_limiter.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(600));
        loop {
            interval.tick().await;
            rl_clone.cleanup().await;
        }
    });

    let state = AppState {
        config_dir: args.config_dir.clone(),
        users: Arc::new(RwLock::new(initial_users)),
        sessions: Arc::new(RwLock::new(HashMap::new())),
        fetcher: Arc::new(SubscriptionFetcher::new()),
        rate_limiter,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Auth APIs
        .route("/api/login", post(login_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/register", post(register_handler))
        .route("/api/auth/register", post(register_handler))
        .route("/api/me", get(me_handler))
        .route("/api/auth/me", get(me_handler))
        .route("/api/logout", post(logout_handler))
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/change-password", post(change_password_handler))
        .route("/api/auth/change-password", post(change_password_handler))
        // Admin User Management APIs
        .route("/api/admin/users", get(list_users_handler).post(create_user_handler))
        .route("/api/admin/users/:username", delete(delete_user_handler))
        .route("/api/admin/users/:username/status", post(user_status_handler))
        .route("/api/admin/users/:username/role", post(user_role_handler))
        .route("/api/admin/users/:username/reset-password", post(reset_password_handler))
        // Config & Token APIs
        .route("/api/config", get(get_config_handler).post(save_config_handler))
        .route("/api/config/purge", delete(purge_config_handler))
        .route("/api/regenerate-token", post(regenerate_token_handler))
        .route("/api/set-token-expiry", post(set_token_expiry_handler))
        // Subscription Management APIs
        .route("/api/subscriptions/:id/nodes", get(inspect_nodes_handler))
        .route("/api/subscriptions/refresh", post(refresh_subscriptions_handler))
        .route("/api/subscriptions/test", post(test_subscription_handler))
        // Nodes Rename & Health APIs
        .route("/api/nodes/preview-rename", post(preview_rename_handler))
        .route("/api/nodes/health", post(nodes_health_handler))
        // Access Logs APIs
        .route("/api/access-log", get(get_access_logs_handler))
        .route("/api/access-log/clear", post(clear_access_logs_handler))
        // Subscription Multi-Format Distribution APIs
        .route("/api/sub", get(unified_sub_handler))
        .route("/api/subscription", get(unified_sub_handler))
        .route("/api/clash.yaml", get(unified_sub_handler))
        .route("/api/sing-box.json", get(unified_sub_handler))
        .route("/api/singbox", get(unified_sub_handler))
        .route("/api/sb.json", get(unified_sub_handler))
        .route("/api/surge.list", get(unified_sub_handler))
        .route("/api/surge", get(unified_sub_handler))
        .route("/api/base64", get(unified_sub_handler))
        .route("/api/sub.txt", get(unified_sub_handler))
        .route("/api/nodes.txt", get(unified_sub_handler))
        // JavaScript Override Script & Transient Compilation APIs
        .route("/api/rules.js", get(serve_rules_js_handler))
        .route("/api/js", get(serve_rules_js_handler))
        .route("/api/rules", get(serve_rules_js_handler))
        .route("/api/public/compile-transient", post(compile_transient_handler))
        .route("/api/preview", post(preview_config_handler))
        // System Settings & Versions & Backup APIs
        .route("/api/system/version", get(get_versions_handler))
        .route("/api/system/versions", get(get_versions_handler))
        .route("/api/system/update", post(system_update_handler))
        .route("/api/admin/backup/export", get(admin_backup_export_handler))
        .route("/api/admin/backup/restore", post(admin_backup_restore_handler))
        .route("/api/admin/backups", get(admin_get_backups_handler))
        .route("/api/admin/backup/settings", post(admin_save_backup_settings_handler))
        .route("/api/admin/backup/create", post(admin_create_backup_archive_handler))
        .route("/api/admin/backup/restore-file", post(admin_restore_backup_archive_handler))
        .route("/api/admin/backups/:filename", delete(admin_delete_backup_archive_handler))
        .route("/api/admin/backups/download/:filename", get(admin_download_backup_archive_handler))
        .route("/api/admin/system/settings", get(admin_get_system_settings_handler).post(admin_save_system_settings_handler))
        .route("/api/system/public-settings", get(public_system_settings_handler))
        .route("/api/admin/system/domain/test", post(domain_test_handler))
        .route("/api/admin/system/ssl/custom-cert", post(ssl_provision_handler))
        .route("/api/admin/system/ssl/generate-self-signed", post(ssl_provision_handler))
        .route("/api/admin/system/ssl/provision", post(ssl_provision_handler))
        .route("/api/system/settings", get(get_system_settings_handler))
        // Static Files fallback
        .fallback(static_files::static_handler)
        .layer(axum::middleware::from_fn(security::security_headers_middleware))
        .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Spawn Auto-Backup Scheduler Worker
    backup::spawn_backup_scheduler(args.config_dir.clone(), state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    println!("====================================================");
    println!("🦀 SubHub v{} (Rust Native Engine) 已启动", env!("CARGO_PKG_VERSION"));
    println!("🌐 Web 管理端: http://localhost:{}", args.port);
    println!("👤 默认账号: admin / admin");
    println!("====================================================");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
