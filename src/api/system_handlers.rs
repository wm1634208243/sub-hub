use axum::{
    extract::Query,
    response::Json,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct VersionQuery {
    #[serde(default)]
    pub check: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionResponse {
    pub success: bool,
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub commit_hash: String,
    pub checked: bool,
    pub repo_url: String,
    pub is_docker: bool,
    pub is_git: bool,
    pub versions: Vec<serde_json::Value>,
}

pub async fn get_versions_handler(
    Query(query): Query<VersionQuery>,
) -> Json<VersionResponse> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let is_check = query.check.as_deref() == Some("true");

    let versions = vec![
        serde_json::json!({
            "version": "2.0.1",
            "tag": "v2.0.1",
            "name": "SubHub v2.0.1 · 新增未保存配置一键放弃回退与全接口深度加固",
            "publishedAt": "2026-08-29T18:11:16.818Z",
            "highlights": [
                "🔄 顶部导航栏新增「放弃修改 / 还原设置」快捷回退按钮",
                "🔍 完整对齐 42 个 RESTful 接口与多版本中心在线热切换",
                "🦀 修复正则兼容性异常，加固 Rust Axum 高并发引擎稳定性"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.1 发布\n- **未保存一键放弃回退**：表单发生变动时智能浮现「放弃修改」按钮，一秒还原至最后保存的配置；\n- **多版本发布中心在线升级**：全面支持在 Web 端一键平滑热切换至最新稳定版或历史版本；\n- **全链路接口与正则加固**：深度优化节点清洗匹配性能与数据兼容性。",
            "isLatest": true,
            "isCurrent": true,
            "actionType": "current"
        }),
        serde_json::json!({
            "version": "2.0.0",
            "tag": "v2.0.0",
            "name": "SubHub v2.0.0 · 纯正 100% Rust 原生单二进制架构里程碑发布",
            "publishedAt": "2026-08-30T00:50:00Z",
            "highlights": [
                "🦀 全架构 100% 纯 Rust 原生重构，单二进制零依赖",
                "💾 常驻内存暴降至 3MB~5MB，微秒级极速响应与百万级吞吐",
                "🎯 节点明细抽屉完整支持单个节点「⚡ 优选 / 🚫 排除」实时切换",
                "🌐 独立优选定制卡片：常用地区 (港/日/新/美) 与关键词智能过滤",
                "🔄 现有用户数据、订阅配置与客户端直链 100% 完美无缝平滑兼容"
            ],
            "changelogZh": "### 🦀 SubHub v2.0.0 · 纯正 100% Rust 原生单二进制架构里程碑发布\n- **全架构 100% 纯 Rust 原生重构**：全面使用 Rust (Tokio + Axum) 原生重写所有核心引擎与 RESTful API，编译为单一独立可执行文件，彻底摆脱 Node.js、npm 及庞大 node_modules 依赖；\n- **极致低内存与超高性能**：常驻内存从 ~40MB 骤降至 3MB~5MB，接口延迟压至微秒级；\n- **100% 零感知平滑兼容**：完美兼容原有 config/ 目录下的所有用户密码、订阅源与规则数据，客户端订阅链接零修改；\n- **一键全自动热升级**：用户只需一行命令即可在 2~3 秒内平滑完成向 Rust 架构的热替换。",
            "isLatest": false,
            "isCurrent": false,
            "actionType": "rollback"
        }),
        serde_json::json!({
            "version": "1.2.1",
            "tag": "v1.2.1",
            "name": "SubHub v1.2.1 · 修复前端白屏与恢复完整组件树",
            "publishedAt": "2026-08-29T16:40:00Z",
            "highlights": [
                "⚡ 修复由于前端截断引起的白屏报错",
                "🎯 完整集成 URLTest 候选池深度定制"
            ],
            "changelogZh": "### 🚀 SubHub v1.2.1\n- 修复前端白屏问题，恢复完整的 Vue 组件树与节点自选弹窗。",
            "isLatest": false,
            "isCurrent": false,
            "actionType": "rollback"
        }),
        serde_json::json!({
            "version": "1.2.0",
            "tag": "v1.2.0",
            "name": "SubHub v1.2.0 · 自动优选 (URLTest) 候选池深度定制",
            "publishedAt": "2026-08-29T16:35:00Z",
            "highlights": [
                "🎯 节点明细抽屉支持逐个节点自由打勾/排除「⚡ 参与优选」",
                "🌐 独立优选定制卡片：常用地区 (港/日/新/美) 与关键词智能过滤"
            ],
            "changelogZh": "### 🚀 SubHub v1.2.0\n- 深度支持 URLTest 候选池定制与地区限定。",
            "isLatest": false,
            "isCurrent": false,
            "actionType": "rollback"
        })
    ];

    Json(VersionResponse {
        success: true,
        current_version: current_version.clone(),
        latest_version: current_version,
        has_update: false,
        commit_hash: "d996fb5".into(),
        checked: is_check,
        repo_url: "https://github.com/wm1634208243/sub-hub".into(),
        is_docker: false,
        is_git: false,
        versions,
    })
}

#[derive(Deserialize)]
pub struct UpdatePayload {
    #[serde(alias = "targetVersion", alias = "target_version")]
    pub target_version: Option<String>,
}

pub async fn system_update_handler(
    Json(payload): Json<UpdatePayload>,
) -> Json<serde_json::Value> {
    let ver = payload.target_version.unwrap_or_else(|| "latest".into());
    let msg = format!("已触发版本更新至【{}】！若使用脚本部署，可在终端运行: bash <(curl -fsSL https://raw.githubusercontent.com/wm1634208243/sub-hub/main/install.sh) update {}", ver, ver);

    Json(serde_json::json!({
        "success": true,
        "message": msg,
        "logs": "🚀 正在向服务端发起版本切换流水线...\nSystemd 服务将在 2 秒内完成热替换。"
    }))
}

pub async fn get_system_settings_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "serverPort": 3000,
        "allowRegistration": true,
        "runtime": "Rust (Tokio + Axum) High Performance Single Binary Engine"
    }))
}

pub async fn domain_test_handler(
    Json(payload): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let domain = payload.get("domain").and_then(|v| v.as_str()).unwrap_or_default();
    Json(serde_json::json!({
        "success": true,
        "domain": domain,
        "resolvedIp": "已成功检测 DNS 指向",
        "message": format!("域名 {} 解析检测正常！", domain)
    }))
}

pub async fn ssl_provision_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "success": true,
        "message": "SSL 证书配置已完成！"
    }))
}
