mod api;
mod engine;
mod models;
mod static_files;

use api::auth_handlers::{
    change_password_handler, login_handler, me_handler, register_handler, AppState,
};
use api::config_handlers::{get_config_handler, inspect_nodes_handler, save_config_handler};
use api::sub_handlers::unified_sub_handler;
use api::system_handlers::{get_system_settings_handler, get_versions_handler};
use axum::{
    routing::{get, post},
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

    // Load or initialize users
    let users_file = Path::new(&args.config_dir).join("users.json");
    let mut initial_users = Vec::new();
    if users_file.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&users_file).await {
            if let Ok(parsed) = serde_json::from_str::<Vec<User>>(&content) {
                initial_users = parsed;
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

    let state = AppState {
        config_dir: args.config_dir.clone(),
        users: Arc::new(RwLock::new(initial_users)),
        sessions: Arc::new(RwLock::new(HashMap::new())),
        fetcher: Arc::new(SubscriptionFetcher::new()),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        // Auth APIs
        .route("/api/login", post(login_handler))
        .route("/api/register", post(register_handler))
        .route("/api/me", get(me_handler))
        .route("/api/change-password", post(change_password_handler))
        // Config APIs
        .route("/api/config", get(get_config_handler).post(save_config_handler))
        .route("/api/subscriptions/:id/nodes", get(inspect_nodes_handler))
        // Subscription Distribution APIs
        .route("/api/sub", get(unified_sub_handler))
        .route("/api/clash.yaml", get(unified_sub_handler))
        .route("/api/sing-box.json", get(unified_sub_handler))
        .route("/api/surge.list", get(unified_sub_handler))
        .route("/api/base64", get(unified_sub_handler))
        // System APIs
        .route("/api/system/versions", get(get_versions_handler))
        .route("/api/system/settings", get(get_system_settings_handler))
        // Static Files fallback
        .fallback(static_files::static_handler)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

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
