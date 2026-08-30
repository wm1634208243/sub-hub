use crate::api::auth_handlers::{save_user_config_to_disk, save_users_to_disk};
use crate::api::config_handlers::load_user_config;
use crate::models::{User, UserConfig};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::Path as FilePath;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSettings {
    #[serde(default = "default_enable")]
    pub enable_auto_backup: bool,
    #[serde(default = "default_interval")]
    pub interval_hours: u32,
    #[serde(default = "default_max_retention")]
    pub max_retention: u32,
    #[serde(default)]
    pub last_backup_time: Option<String>,
}

fn default_enable() -> bool {
    true
}
fn default_interval() -> u32 {
    24
}
fn default_max_retention() -> u32 {
    10
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enable_auto_backup: true,
            interval_hours: 24,
            max_retention: 10,
            last_backup_time: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupArchiveInfo {
    pub filename: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub trigger_type: String, // "auto" | "manual"
    pub users_count: usize,
}

pub async fn load_backup_settings(config_dir: &str) -> BackupSettings {
    let file = FilePath::new(config_dir).join("backup_settings.json");
    if let Ok(content) = tokio::fs::read_to_string(&file).await {
        if let Ok(settings) = serde_json::from_str::<BackupSettings>(&content) {
            return settings;
        }
    }
    BackupSettings::default()
}

pub async fn save_backup_settings(config_dir: &str, settings: &BackupSettings) -> Result<(), String> {
    let file = FilePath::new(config_dir).join("backup_settings.json");
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    tokio::fs::write(&file, json).await.map_err(|e| e.to_string())?;
    Ok(())
}

fn sanitize_backup_filename(name: &str) -> Result<String, String> {
    let clean = name.trim();
    if clean.is_empty() || clean.contains("..") || clean.contains('/') || clean.contains('\\') || !clean.ends_with(".json") {
        return Err("非法的文件名".into());
    }
    Ok(clean.to_string())
}

pub async fn create_backup_archive(
    config_dir: &str,
    state: &AppState,
    trigger: &str,
) -> Result<BackupArchiveInfo, String> {
    let backups_dir = FilePath::new(config_dir).join("backups");
    tokio::fs::create_dir_all(&backups_dir).await.map_err(|e| e.to_string())?;

    let now = chrono::Utc::now();
    let time_str = now.format("%Y%m%d_%H%M%S").to_string();
    let filename = format!("subhub_backup_{}_{}.json", time_str, trigger);
    let target_file = backups_dir.join(&filename);

    let users = state.users.read().await;
    let mut configs = serde_json::Map::new();

    for u in users.iter() {
        let cfg = load_user_config(config_dir, &u.username, &u.password_hash).await;
        configs.insert(u.username.clone(), serde_json::to_value(cfg).unwrap_or(serde_json::Value::Null));
    }

    let payload = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "exportedAt": now.to_rfc3339(),
        "trigger": trigger,
        "usersCount": users.len(),
        "users": *users,
        "configs": configs
    });

    let json_bytes = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    let size_bytes = json_bytes.len() as u64;

    tokio::fs::write(&target_file, &json_bytes).await.map_err(|e| e.to_string())?;

    // Update settings
    let mut settings = load_backup_settings(config_dir).await;
    settings.last_backup_time = Some(now.to_rfc3339());
    let _ = save_backup_settings(config_dir, &settings).await;

    // Prune old backups exceeding max_retention
    prune_old_backups(config_dir, settings.max_retention).await;

    tracing::info!("✅ 系统快照归档生成成功: {} ({} 字节, 触发方式: {})", filename, size_bytes, trigger);

    Ok(BackupArchiveInfo {
        filename,
        created_at: now.to_rfc3339(),
        size_bytes,
        trigger_type: trigger.to_string(),
        users_count: users.len(),
    })
}

