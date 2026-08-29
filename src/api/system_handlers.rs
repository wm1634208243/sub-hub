use axum::response::Json;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_notes: String,
    pub history: Vec<serde_json::Value>,
}

pub async fn get_versions_handler() -> Json<VersionInfo> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let history = vec![
        serde_json::json!({
            "version": "2.0.0",
            "tag": "v2.0.0",
            "name": "SubHub v2.0.0 · 纯 Rust 高性能架构重构版",
            "publishedAt": "2026-08-30T00:50:00Z",
            "highlights": [
                "🦀 全面重构为 Rust 原生单二进制高性能架构",
                "💾 常驻内存暴降至 3MB~5MB，微秒级极速响应",
                "🎯 节点明细抽屉支持逐个节点自由打勾/排除「⚡ 参与优选」",
                "🌐 独立优选定制卡片：常用地区 (港/日/新/美) 与关键词包含/排除"
            ],
            "changelogZh": "SubHub 全面升级为 Rust 原生单文件架构，零运行时依赖，性能与资源占用极致飞跃！"
        })
    ];

    Json(VersionInfo {
        current_version: current_version.clone(),
        latest_version: current_version,
        has_update: false,
        release_notes: "当前已是最新 Rust 高性能稳定版".into(),
        history,
    })
}

pub async fn get_system_settings_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "serverPort": 3000,
        "allowRegistration": true,
        "runtime": "Rust (tokio/axum) High Performance Engine"
    }))
}
