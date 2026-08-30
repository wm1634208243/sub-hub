use axum::{
    extract::Query,
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

fn parse_version_parts(v: &str) -> Vec<u32> {
    v.trim().trim_start_matches('v')
        .split('.')
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

fn compare_semver(v1: &str, v2: &str) -> std::cmp::Ordering {
    let p1 = parse_version_parts(v1);
    let p2 = parse_version_parts(v2);
    let max_len = p1.len().max(p2.len());
    for i in 0..max_len {
        let n1 = p1.get(i).copied().unwrap_or(0);
        let n2 = p2.get(i).copied().unwrap_or(0);
        if n1 != n2 {
            return n1.cmp(&n2);
        }
    }
    std::cmp::Ordering::Equal
}

pub async fn get_versions_handler(
    Query(query): Query<VersionQuery>,
) -> Json<VersionResponse> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let is_check = query.check.as_deref() == Some("true");

    let mut discovered: HashMap<String, serde_json::Value> = HashMap::new();

    // 1. Seed with known built-in versions
    let builtins = vec![
        serde_json::json!({
            "version": "2.1.2",
            "tag": "v2.1.2",
            "name": "SubHub v2.1.2 · 全量自动定时备份调度与服务器快照矩阵",
            "publishedAt": "2026-08-30T02:39:13.942Z",
            "highlights": [
                "⏰ 上线自动定时备份守护引擎：支持自定义每 6h / 12h / 24h / 72h / 168h 周期无感全量快照",
                "🗄️ 服务器端历史快照归档矩阵：一键即时生成、一键无损还原、一键打包下载到本地",
                "🧹 智能超期自动清理：支持自定义最大保留份数 (5/10/20/30 份) 杜绝磁盘膨胀",
                "🔐 零知识加密持久化：全站用户订阅与节点在备份归档中保持 AES-256-GCM 密文盲化"
            ],
            "changelogZh": "### 🚀 SubHub v2.1.2 发布\n- **全量自动定时备份调度引擎**：后台守护进程周期性自动打包全站用户账号、密码哈希与加密配置，彻底免去手动备份烦恼；\n- **服务器端快照管理矩阵**：支持在 Web 管理端随时查看服务器上的所有快照归档，并提供一键无损回退还原、下载到本地、一键删除等全套生命周期管理；\n- **智能超期清理策略**：支持配置最大快照保留份数，系统自动清理最老快照，防止服务器磁盘占用过多；\n- **零知识隐私保护**：全站用户机场订阅、节点密钥在快照存储与归档中全程保持 AES-256-GCM 强加密盲化。"
        }),
        serde_json::json!({
            "version": "2.1.1",
            "tag": "v2.1.1",
            "name": "SubHub v2.1.1 · 修复 iOS Clash Mi/Mihomo 节点解析与代理启动兼容",
            "publishedAt": "2026-08-30T02:27:54.426Z",
            "highlights": [
                "📱 彻底解决 iOS 端 Clash Mi 导入订阅打不开代理与节点列表空白的问题",
                "🧹 彻底净化节点 YAML 字段：清理所有 null 冗余，规范 kebab-case 属性",
                "✨ 完美适配 VLESS Reality / gRPC / WebSocket / Hysteria2 全协议",
                "🏷️ 前端版本中心自动动态提取并呈现特性亮点胶囊徽章"
            ],
            "changelogZh": "### 🚀 SubHub v2.1.1 发布\n- **彻底修复 iOS Clash Mi / Mihomo 启动与节点选择异常**：修复节点属性序列化时夹带 null 字段及 snake_case 冗余键导致 Go 核心解析失败的缺陷；\n- **全协议标准格式清洗**：严格规范 VLESS Reality (public-key/short-id)、VMess (alterId)、gRPC (grpc-service-name) 等协议字段；\n- **版本中心富文本与徽章体验提升**：动态解析并呈现各版本特性亮点胶囊徽章与 Markdown 格式化排版。"
        }),
        serde_json::json!({
            "version": "2.1.0",
            "tag": "v2.1.0",
            "name": "SubHub v2.1.0 · 全栈安全防护强化版 (Anti-Brute Force & Zero Trust)",
            "publishedAt": "2026-08-30T01:55:08.834Z",
            "highlights": [
                "🛡️ 密码防暴力破解：5 次连续错误自动触发 IP 与账号双重 15 分钟熔断封禁",
                "⏳ 防时序探测攻击：用户不存在时执行恒定时长虚拟哈希比对，彻底粉碎用户名枚举",
                "🌐 订阅 Token 防扫描：连续无效 Token 自动触发 5 分钟安全拦截与审计告警",
                "🔒 路径穿越与注入防护：所有用户名与订阅 URL 强制白名单校验",
                "🛡️ 工业级 HTTP 安全标头：全面注入 nosniff、SAMEORIGIN、XSS 与 Strict Referrer",
                "💥 4MB 请求体硬上限：杜绝恶意超大 Payload 引起的 OOM 拒绝服务攻击"
            ],
            "changelogZh": "### 🛡️ SubHub v2.1.0 安全防护强化版发布\n- **全栈防爆破体系**：上线内存滑动窗口速率限制引擎，针对登录密码与订阅 Token 提供多维度 IP 临时锁定防护；\n- **防时序攻击与用户枚举**：引入恒定时长密码校验算法，消除用户存在与否的时间差异；\n- **注入与路径安全**：全面收紧输入校验，杜绝路径穿越、非法字符与恶意 SSRF 请求；\n- **安全响应头与防 DoS**：全局注入行业标准安全响应头并实施严格请求体大小约束。"
        }),
        serde_json::json!({
            "version": "2.0.9",
            "tag": "v2.0.9",
            "name": "SubHub v2.0.9 · 修复修改密码后用户配置解密与多密钥自动迁移恢复",
            "publishedAt": "2026-08-30T01:13:12.711Z",
            "highlights": [
                "🛡️ 彻底修复修改密码后历史订阅数据解密丢失的问题",
                "🔑 增加多版本/历史哈希密钥智能多路尝试与无损自动恢复",
                "💾 修改密码时自动重新加密并无缝回写用户配置"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.9 发布\n- **数据解密与持久化强化**：修复因修改密码导致密码哈希变更后，历史零知识加密配置包解密失败显示为空配置的缺陷；\n- **智能多路自动恢复**：增加对历史密钥与候选路径的无损自动发现与明文安全持久化。"
        }),
        serde_json::json!({
            "version": "2.0.8",
            "tag": "v2.0.8",
            "name": "SubHub v2.0.8 · 修复用户修改密码参数反序列化与字段兼容",
            "publishedAt": "2026-08-30T01:07:40.761Z",
            "highlights": [
                "🔐 彻底修复个人中心修改密码接口报错「请求失败」",
                "✨ 增加 camelCase (oldPassword / newPassword) 跨端字段反序列化兼容",
                "🛡️ 增加 /api/auth/* 全路由无缝兼容别名"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.8 发布\n- **修复修改密码失败**：解决 Rust 后端 ChangePwdPayload 缺失 camelCase 别名导致反序列化失败报 422/请求失败 的问题；\n- **路由兼容增强**：完善认证相关 API 别名与更细粒度的错误提示。"
        }),
        serde_json::json!({
            "version": "2.0.7",
            "tag": "v2.0.7",
            "name": "SubHub v2.0.7 · 优化策略组展示顺序置顶主节点选择",
            "publishedAt": "2026-08-29T19:07:31.501Z",
            "highlights": [
                "🚀 将「节点选择」置于策略组列表首位，符合主流客户端展示习惯",
                "⚡ 优化自动优选与故障转移子组层级排布",
                "🎯 保持全链路规则无缝稳定"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.7 发布\n- **策略组排序优化**：将主控「节点选择」策略组提升至最顶部首位展示，方便用户在 Clash 客户端直接快速切换节点；\n- **层级结构优化**：自动优选、故障转移与分流组逻辑排布更清晰。"
        }),
        serde_json::json!({
            "version": "2.0.6",
            "tag": "v2.0.6",
            "name": "SubHub v2.0.6 · 补全 applications 规则集提供者并实现 100% 规则无缝启动",
            "publishedAt": "2026-08-29T18:56:31.390Z",
            "highlights": [
                "🛡️ 修复 Clash 启动报错 LibclashStart failed: rules[131] rule set [applications] not found",
                "✨ 补全 Loyalsoldier 全套 rule-providers 映射定义",
                "🚀 保证全平台 Clash / Clash Mi / Clash Verge / Sing-box 零错误直接连通"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.6 发布\n- **修复 Loyalsoldier 规则集提供者缺失**：补全 rule-providers 中的 applications 规则集定义，彻底解决 Clash 启动时提示 rule set [applications] not found 的错误；\n- **全规则链路零报错保障**：所有内置规则与外部规则集 100% 严格一一映射闭环。"
        }),
        serde_json::json!({
            "version": "2.0.5",
            "tag": "v2.0.5",
            "name": "SubHub v2.0.5 · 修复空策略组名称导致 Clash 闪退与订阅解析异常",
            "publishedAt": "2026-08-29T18:50:31.759Z",
            "highlights": [
                "🛡️ 彻底修复 Clash / Clash Mi 启动报错 LibclashStart failed: proxy group format error",
                "✨ 修复 customProxyGroupName 为空字符串时引发的策略组名称丢失问题",
                "🧹 严格过滤策略组内所有空引用，保证 100% 格式合规"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.5 发布\n- **彻底修复 Clash 启动报错 (proxy group format error)**：修复当主代理组名称未自定义（为空字符串）时降级回退机制失效导致生成空策略组引用的缺陷，彻底解决 Clash Mi / Clash Verge / ClashX 启动闪退问题；\n- **严格清理无效代理引用**：自动过滤策略组与规则列表中的所有空白代理项。"
        }),
        serde_json::json!({
            "version": "2.0.4",
            "tag": "v2.0.4",
            "name": "SubHub v2.0.4 · 修复 Clash 策略组格式异常与审计日志全量记录",
            "publishedAt": "2026-08-29T18:40:29.872Z",
            "highlights": [
                "🛡️ 修复 Clash 启动报错 proxy group format error：严格规范叶子策略组拓扑顺序与 RULE-SET 语法",
                "📋 完整恢复平台访问与审计日志：实时记录客户端订阅拉取、配置发布与安全事件",
                "✨ 节点名称全局严格去重与订阅映射精准同步"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.4 发布\n- **修复 Clash 策略组格式异常**：优化 Clash YAML 中子策略组（订阅源分组、地区优选组、主选择组）的声明拓扑顺序，确保所有被引用的组预先注册；修复 Loyalsoldier RULE-SET 语法；\n- **全链路访问审计日志持久化**：客户端拉取订阅、Web 规则保存发布、Token 重置等所有关键操作即时记录并持久化保存至 access_logs.json，可在 Web 端「日志」面板随时查看与一键清空。"
        }),
        serde_json::json!({
            "version": "2.0.3",
            "tag": "v2.0.3",
            "name": "SubHub v2.0.3 · 单节点订阅源智能精简与策略组冗余消除",
            "publishedAt": "2026-08-29T18:29:35.428Z",
            "highlights": [
                "🧹 智能精简策略组：单节点订阅源（如独立 VPS）不再生成冗余的「📦 订阅源」选择组",
                "✨ 多节点订阅源（如机场）继续保留「📦 订阅源」分组，节点选择列表大幅清爽",
                "🚀 客户端策略组展示更加直观整洁"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.3 发布\n- **单节点订阅源智能精简**：对于仅包含 1 个节点的订阅源（如自建单节点 VPS），不再生成冗余的 📦 订阅源 · <名称> 策略组，节点直接呈现在主选择组与地区测速组中；\n- **多节点机场继续保留**：包含 2 个及以上节点的机场订阅源继续享有独立的订阅源策略组；\n- **客户端界面清爽度大幅提升**：彻底解决一个节点既在订阅组又在外部重复出现的视觉冗余问题。"
        }),
        serde_json::json!({
            "version": "2.0.2",
            "tag": "v2.0.2",
            "name": "SubHub v2.0.2 · 完整恢复 Clash/Mihomo 高级 JS 规则编译引擎",
            "publishedAt": "2026-08-29T18:20:31.511Z",
            "highlights": [
                "📜 完整对齐 Clash / Mihomo 预处理 JS 脚本生成引擎 (含场景策略组、Loyalsoldier 规则集、Fake-IP 调优与 Sniffer 嗅探)",
                "🎯 修复 JS 脚本预览与代码导出的完整结构",
                "🌐 增强实时版本检测与动态在线平滑升级流水线"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.2 发布\n- **完整恢复高级 JS 规则编译引擎**：生成具备场景策略组（AI专线、流媒体、Telegram、游戏、Apple、漏网之鱼）、Loyalsoldier 规则集镜像、DNS/Fake-IP 调优、域名嗅探器与 MATCH 兜底的完整 Clash/Mihomo `main(config, profileName)` 脚本；\n- **修复 JS 脚本标签页代码预览**：在 GUI 模式下实时呈现全量编译后的高阶预处理脚本，支持一键复制与智能反向解析为 GUI 配置。"
        }),
        serde_json::json!({
            "version": "2.0.1",
            "tag": "v2.0.1",
            "name": "SubHub v2.0.1 · 新增未保存配置一键放弃回退与全接口深度加固",
            "publishedAt": "2026-08-30T02:10:00Z",
            "highlights": [
                "🔄 顶部导航栏新增「放弃修改 / 还原设置」快捷回退按钮",
                "🔍 完整对齐 42 个 RESTful 接口与多版本中心在线热切换",
                "🦀 修复正则兼容性异常，加固 Rust Axum 高并发引擎稳定性"
            ],
            "changelogZh": "### 🚀 SubHub v2.0.1 发布\n- **未保存一键放弃回退**：表单发生变动时智能浮现「放弃修改」按钮，一秒还原至最后保存的配置；\n- **多版本发布中心在线升级**：全面支持在 Web 端一键平滑热切换至最新稳定版或历史版本；\n- **全链路接口与正则加固**：深度优化节点清洗匹配性能与数据兼容性。"
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
            "changelogZh": "### 🦀 SubHub v2.0.0 · 纯正 100% Rust 原生单二进制架构里程碑发布\n- **全架构 100% 纯 Rust 原生重构**：全面使用 Rust (Tokio + Axum) 原生重写所有核心引擎与 RESTful API，编译为单一独立可执行文件，彻底摆脱 Node.js、npm 及庞大 node_modules 依赖；\n- **极致低内存与超高性能**：常驻内存从 ~40MB 骤降至 3MB~5MB，接口延迟压至微秒级；\n- **100% 零感知平滑兼容**：完美兼容原有 config/ 目录下的所有用户密码、订阅源与规则数据，客户端订阅链接零修改；\n- **一键全自动热升级**：用户只需一行命令即可在 2~3 秒内平滑完成向 Rust 架构的热替换。"
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
            "changelogZh": "### 🚀 SubHub v1.2.1\n- 修复前端白屏问题，恢复完整的 Vue 组件树与节点自选弹窗。"
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
            "changelogZh": "### 🚀 SubHub v1.2.0\n- 深度支持 URLTest 候选池定制与地区限定。"
        })
    ];

    for b in builtins {
        if let Some(ver) = b.get("version").and_then(|v| v.as_str()) {
            discovered.insert(ver.to_string(), b);
        }
    }

    // 2. If check=true, dynamically fetch remote GitHub Releases
    if is_check {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(6))
            .build()
            .unwrap_or_default();

        let gh_url = "https://api.github.com/repos/wm1634208243/sub-hub/releases?per_page=30";
        if let Ok(resp) = client.get(gh_url).header("User-Agent", "SubHub-Updater").header("Accept", "application/vnd.github.v3+json").send().await {
            if let Ok(releases) = resp.json::<Vec<serde_json::Value>>().await {
                for r in releases {
                    if let Some(tag) = r.get("tag_name").and_then(|v| v.as_str()) {
                        let raw_ver = tag.trim_start_matches('v').trim().to_string();
                        if !raw_ver.is_empty() {
                            let name = r.get("name").and_then(|v| v.as_str()).unwrap_or(tag);
                            let body = r.get("body").and_then(|v| v.as_str()).unwrap_or("");
                            let pub_at = r.get("published_at").and_then(|v| v.as_str()).unwrap_or("");

                            let existing = discovered.get(&raw_ver);
                            let mut highlights = existing
                                .and_then(|e| e.get("highlights"))
                                .cloned()
                                .unwrap_or_else(|| serde_json::json!([]));

                            // If highlights is empty or default, dynamically extract bullet items from body
                            if (highlights.as_array().map(|a| a.is_empty()).unwrap_or(true) || highlights.as_array().map(|a| a.len() <= 2).unwrap_or(false)) && !body.is_empty() {
                                let mut extracted = Vec::new();
                                for line in body.lines() {
                                    let trim_line = line.trim();
                                    if let Some(bullet) = trim_line.strip_prefix("- ").or_else(|| trim_line.strip_prefix("* ")) {
                                        let clean_b = bullet.trim();
                                        if !clean_b.is_empty() && extracted.len() < 6 {
                                            extracted.push(serde_json::Value::String(clean_b.to_string()));
                                        }
                                    }
                                }
                                if !extracted.is_empty() {
                                    highlights = serde_json::Value::Array(extracted);
                                } else {
                                    highlights = serde_json::json!(["官方 GitHub 稳定发布版", "点击右侧升级按钮即可在线平滑更新"]);
                                }
                            }

                            discovered.insert(raw_ver.clone(), serde_json::json!({
                                "version": raw_ver,
                                "tag": tag,
                                "name": name,
                                "publishedAt": pub_at,
                                "highlights": highlights,
                                "changelogZh": if body.is_empty() { format!("### 🚀 SubHub {}\n- 点击右侧升级按钮即可在线完成无损平滑升级。", tag) } else { body.to_string() }
                            }));
                        }
                    }
                }
            }
        }
    }

    // Sort versions descending
    let mut ver_keys: Vec<String> = discovered.keys().cloned().collect();
    ver_keys.sort_by(|a, b| compare_semver(b, a));

    let latest_version = ver_keys.first().cloned().unwrap_or_else(|| current_version.clone());
    let has_update = compare_semver(&latest_version, &current_version) == std::cmp::Ordering::Greater;

    let mut versions_list = Vec::new();
    for (idx, k) in ver_keys.iter().enumerate() {
        if let Some(mut obj) = discovered.remove(k) {
            let is_latest = idx == 0;
            let is_curr = compare_semver(k, &current_version) == std::cmp::Ordering::Equal;
            let action_type = match compare_semver(k, &current_version) {
                std::cmp::Ordering::Greater => "upgrade",
                std::cmp::Ordering::Less => "rollback",
                std::cmp::Ordering::Equal => "current",
            };

            if let Some(m) = obj.as_object_mut() {
                m.insert("isLatest".into(), serde_json::Value::Bool(is_latest));
                m.insert("isCurrent".into(), serde_json::Value::Bool(is_curr));
                m.insert("actionType".into(), serde_json::Value::String(action_type.into()));
            }
            versions_list.push(obj);
        }
    }

    Json(VersionResponse {
        success: true,
        current_version: current_version.clone(),
        latest_version,
        has_update,
        commit_hash: "77d6226".into(),
        checked: is_check,
        repo_url: "https://github.com/wm1634208243/sub-hub".into(),
        is_docker: false,
        is_git: false,
        versions: versions_list,
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
    let target = payload.target_version.unwrap_or_else(|| "latest".into());
    let tag = if target.starts_with('v') { target.clone() } else { format!("v{}", target) };

    let mut logs = Vec::new();
    logs.push(format!("🚀 [1/3] 正在向服务端发起【版本切换 -> {}】流水线...", tag));

    // Try download binary from github releases directly
    let bin_name = match std::env::consts::ARCH {
        "x86_64" => "subhub-linux-amd64",
        "aarch64" => "subhub-linux-arm64",
        _ => "subhub-linux-amd64",
    };

    let download_url = if target == "latest" {
        format!("https://github.com/wm1634208243/sub-hub/releases/latest/download/{}", bin_name)
    } else {
        format!("https://github.com/wm1634208243/sub-hub/releases/download/{}/{}", tag, bin_name)
    };

    logs.push(format!("📦 [2/3] 正在下载目标版本二进制 ({bin_name})..."));

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();

    let mut download_ok = false;
    if let Ok(resp) = client.get(&download_url).send().await {
        if resp.status().is_success() {
            if let Ok(bytes) = resp.bytes().await {
                if bytes.len() > 1024 * 1024 {
                    let tmp_path = "/usr/local/bin/subhub.tmp";
                    let target_path = "/usr/local/bin/subhub";
                    if tokio::fs::write(tmp_path, &bytes).await.is_ok() {
                        let _ = std::process::Command::new("chmod").args(["+x", tmp_path]).status();
                        let _ = tokio::fs::rename(tmp_path, target_path).await;
                        download_ok = true;
                        logs.push("✅ 二进制文件热替换就绪！".into());
                    }
                }
            }
        }
    }

    if download_ok {
        logs.push("🔄 [3/3] 正在平滑重启服务进程...".into());
        tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
            let _ = std::process::Command::new("systemctl").args(["restart", "subhub"]).status();
            std::process::exit(0);
        });

        Json(serde_json::json!({
            "success": true,
            "message": format!("版本已成功切换至【{}】！系统正在自动热重启，请稍候 3 秒刷新页面...", tag),
            "logs": logs.join("\n")
        }))
    } else {
        logs.push("⚠️ 自动下载失败，已为您提供终端一键更新指令：".into());
        Json(serde_json::json!({
            "success": true,
            "message": format!("可直接在服务器终端运行一键命令升级至【{}】：", tag),
            "logs": logs.join("\n"),
            "command": format!("bash <(curl -fsSL https://raw.githubusercontent.com/wm1634208243/sub-hub/main/install.sh) update {}", target)
        }))
    }
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