pub async fn list_backup_archives(config_dir: &str) -> Vec<BackupArchiveInfo> {
    let backups_dir = FilePath::new(config_dir).join("backups");
    let mut list = Vec::new();

    if let Ok(mut entries) = tokio::fs::read_dir(&backups_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                let filename = entry.file_name().to_string_lossy().to_string();
                let metadata = entry.metadata().await.ok();
                let size_bytes = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = metadata.and_then(|m| m.modified().ok())
                    .map(|st| chrono::DateTime::<chrono::Utc>::from(st).to_rfc3339())
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

                let trigger_type = if filename.contains("_auto") { "auto" } else { "manual" }.to_string();

                // Extract timestamp from filename like subhub_backup_20260830_111136_manual.json
                let created_at = if let Some(stripped) = filename.strip_prefix("subhub_backup_") {
                    let parts: Vec<&str> = stripped.split('_').collect();
                    if parts.len() >= 2 {
                        let date_str = format!("{}_{}", parts[0], parts[1]);
                        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(&date_str, "%Y%m%d_%H%M%S") {
                            chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(dt, chrono::Utc).to_rfc3339()
                        } else {
                            modified
                        }
                    } else {
                        modified
                    }
                } else {
                    modified
                };

                list.push(BackupArchiveInfo {
                    filename,
                    created_at,
                    size_bytes,
                    trigger_type,
                    users_count: 1,
                });
            }
        }
    }

    // Sort newest first
    list.sort_by(|a, b| b.filename.cmp(&a.filename));
    list
}

pub async fn prune_old_backups(config_dir: &str, max_retention: u32) {
    let limit = max_retention.max(1) as usize;
    let list = list_backup_archives(config_dir).await;
    if list.len() > limit {
        let to_remove = &list[limit..];
        let backups_dir = FilePath::new(config_dir).join("backups");
        for item in to_remove {
            let path = backups_dir.join(&item.filename);
            let _ = tokio::fs::remove_file(path).await;
            tracing::info!("🧹 自动清理超期快照文件: {}", item.filename);
        }
    }
}

pub async fn restore_backup_archive(
    config_dir: &str,
    state: &AppState,
    filename: &str,
) -> Result<(), String> {
    let safe_name = sanitize_backup_filename(filename)?;
    let target_file = FilePath::new(config_dir).join("backups").join(safe_name);

    if !target_file.exists() {
        return Err("指定的备份快照文件不存在".into());
    }

    let content = tokio::fs::read_to_string(&target_file)
        .await
        .map_err(|e| format!("读取备份文件失败: {}", e))?;

    let payload = serde_json::from_str::<serde_json::Value>(&content)
        .map_err(|e| format!("解析备份文件失败: {}", e))?;

    if let Some(users_val) = payload.get("users") {
        if let Ok(new_users) = serde_json::from_value::<Vec<User>>(users_val.clone()) {
            let mut users = state.users.write().await;
            *users = new_users.clone();
            save_users_to_disk(config_dir, &new_users).await;
        }
    }

    if let Some(configs_val) = payload.get("configs").and_then(|v| v.as_object()) {
        for (uname, cfg_json) in configs_val {
            if let Ok(cfg) = serde_json::from_value::<UserConfig>(cfg_json.clone()) {
                save_user_config_to_disk(config_dir, uname, &cfg).await;
            }
        }
    }

    tracing::info!("🔄 已成功从快照 {} 还原全站数据", filename);
    Ok(())
}

pub async fn delete_backup_archive(config_dir: &str, filename: &str) -> Result<(), String> {
    let safe_name = sanitize_backup_filename(filename)?;
    let target_file = FilePath::new(config_dir).join("backups").join(safe_name);
    if target_file.exists() {
        tokio::fs::remove_file(target_file)
            .await
            .map_err(|e| format!("删除失败: {}", e))?;
    }
    Ok(())
}

pub fn spawn_backup_scheduler(config_dir: String, state: AppState) {
    tokio::spawn(async move {
        tracing::info!("⏰ 自动定时备份调度引擎已就绪，正在监听备份策略...");
        // Check every 5 minutes
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        loop {
            interval.tick().await;

            let settings = load_backup_settings(&config_dir).await;
            if !settings.enable_auto_backup {
                continue;
            }

            let interval_secs = (settings.interval_hours as i64) * 3600;
            let should_run = match &settings.last_backup_time {
                None => true,
                Some(time_str) => match chrono::DateTime::parse_from_rfc3339(time_str) {
                    Ok(last_dt) => {
                        let elapsed = chrono::Utc::now().signed_duration_since(last_dt.with_timezone(&chrono::Utc)).num_seconds();
                        elapsed >= interval_secs
                    }
                    Err(_) => true,
                },
            };

            if should_run {
                tracing::info!("🚀 触发自动定时备份 (周期: {} 小时)...", settings.interval_hours);
                if let Err(e) = create_backup_archive(&config_dir, &state, "auto").await {
                    tracing::error!("❌ 自动定时备份执行失败: {}", e);
                }
            }
        }
    });
}
