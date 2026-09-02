use crate::engine::fetcher::SubscriptionFetcher;
use crate::engine::renamer::{detect_node_primary_region, format_node_name, REGION_FLAGS};
use crate::models::{ProxyNode, UserConfig};
use regex::Regex;
use std::collections::{HashMap, HashSet};

pub struct AggregatedResult {
    pub yaml: String,
    pub user_info: Option<String>,
    pub total_nodes: usize,
    pub proxies: Vec<ProxyNode>,
}

pub async fn aggregate_clash_yaml(
    config: &UserConfig,
    fetcher: &SubscriptionFetcher,
) -> Result<AggregatedResult, String> {
    let mut all_proxies: Vec<ProxyNode> = Vec::new();
    let mut sub_group_map: Vec<(String, Vec<String>)> = Vec::new();

    let mut agg_upload: u64 = 0;
    let mut agg_download: u64 = 0;
    let mut agg_total: u64 = 0;
    let mut min_expire: Option<u64> = None;

    let mut seen_names: HashMap<String, usize> = HashMap::new();

    for sub in &config.subscriptions {
        if !sub.enabled || sub.url.is_empty() {
            continue;
        }

        let prefix = sub.prefix.as_deref().or(Some(&sub.name)).unwrap_or_default();
        let fetched_res = fetcher.fetch(&sub.url, prefix, false).await;

        let mut current_sub_nodes = Vec::new();

        if let Ok(res) = &fetched_res {
            let ui_opt = res.user_info.as_ref().or(sub.user_info.as_ref());
            if let Some(ui) = ui_opt {
                if let Some(up) = ui.get("upload").and_then(|v| v.as_u64()) { agg_upload += up; }
                if let Some(down) = ui.get("download").and_then(|v| v.as_u64()) { agg_download += down; }
                if let Some(tot) = ui.get("total").and_then(|v| v.as_u64()) { agg_total += tot; }
                if let Some(exp) = ui.get("expire").and_then(|v| v.as_u64()) {
                    if exp > 0 {
                        let exp_sec = if exp > 100_000_000_000 { exp / 1000 } else { exp };
                        min_expire = Some(min_expire.map_or(exp_sec, |m| m.min(exp_sec)));
                    }
                }
            }
            if let Some(exp_str) = &sub.custom_expire {
                if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(exp_str) {
                    let sec = dt.timestamp() as u64;
                    if sec > 0 {
                        min_expire = Some(min_expire.map_or(sec, |m| m.min(sec)));
                    }
                } else if let Ok(dt) = chrono::NaiveDate::parse_from_str(exp_str, "%Y-%m-%d") {
                    if let Some(ndt) = dt.and_hms_opt(23, 59, 59) {
                        let sec = ndt.and_utc().timestamp() as u64;
                        if sec > 0 {
                            min_expire = Some(min_expire.map_or(sec, |m| m.min(sec)));
                        }
                    }
                }
            }
        } else if let Err(e) = &fetched_res {
            tracing::warn!("Failed to fetch subscription {}: {}", sub.name, e);
            if let Some(ui) = &sub.user_info {
                if let Some(up) = ui.get("upload").and_then(|v| v.as_u64()) { agg_upload += up; }
                if let Some(down) = ui.get("download").and_then(|v| v.as_u64()) { agg_download += down; }
                if let Some(tot) = ui.get("total").and_then(|v| v.as_u64()) { agg_total += tot; }
                if let Some(exp) = ui.get("expire").and_then(|v| v.as_u64()) {
                    if exp > 0 {
                        let exp_sec = if exp > 100_000_000_000 { exp / 1000 } else { exp };
                        min_expire = Some(min_expire.map_or(exp_sec, |m| m.min(exp_sec)));
                    }
                }
            }
        }

        if let Ok(res) = fetched_res {
            for mut node in res.nodes {
                let mut formatted = format_node_name(
                    &node.name,
                    &node.server,
                    config.enable_auto_flags,
                    config.enable_clean_ad_and_rate,
                    &config.custom_rename_rules,
                    sub.default_region.as_deref(),
                );

                // Deduplicate names strictly
                if let Some(count) = seen_names.get_mut(&formatted) {
                    *count += 1;
                    formatted = format!("{} ({})", formatted, *count);
                } else {
                    seen_names.insert(formatted.clone(), 1);
                }

                node.name = formatted.clone();
                node.extra.remove("name");
                current_sub_nodes.push(formatted);
                all_proxies.push(node);
            }
        }

        if !current_sub_nodes.is_empty() {
            let sub_group_name = format!("📦 订阅源 · {}", sub.name);
            sub_group_map.push((sub_group_name, current_sub_nodes));
        }
    }

    if let Some(exp_str) = &config.token_expires_at {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(exp_str) {
            let sec = dt.timestamp() as u64;
            if sec > 0 { min_expire = Some(min_expire.map_or(sec, |m| m.min(sec))); }
        } else if let Ok(dt) = chrono::NaiveDate::parse_from_str(exp_str, "%Y-%m-%d") {
            if let Some(ndt) = dt.and_hms_opt(23, 59, 59) {
                let sec = ndt.and_utc().timestamp() as u64;
                if sec > 0 { min_expire = Some(min_expire.map_or(sec, |m| m.min(sec))); }
            }
        }
    }

    let cleaned_proxies: Vec<serde_json::Value> = all_proxies.iter().map(sanitize_proxy_node_for_clash).collect();
    let all_node_names: Vec<String> = cleaned_proxies
        .iter()
        .filter_map(|p| p.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    let active_proxies: Vec<ProxyNode> = all_proxies.clone();

    let main_proxy_group = config.custom_proxy_group_name
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("🚀 节点选择");

    // 1. Regional node grouping
    let mut node_region_map: HashMap<String, String> = HashMap::new(); // nodeName -> regionCode
    for p in &active_proxies {
        if let Some(reg) = detect_node_primary_region(&p.name, &p.server, p.default_region.as_deref()) {
            node_region_map.insert(p.name.clone(), reg.code.to_string());
        }
    }

    let mut region_groups: Vec<(String, String, Vec<String>)> = Vec::new(); // (autoName, fallbackName, nodeNames)
    for reg in REGION_FLAGS {
        let matched: Vec<String> = all_node_names
            .iter()
            .filter(|name| node_region_map.get(*name).map_or(false, |c| c == reg.code))
            .cloned()
            .collect();
        if !matched.is_empty() {
            let auto_name = format!("{} {}自动", reg.flag, reg.name);
            let fallback_name = format!("{} {}故障转移", reg.flag, reg.name);
            region_groups.push((auto_name, fallback_name, matched));
        }
    }

    // 2. Candidate Pool Filtering for URLTest & Fallback
    let excluded_list: Vec<String> = config.excluded_auto_test_nodes.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let is_excluded = |candidate: &str| -> bool {
        for exc in &excluded_list {
            if exc == candidate || candidate.starts_with(exc) || exc.starts_with(candidate) || candidate.contains(exc) {
                return true;
            }
        }
        false
    };

    let mut auto_test_candidates: Vec<String> = all_node_names
        .iter()
        .filter(|name| !is_excluded(name))
        .cloned()
        .collect();

    if config.auto_test_scope == "custom" && !config.auto_test_regions.is_empty() {
        let allowed_region_set: HashSet<String> = config.auto_test_regions.iter().map(|r| r.to_uppercase()).collect();
        auto_test_candidates.retain(|name| {
            node_region_map.get(name).map_or(false, |c| allowed_region_set.contains(c))
        });
    }

    if let Some(inc_kw) = &config.auto_test_include_keywords {
        if !inc_kw.trim().is_empty() {
            if let Ok(re) = Regex::new(&format!("(?i){}", inc_kw.trim())) {
                auto_test_candidates.retain(|name| re.is_match(name));
            }
        }
    }

    if let Some(exc_kw) = &config.auto_test_exclude_keywords {
        if !exc_kw.trim().is_empty() {
            if let Ok(re) = Regex::new(&format!("(?i){}", exc_kw.trim())) {
                auto_test_candidates.retain(|name| !re.is_match(name));
            }
        }
    }

    let final_auto_test_proxies = if !auto_test_candidates.is_empty() {
        auto_test_candidates
    } else if !all_node_names.is_empty() {
        all_node_names.clone()
    } else {
        vec!["DIRECT".into()]
    };

    // 3. Construct Proxy Groups (极简核心策略组排布：主控置顶，场景分流组紧随)
    let mut proxy_groups: Vec<serde_json::Value> = Vec::new();

    // 3.1 Per-Subscription Auto Groups
    let mut sub_auto_groups: Vec<(String, Vec<String>)> = Vec::new();
    for (group_name, nodes) in &sub_group_map {
        if !nodes.is_empty() {
            let auto_name = if group_name.starts_with("📦 订阅源 · ") {
                group_name.replace("📦 订阅源 · ", "⚡ 优选 · ")
            } else {
                format!("⚡ 优选 · {}", group_name)
            };
            sub_auto_groups.push((auto_name, nodes.clone()));
        }
    }

    // 3.2 🚀 节点选择 (Master Selector - 置顶展示)
    let mut master_selector_proxies = Vec::new();
    master_selector_proxies.push("⚡ 自动优选".to_string());
    for (auto_name, _) in &sub_auto_groups {
        master_selector_proxies.push(auto_name.clone());
    }
    for name in &all_node_names {
        master_selector_proxies.push(name.clone());
    }
    master_selector_proxies.push("DIRECT".to_string());
    master_selector_proxies.retain(|s| !s.trim().is_empty());
    master_selector_proxies.dedup();

    proxy_groups.push(serde_json::json!({
        "name": main_proxy_group,
        "type": "select",
        "proxies": master_selector_proxies
    }));

    // 3.3 ⚡ 全局自动优选 (从主界面卡片隐藏，收纳于「🚀 节点选择」内)
    proxy_groups.push(serde_json::json!({
        "name": "⚡ 自动优选",
        "type": "url-test",
        "hidden": true,
        "url": "http://www.gstatic.com/generate_204",
        "interval": 300,
        "tolerance": 50,
        "lazy": true,
        "proxies": final_auto_test_proxies
    }));

    // 3.4 各订阅源专属自动优选组 (从主界面卡片隐藏，收纳于「🚀 节点选择」内)
    for (auto_name, nodes) in &sub_auto_groups {
        proxy_groups.push(serde_json::json!({
            "name": auto_name,
            "type": "url-test",
            "hidden": true,
            "url": "http://www.gstatic.com/generate_204",
            "interval": 300,
            "tolerance": 50,
            "lazy": true,
            "proxies": nodes
        }));
    }

    // 3.5 Scenario Groups Proxies List
    let mut scenario_proxies = Vec::new();
    scenario_proxies.push(main_proxy_group.to_string());
    scenario_proxies.push("⚡ 自动优选".to_string());
    for (auto_name, _) in &sub_auto_groups {
        scenario_proxies.push(auto_name.clone());
    }
    for name in &all_node_names {
        scenario_proxies.push(name.clone());
    }
    scenario_proxies.push("DIRECT".to_string());
    scenario_proxies.retain(|s| !s.trim().is_empty());
    scenario_proxies.dedup();

    let mut direct_first_proxies = Vec::new();
    direct_first_proxies.push("DIRECT".to_string());
    direct_first_proxies.push(main_proxy_group.to_string());
    direct_first_proxies.push("⚡ 自动优选".to_string());
    for (auto_name, _) in &sub_auto_groups {
        direct_first_proxies.push(auto_name.clone());
    }
    for name in &all_node_names {
        direct_first_proxies.push(name.clone());
    }
    direct_first_proxies.retain(|s| !s.trim().is_empty());
    direct_first_proxies.dedup();

    // 3.4 Scenario groups
    if config.enable_ai_group {
        proxy_groups.push(serde_json::json!({ "name": "🤖 AI 专线", "type": "select", "proxies": scenario_proxies }));
    }
    if config.enable_media_group {
        proxy_groups.push(serde_json::json!({ "name": "🎬 国际流媒体", "type": "select", "proxies": scenario_proxies }));
    }
    if config.enable_telegram_group {
        proxy_groups.push(serde_json::json!({ "name": "📲 Telegram", "type": "select", "proxies": scenario_proxies }));
    }
    if config.enable_game_group {
        proxy_groups.push(serde_json::json!({ "name": "🎮 游戏平台", "type": "select", "proxies": direct_first_proxies }));
    }
    if config.enable_apple_group {
        proxy_groups.push(serde_json::json!({ "name": "🍎 Apple / 微软", "type": "select", "proxies": direct_first_proxies }));
    }
    if config.enable_final_group {
        let p_list = if config.fallback_rule == "DIRECT" { &direct_first_proxies } else { &scenario_proxies };
        proxy_groups.push(serde_json::json!({ "name": "🐟 漏网之鱼", "type": "select", "proxies": p_list }));
    }

    // 4. Build Rules List
    let mut rules: Vec<String> = Vec::new();
    let no_resolve = if config.enable_no_resolve { ",no-resolve" } else { "" };

    // High priority direct
    for ip in &config.direct_ips {
        if !ip.trim().is_empty() { rules.push(format!("IP-CIDR,{},DIRECT{}", ip.trim(), no_resolve)); }
    }
    for proc in &config.direct_processes {
        if !proc.trim().is_empty() { rules.push(format!("PROCESS-NAME,{},DIRECT", proc.trim())); }
    }
    // High-priority domestic app desktop processes
    for proc in &["Douyin", "抖音", "Douyin-Darwin", "NeteaseMusic", "QQMusic", "KugouMusic", "WeChat", "企业微信", "DingTalk", "Feishu", "bilibili"] {
        rules.push(format!("PROCESS-NAME,{},DIRECT", proc));
    }
    for kw in &config.direct_keywords {
        if !kw.trim().is_empty() { rules.push(format!("DOMAIN-KEYWORD,{},DIRECT", kw.trim())); }
    }
    // High-priority domestic keyword matches
    for kw in &[
        "douyin", "bytedance", "byteimg", "bytetos", "pstatp", "snssdk", "zijieapi", "iesdouyin",
        "kuaishou", "bilibili", "bilivideo", "hdslb", "alipay", "alicdn", "taobao", "tencent",
        "qqmusic", "netease", "123pan", "wps", "kingsoft", "todesk"
    ] {
        rules.push(format!("DOMAIN-KEYWORD,{},DIRECT", kw));
    }
    // SubHub server domain & LAN always DIRECT
    rules.push("DOMAIN-SUFFIX,wmxhub.com,DIRECT".into());

    for dom in &config.direct_domains {
        if !dom.trim().is_empty() { rules.push(format!("DOMAIN-SUFFIX,{},DIRECT", dom.trim())); }
    }

    // Scenario rules (Dedicated application routes take precedence over generic proxy rules)
    if config.enable_ai_group {
        rules.push("GEOSITE,openai,🤖 AI 专线".into());
        rules.push("GEOSITE,anthropic,🤖 AI 专线".into());
        for d in &["openai.com", "chatgpt.com", "anthropic.com", "claude.ai", "oaistatic.com", "oaiusercontent.com", "gemini.google.com", "x.ai", "grok.com", "mistral.ai", "copilot.microsoft.com", "perplexity.ai"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🤖 AI 专线", d));
        }
    }
    if config.enable_media_group {
        rules.push("GEOSITE,youtube,🎬 国际流媒体".into());
        rules.push("GEOSITE,netflix,🎬 国际流媒体".into());
        rules.push("GEOSITE,disney,🎬 国际流媒体".into());
        rules.push("GEOSITE,spotify,🎬 国际流媒体".into());
        for d in &["youtube.com", "googlevideo.com", "ytimg.com", "netflix.com", "nflxvideo.net", "disneyplus.com", "spotify.com", "hulu.com", "hbo.com", "max.com", "primevideo.com", "bilibili.tv", "bahamut.com.tw"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🎬 国际流媒体", d));
        }
    }
    if config.enable_telegram_group {
        rules.push("GEOSITE,telegram,📲 Telegram".into());
        for ip in &["91.108.4.0/22", "91.108.8.0/22", "91.108.12.0/22", "91.108.16.0/22", "91.108.20.0/22", "91.108.56.0/22", "149.154.160.0/20", "149.154.164.0/22", "149.154.168.0/22", "149.154.172.0/22"] {
            rules.push(format!("IP-CIDR,{},📲 Telegram{}", ip, no_resolve));
        }
        for d in &["t.me", "telegram.org", "telegram.me", "tdesktop.com", "telesco.pe"] {
            rules.push(format!("DOMAIN-SUFFIX,{},📲 Telegram", d));
        }
    }
    if config.enable_game_group {
        rules.push("GEOSITE,steam,🎮 游戏平台".into());
        rules.push("GEOSITE,epicgames,🎮 游戏平台".into());
        for d in &["steampowered.com", "steamcommunity.com", "steamgames.com", "epicgames.com", "ea.com", "origin.com", "playstation.com", "playstation.net", "xboxlive.com", "battle.net", "riotgames.com"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🎮 游戏平台", d));
        }
    }
    if config.enable_apple_group {
        rules.push("GEOSITE,apple,🍎 Apple / 微软".into());
        rules.push("GEOSITE,microsoft,🍎 Apple / 微软".into());
        for d in &["apple.com", "icloud.com", "itunes.com", "apple-cloudkit.com", "microsoft.com", "windowsupdate.com", "office.com", "live.com", "azure.com"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🍎 Apple / 微软", d));
        }
    }

    // High priority user custom proxy rules
    for ip in &config.proxy_ips {
        if !ip.trim().is_empty() { rules.push(format!("IP-CIDR,{},{}{}", ip.trim(), main_proxy_group, no_resolve)); }
    }
    for proc in &config.proxy_processes {
        if !proc.trim().is_empty() { rules.push(format!("PROCESS-NAME,{},{}", proc.trim(), main_proxy_group)); }
    }
    for kw in &config.proxy_keywords {
        if !kw.trim().is_empty() { rules.push(format!("DOMAIN-KEYWORD,{},{}", kw.trim(), main_proxy_group)); }
    }
    for dom in &config.proxy_domains {
        if !dom.trim().is_empty() { rules.push(format!("DOMAIN-SUFFIX,{},{}", dom.trim(), main_proxy_group)); }
    }
    // High-priority global tech forums & developer ecosystems
    for d in &[
        "nodeseek.com", "linux.do", "v2ex.com", "hostloc.com", "github.com", "githubusercontent.com",
        "githubassets.com", "gitlab.com", "google.com", "googleapis.com", "gstatic.com", "twitter.com",
        "x.com", "twimg.com", "reddit.com", "redd.it", "medium.com", "discord.com", "discord.gg",
        "notion.so", "huggingface.co", "docker.com", "docker.io", "t.me", "telegram.org",
        "wikipedia.org", "stackoverflow.com", "cloudflare.com", "jsdelivr.net"
    ] {
        rules.push(format!("DOMAIN-SUFFIX,{},{}", d, main_proxy_group));
    }

    // 3.5 Domestic common domains directly to DIRECT (Zero-delay, zero-download, instantaneous startup)
    if config.enable_geo_site_cn {
        rules.push("GEOSITE,private,DIRECT".into());
        rules.push("GEOSITE,cn,DIRECT".into());
        for d in &[
            "cn",
            // ByteDance / Douyin full ecosystem
            "douyin.com", "douyincdn.com", "douyinpic.com", "douyinstatic.com", "douyinvod.com", "iesdouyin.com",
            "bytedance.com", "bytegoofy.com", "byteimg.com", "bytescm.com", "bytetos.com", "bytedns.net", "bytednsdoc.com",
            "pstatp.com", "snssdk.com", "toutiao.com", "toutiaocdn.com", "toutiaopage.com", "zijieapi.com", "volces.com",
            "volccdn.com", "amemv.com", "feiliao.com", "ixigua.com", "pangle.cn", "oceanengine.com", "ecombdapi.com", "ecombdimg.com",
            // Kuaishou
            "kuaishou.com", "yximgs.com", "ksapisvr.com", "gifshow.com",
            // Tencent / WeChat
            "qq.com", "weixin.qq.com", "tencent.com", "gtimg.com", "gtimg.cn", "qlogo.cn", "qpic.cn", "servicewechat.com", "tenpay.com",
            // Alibaba / Taobao / Alipay
            "aliyun.com", "aliyuncs.com", "taobao.com", "tmall.com", "alipay.com", "alipayobjects.com", "alicdn.com", "tbcdn.cn", "ele.me",
            // JD
            "jd.com", "jd.hk", "360buyimg.com", "jdpay.com",
            // Bilibili
            "bilibili.com", "bilivideo.com", "hdslb.com", "biliapi.net",
            // NetEase
            "163.com", "126.net", "netease.com", "ydstatic.com", "music.163.com", "music.126.net",
            // Baidu
            "baidu.com", "baidupcs.com", "bdimg.com", "bdstatic.com", "baidubce.com",
            // Video / Streaming (Domestic)
            "iqiyi.com", "iqiyipic.com", "qy.net", "youku.com", "ykimg.com", "mgtv.com", "hunantv.com", "cctv.com",
            // Social / Content
            "zhihu.com", "zhimg.com", "xiaohongshu.com", "xhscdn.com", "xhscdn.net", "weibo.com", "weibocdn.com", "sinaimg.cn", "sina.com.cn", "sohu.com",
            // Life / Travel
            "meituan.com", "meituan.net", "dianping.com", "dpfile.com", "amap.com", "autonavi.com", "ctrip.com", "qunar.com", "12306.cn",
            // Productivity / Tools
            "123pan.com", "123pan.cn", "wps.com", "wps.cn", "wpscdn.com", "kingsoft.com", "ksosoft.com", "todesk.com", "feishu.cn", "dingtalk.com",
            // Hardware / Cloud
            "mi.com", "xiaomi.com", "mifile.cn", "huawei.com", "dbankcdn.com", "honor.com", "oppo.com", "vivo.com",
            // Tech Community
            "gitee.com", "csdn.net", "juejin.cn", "segmentfault.com", "oschina.net"
        ] {
            rules.push(format!("DOMAIN-SUFFIX,{},DIRECT", d));
        }
    }

    rules.push(format!("GEOIP,LAN,DIRECT{}", no_resolve));
    if config.enable_geo_ip_cn {
        rules.push(format!("GEOIP,CN,DIRECT{}", no_resolve));
    }
    rules.push(format!("GEOSITE,geolocation-!cn,{}", main_proxy_group));

    // QUIC Reject to prevent video streaming / Douyin / Bilibili packet loss and stalls
    rules.insert(0, "AND,((NETWORK,udp),(DST-PORT,443)),REJECT".into());

    let final_group = if config.enable_final_group { "🐟 漏网之鱼" } else if config.fallback_rule == "DIRECT" { "DIRECT" } else { main_proxy_group };
    rules.push(format!("MATCH,{}", final_group));

    let mut clash_map = serde_json::Map::new();
    clash_map.insert("mixed-port".into(), serde_json::json!(7890));
    clash_map.insert("allow-lan".into(), serde_json::json!(true));
    clash_map.insert("mode".into(), serde_json::json!("rule"));
    clash_map.insert("log-level".into(), serde_json::json!("info"));
    clash_map.insert("ipv6".into(), serde_json::json!(false));

    // Native TUN anti-loop routing engine
    clash_map.insert("tun".into(), serde_json::json!({
        "enable": true,
        "stack": "mixed",
        "dns-hijack": ["any:53", "tcp://any:53"],
        "auto-route": true,
        "auto-detect-interface": true,
        "strict-route": true
    }));

    if config.enable_tcp_concurrent {
        clash_map.insert("tcp-concurrent".into(), serde_json::json!(true));
    }
    if config.enable_unified_delay {
        clash_map.insert("unified-delay".into(), serde_json::json!(true));
    }

    let mut dns_cfg = serde_json::Map::new();
    dns_cfg.insert("enable".into(), serde_json::json!(true));
    dns_cfg.insert("ipv6".into(), serde_json::json!(false));
    dns_cfg.insert("enhanced-mode".into(), serde_json::json!("fake-ip"));
    dns_cfg.insert("fake-ip-range".into(), serde_json::json!("198.18.0.1/16"));
    dns_cfg.insert("default-nameserver".into(), serde_json::json!(["223.5.5.5", "119.29.29.29", "180.76.76.76"]));
    dns_cfg.insert("direct-nameserver".into(), serde_json::json!(["223.5.5.5", "119.29.29.29", "180.76.76.76"]));

    let mut nameservers = config.nameservers.clone();
    if nameservers.is_empty() {
        nameservers = vec![
            "223.5.5.5".to_string(),
            "119.29.29.29".to_string(),
            "180.76.76.76".to_string(),
            "https://223.5.5.5/dns-query".to_string(),
            "https://1.12.12.12/dns-query".to_string(),
        ];
    } else {
        if !nameservers.iter().any(|s| s.contains("223.5.5.5/dns-query")) {
            nameservers.push("https://223.5.5.5/dns-query".to_string());
        }
        if !nameservers.iter().any(|s| s.contains("1.12.12.12/dns-query")) {
            nameservers.push("https://1.12.12.12/dns-query".to_string());
        }
    }
    dns_cfg.insert("nameserver".into(), serde_json::to_value(&nameservers).unwrap_or_default());

    let mut fallback_dns = config.fallback_dns.clone();
    if fallback_dns.is_empty() {
        fallback_dns = vec![
            "https://1.1.1.1/dns-query".to_string(),
            "https://8.8.8.8/dns-query".to_string(),
        ];
    }
    dns_cfg.insert("fallback".into(), serde_json::to_value(&fallback_dns).unwrap_or_default());

    // Standard clean fake-ip-filter (Strip normal websites that break direct resolution)
    let mut clean_fake_ip_filter = vec![
        "*.lan".to_string(),
        "*.local".to_string(),
        "*.internal".to_string(),
        "*.home.arpa".to_string(),
        "time.*.com".to_string(),
        "time.*.gov".to_string(),
        "time.*.edu.cn".to_string(),
        "time.*.apple.com".to_string(),
        "time1.cloud.tencent.com".to_string(),
        "*.ntp.org.cn".to_string(),
        "ntp.*.com".to_string(),
        "localhost.ptlogin2.qq.com".to_string(),
        "*.srv.nintendo.net".to_string(),
        "*.stun.*.*".to_string(),
        "+.msftconnecttest.com".to_string(),
        "+.msftncsi.com".to_string(),
        "+.wmxhub.com".to_string(),
    ];
    for item in &config.fake_ip_filter {
        let trimmed = item.trim().to_lowercase();
        if !trimmed.is_empty() 
            && !trimmed.contains("bilibili")
            && !trimmed.contains("bilivideo")
            && !trimmed.contains("hdslb")
            && !trimmed.contains("baidu")
            && !trimmed.contains("qq.com")
            && !trimmed.contains("tencent")
            && !trimmed.contains("aliyun")
            && !trimmed.contains("taobao")
            && !trimmed.contains("jd.com")
            && !trimmed.contains("wps")
            && !trimmed.contains("douyin")
            && !trimmed.contains("bytedance")
            && !trimmed.contains("pstatp")
            && !trimmed.contains("snssdk")
            && !trimmed.contains("zijieapi")
            && !trimmed.contains("bytetos")
            && !trimmed.contains(".cn")
            && !clean_fake_ip_filter.contains(&trimmed) {
            clean_fake_ip_filter.push(item.trim().to_string());
        }
    }
    dns_cfg.insert("fake-ip-filter".into(), serde_json::to_value(&clean_fake_ip_filter).unwrap_or_default());

    dns_cfg.insert("fallback-filter".into(), serde_json::json!({
        "geoip": true,
        "geoip-code": "CN",
        "ipcidr": ["240.0.0.0/4"]
    }));

    let mut ns_policy = serde_json::Map::new();
    let domestic_doh = serde_json::json!([
        "223.5.5.5",
        "119.29.29.29",
        "180.76.76.76",
        "https://223.5.5.5/dns-query",
        "https://1.12.12.12/dns-query"
    ]);
    ns_policy.insert("+.cn".into(), domestic_doh.clone());
    ns_policy.insert("geosite:cn,private".into(), domestic_doh.clone());
    ns_policy.insert("+.bilibili.com,+.bilivideo.com,+.hdslb.com,+.baidu.com,+.baidupcs.com,+.qq.com,+.weixin.qq.com,+.tencent.com,+.taobao.com,+.aliyun.com,+.aliyuncs.com,+.jd.com,+.163.com,+.126.net,+.zhihu.com,+.douyin.com,+.douyincdn.com,+.douyinvod.com,+.iesdouyin.com,+.bytedance.com,+.byteimg.com,+.bytetos.com,+.pstatp.com,+.snssdk.com,+.zijieapi.com,+.kuaishou.com,+.xiaohongshu.com,+.weibo.com,+.sina.com.cn,+.sohu.com,+.meituan.com,+.amap.com,+.autonavi.com,+.123pan.com,+.wps.com,+.wps.cn,+.wpscdn.com,+.kingsoft.com,+.todesk.com,+.feishu.cn,+.dingtalk.com,+.mi.com,+.xiaomi.com,+.mifile.cn,+.gitee.com,+.csdn.net".into(), domestic_doh);
    dns_cfg.insert("nameserver-policy".into(), serde_json::Value::Object(ns_policy));
    clash_map.insert("dns".into(), serde_json::Value::Object(dns_cfg));

    if config.enable_sniffer {
        clash_map.insert("sniffer".into(), serde_json::json!({
            "enable": true,
            "sniff": {
                "TLS": { "ports": [443, 8443] },
                "HTTP": { "ports": [80, 8080, 8880], "override-destination": true }
            },
            "skip-domain": [
                "Mijia Cloud", "dlg.io.mi.com", "+.apple.com", "+.bilibili.com", "+.douyin.com",
                "+.douyinstatic.com", "+.douyinvod.com", "+.bytedance.com", "+.pstatp.com",
                "+.snssdk.com", "+.zijieapi.com", "+.qq.com", "+.tencent.com", "+.baidu.com",
                "+.taobao.com", "+.aliyun.com", "+.jd.com", "+.163.com", "geosite:cn"
            ]
        }));
    }

    // Collect all valid proxy names strictly from cleaned_proxies and proxy_groups
    let mut valid_target_names: HashSet<String> = HashSet::new();
    valid_target_names.insert("DIRECT".to_string());
    valid_target_names.insert("REJECT".to_string());
    valid_target_names.insert("GLOBAL".to_string());
    for p in &cleaned_proxies {
        if let Some(n) = p.get("name").and_then(|v| v.as_str()) {
            valid_target_names.insert(n.to_string());
        }
    }
    for g in &proxy_groups {
        if let Some(n) = g.get("name").and_then(|v| v.as_str()) {
            valid_target_names.insert(n.to_string());
        }
    }

    // Filter all proxy-groups to guarantee 100% referential integrity
    for g in &mut proxy_groups {
        if let Some(p_arr) = g.get_mut("proxies").and_then(|v| v.as_array_mut()) {
            p_arr.retain(|item| {
                if let Some(target) = item.as_str() {
                    valid_target_names.contains(target)
                } else {
                    false
                }
            });
            if p_arr.is_empty() {
                p_arr.push(serde_json::json!("DIRECT"));
            }
        }
    }

    clash_map.insert("proxies".into(), serde_json::Value::Array(cleaned_proxies));
    clash_map.insert("proxy-groups".into(), serde_json::Value::Array(proxy_groups));
    clash_map.insert("rules".into(), serde_json::to_value(rules).unwrap_or_default());

    let yaml_string = serde_yaml::to_string(&clash_map).map_err(|e| format!("YAML 序列化失败: {}", e))?;

    let mut userinfo_header = None;
    if agg_total > 0 || agg_upload > 0 || agg_download > 0 || min_expire.is_some() {
        let mut s = format!("upload={}; download={}; total={}", agg_upload, agg_download, agg_total);
        if let Some(exp) = min_expire {
            let exp_sec = if exp > 100_000_000_000 { exp / 1000 } else { exp };
            s.push_str(&format!("; expire={}", exp_sec));
        }
        userinfo_header = Some(s);
    }

    Ok(AggregatedResult {
        yaml: yaml_string,
        user_info: userinfo_header,
        total_nodes: active_proxies.len(),
        proxies: active_proxies,
    })
}

#[allow(dead_code)]
pub fn format_bytes_human(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024.0;
    let b = bytes as f64;
    if b < k {
        format!("{} B", bytes)
    } else if b < k * k {
        format!("{:.1} KB", b / k)
    } else if b < k * k * k {
        format!("{:.2} MB", b / (k * k))
    } else if b < k * k * k * k {
        format!("{:.2} GB", b / (k * k * k))
    } else {
        format!("{:.2} TB", b / (k * k * k * k))
    }
}

pub async fn batch_test_proxies_health(proxies: &[ProxyNode], timeout_ms: u64) -> Vec<serde_json::Value> {
    use tokio::net::TcpStream;
    use tokio::time::{timeout, Duration, Instant};

    let mut results = Vec::new();
    for p in proxies {
        let addr = format!("{}:{}", p.server, p.port);
        let start = Instant::now();
        let is_alive = match timeout(Duration::from_millis(timeout_ms), TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => true,
            _ => false,
        };
        let latency = if is_alive { start.elapsed().as_millis() as u64 } else { 9999 };
        results.push(serde_json::json!({
            "name": p.name,
            "server": p.server,
            "port": p.port,
            "alive": is_alive,
            "latency": latency
        }));
    }
    results
}

pub fn sanitize_proxy_node_for_clash(node: &ProxyNode) -> serde_json::Value {
    if let Ok(mut val) = serde_json::to_value(node) {
        if let Some(obj) = val.as_object_mut() {
            // Force name to strictly match node.name (overriding any stale name in extra)
            obj.insert("name".into(), serde_json::Value::String(node.name.clone()));

            // 1. Remove all null values
            obj.retain(|_, v| !v.is_null());

            // 2. Remove legacy snake_case keys if kebab-case exists
            for (snake, kebab) in &[
                ("reality_opts", "reality-opts"),
                ("ws_opts", "ws-opts"),
                ("grpc_opts", "grpc-opts"),
                ("h2_opts", "h2-opts"),
                ("http_opts", "http-opts"),
                ("client_fingerprint", "client-fingerprint"),
                ("skip_cert_verify", "skip-cert-verify"),
                ("alter_id", "alterId"),
            ] {
                if obj.contains_key(*kebab) {
                    obj.remove(*snake);
                }
            }

            // 3. Clean reality-opts internal keys
            if let Some(r_opts) = obj.get_mut("reality-opts").and_then(|v| v.as_object_mut()) {
                r_opts.retain(|_, v| !v.is_null());
                if let Some(pbk) = r_opts.remove("public_key") {
                    r_opts.insert("public-key".into(), pbk);
                }
                if let Some(sid) = r_opts.remove("short_id") {
                    r_opts.insert("short-id".into(), sid);
                }
            }

            // 4. Clean ws-opts internal keys
            if let Some(w_opts) = obj.get_mut("ws-opts").and_then(|v| v.as_object_mut()) {
                w_opts.retain(|_, v| !v.is_null());
            }

            // 5. Clean grpc-opts internal keys
            if let Some(g_opts) = obj.get_mut("grpc-opts").and_then(|v| v.as_object_mut()) {
                g_opts.retain(|_, v| !v.is_null());
                if let Some(svc) = g_opts.remove("service_name").or_else(|| g_opts.remove("grpc_service_name")) {
                    g_opts.insert("grpc-service-name".into(), svc);
                }
            }

            // 6. Clean uTLS fingerprint: only allow standard browser fingerprints
            let valid_fp = ["chrome", "firefox", "safari", "ios", "android", "edge", "360", "qq", "random", "randomized"];
            if let Some(fp) = obj.get("fingerprint").and_then(|v| v.as_str()) {
                if !valid_fp.contains(&fp.to_lowercase().as_str()) {
                    obj.remove("fingerprint");
                }
            }
            if let Some(fp) = obj.get("client-fingerprint").and_then(|v| v.as_str()) {
                if !valid_fp.contains(&fp.to_lowercase().as_str()) {
                    obj.remove("client-fingerprint");
                }
            }

            // 7. Clean protocol specific illegal fields
            let node_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
            if node_type == "hysteria2" || node_type == "hy2" {
                obj.remove("fingerprint");
                obj.remove("client-fingerprint");
                obj.remove("mport");
            }

            // 8. Remove duplicate client-fingerprint if fingerprint is same
            if obj.contains_key("client-fingerprint") && obj.contains_key("fingerprint") {
                obj.remove("fingerprint");
            }

            return serde_json::Value::Object(obj.clone());
        }
    }
    serde_json::to_value(node).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_vless_reality_no_nulls() {
        let node = ProxyNode {
            name: "US-Reality".into(),
            node_type: "vless".into(),
            server: "1.2.3.4".into(),
            port: 443,
            uuid: Some("uuid-1234".into()),
            tls: Some(true),
            udp: Some(true),
            servername: Some("www.cloudflare.com".into()),
            client_fingerprint: Some("chrome".into()),
            reality_opts: Some(serde_json::json!({
                "public-key": "pbk123",
                "short-id": "sid123"
            })),
            ..Default::default()
        };

        let val = sanitize_proxy_node_for_clash(&node);
        let yaml = serde_yaml::to_string(&val).unwrap();
        println!("Generated YAML:\n{}", yaml);

        assert!(!yaml.contains("null"), "YAML should not contain any nulls!");
        assert!(yaml.contains("client-fingerprint: chrome"));
        assert!(yaml.contains("reality-opts:"));
        assert!(!yaml.contains("reality_opts:"));
        assert!(!yaml.contains("cipher:"));
        assert!(!yaml.contains("alter_id:"));
        assert!(!yaml.contains("password:"));
    }

    #[tokio::test]
    async fn test_aggregation_user_info_and_compatible_nodes() {
        use crate::models::SubscriptionItem;

        let mut config = UserConfig::default();
        config.subscriptions = vec![SubscriptionItem {
            id: "sub-1".into(),
            name: "DMIT".into(),
            url: "https://example.com/sub".into(),
            prefix: None,
            default_region: None,
            custom_expire: None,
            enabled: true,
            auto_refresh_interval: 60,
            status: None,
            error: None,
            nodes_count: None,
            source_type: None,
            user_info: Some(serde_json::json!({
                "upload": 1000000000_u64,
                "download": 2000000000_u64,
                "total": 100000000000_u64,
                "expire": 1788060000000_u64
            })),
            updated_at: None,
        }];

        let fetcher = SubscriptionFetcher::new();
        let res = aggregate_clash_yaml(&config, &fetcher).await.unwrap();

        assert!(res.user_info.is_some(), "user_info header must be produced");
        let ui = res.user_info.unwrap();
        assert!(ui.contains("upload=1000000000"));
        assert!(ui.contains("download=2000000000"));
        assert!(ui.contains("total=100000000000"));
        assert!(ui.contains("expire=1788060000"), "expire must be in seconds, not ms! got {}", ui);

        // Check YAML is valid and clean
        assert!(res.yaml.contains("🚀 节点选择"));
        assert!(res.yaml.contains("⚡ 自动优选"));

        // If mihomo binary is available on system, execute live kernel syntax test
        if std::path::Path::new("/tmp/mihomo").exists() {
            let tmp_path = format!("/tmp/unit_test_{}.yaml", std::process::id());
            std::fs::write(&tmp_path, &res.yaml).unwrap();
            let output = std::process::Command::new("/tmp/mihomo")
                .args(&["-t", "-f", &tmp_path])
                .output()
                .expect("failed to execute mihomo test");
            let _ = std::fs::remove_file(&tmp_path);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(output.status.success(), "mihomo kernel validation failed!\nSTDOUT:\n{}\nSTDERR:\n{}", stdout, stderr);
        }
    }
}
