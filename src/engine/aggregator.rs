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
        match fetcher.fetch(&sub.url, prefix, false).await {
            Ok(res) => {
                // Aggregated userinfo from fetch result or fallback to stored sub.user_info
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
                        if sec > 0 { min_expire = Some(min_expire.map_or(sec, |m| m.min(sec))); }
                    } else if let Ok(dt) = chrono::NaiveDate::parse_from_str(exp_str, "%Y-%m-%d") {
                        if let Some(ndt) = dt.and_hms_opt(23, 59, 59) {
                            let sec = ndt.and_utc().timestamp() as u64;
                            if sec > 0 { min_expire = Some(min_expire.map_or(sec, |m| m.min(sec))); }
                        }
                    }
                }

                let mut current_sub_nodes = Vec::new();
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
                    current_sub_nodes.push(formatted);
                    all_proxies.push(node);
                }

                // Optimization: only create sub-group if sub has more than 1 node
                if current_sub_nodes.len() > 1 {
                    let sub_group_name = format!("📦 订阅源 · {}", sub.name);
                    sub_group_map.push((sub_group_name, current_sub_nodes));
                }
            }
            Err(e) => {
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

    let mut info_nodes: Vec<ProxyNode> = Vec::new();
    let mut info_node_names: Vec<String> = Vec::new();

    if agg_total > 0 {
        let used_bytes = agg_upload + agg_download;
        let used_str = format_bytes_human(used_bytes);
        let total_str = format_bytes_human(agg_total);
        let pct = if agg_total > 0 { ((used_bytes as f64 / agg_total as f64) * 100.0).round() as u32 } else { 0 };
        let traffic_name = format!("📊 流量: {} / {} ({}%)", used_str, total_str, pct);
        let mut p = ProxyNode::default();
        p.name = traffic_name.clone();
        p.server = "127.0.0.1".into();
        p.port = 80;
        p.node_type = "compatible".into();
        info_node_names.push(traffic_name);
        info_nodes.push(p);
    }

    let expire_name = match min_expire {
        Some(exp_sec) if exp_sec > 2500000000 => "⏰ 到期: 永久有效 (无限制)".to_string(),
        Some(exp_sec) => {
            chrono::DateTime::from_timestamp(exp_sec as i64, 0)
                .map(|dt| format!("⏰ 到期: {} 到期", dt.format("%Y-%m-%d")))
                .unwrap_or_else(|| "⏰ 到期: 永久有效 (无限制)".to_string())
        }
        None => "⏰ 到期: 永久有效 (无限制)".to_string(),
    };
    let mut p = ProxyNode::default();
    p.name = expire_name.clone();
    p.server = "127.0.0.1".into();
    p.port = 80;
    p.node_type = "compatible".into();
    info_node_names.push(expire_name);
    info_nodes.push(p);

    let all_node_names: Vec<String> = all_proxies.iter().map(|p| p.name.clone()).collect();
    let mut active_proxies: Vec<ProxyNode> = Vec::new();
    active_proxies.extend(info_nodes);
    active_proxies.extend(all_proxies);

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
    let excluded_set: HashSet<String> = config.excluded_auto_test_nodes.iter().cloned().collect();
    let mut auto_test_candidates: Vec<String> = all_node_names
        .iter()
        .filter(|name| !excluded_set.contains(*name))
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

    // 3. Construct Proxy Groups in topological order (Leaf groups defined first!)
    let mut proxy_groups: Vec<serde_json::Value> = Vec::new();
    // 3.1 🚀 节点选择 (Master Selector - 放在首位作为主控)
    let mut master_selector_proxies = Vec::new();
    for info_name in &info_node_names {
        master_selector_proxies.push(info_name.clone());
    }
    master_selector_proxies.push("⚡ 自动优选 (全部源)".to_string());
    master_selector_proxies.push("🛡️ 故障转移 (全部源)".to_string());
    for (auto_name, _, _) in &region_groups {
        master_selector_proxies.push(auto_name.clone());
    }
    for (sg_name, _) in &sub_group_map {
        master_selector_proxies.push(sg_name.clone());
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

    // 3.2 ⚡ 自动优选 (全部源) & 🛡️ 故障转移 (全部源)
    proxy_groups.push(serde_json::json!({
        "name": "⚡ 自动优选 (全部源)",
        "type": "url-test",
        "url": "http://www.gstatic.com/generate_204",
        "interval": 300,
        "tolerance": 50,
        "lazy": true,
        "proxies": final_auto_test_proxies
    }));

    proxy_groups.push(serde_json::json!({
        "name": "🛡️ 故障转移 (全部源)",
        "type": "fallback",
        "url": "http://www.gstatic.com/generate_204",
        "interval": 300,
        "lazy": true,
        "proxies": final_auto_test_proxies
    }));

    // 3.3 Regional URLTest & Fallback groups
    for (auto_name, fallback_name, node_names) in &region_groups {
        proxy_groups.push(serde_json::json!({
            "name": auto_name,
            "type": "url-test",
            "url": "http://www.gstatic.com/generate_204",
            "interval": 300,
            "tolerance": 50,
            "lazy": true,
            "hidden": true,
            "proxies": node_names
        }));
        proxy_groups.push(serde_json::json!({
            "name": fallback_name,
            "type": "fallback",
            "url": "http://www.gstatic.com/generate_204",
            "interval": 300,
            "lazy": true,
            "hidden": true,
            "proxies": node_names
        }));
    }

    // 3.4 Upstream Sub Groups
    for (sg_name, node_names) in &sub_group_map {
        let mut sg_proxies = node_names.clone();
        sg_proxies.push("DIRECT".to_string());
        sg_proxies.retain(|s| !s.trim().is_empty());
        sg_proxies.dedup();
        proxy_groups.push(serde_json::json!({
            "name": sg_name,
            "type": "select",
            "hidden": true,
            "proxies": sg_proxies
        }));
    }

    // 3.5 Scenario Groups Proxies List
    let mut scenario_proxies = Vec::new();
    scenario_proxies.push(main_proxy_group.to_string());
    scenario_proxies.push("⚡ 自动优选 (全部源)".to_string());
    scenario_proxies.push("🛡️ 故障转移 (全部源)".to_string());
    for (auto_name, _, _) in &region_groups {
        scenario_proxies.push(auto_name.clone());
    }
    for (sg_name, _) in &sub_group_map {
        scenario_proxies.push(sg_name.clone());
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
    direct_first_proxies.push("⚡ 自动优选 (全部源)".to_string());
    direct_first_proxies.push("🛡️ 故障转移 (全部源)".to_string());
    for (auto_name, _, _) in &region_groups {
        direct_first_proxies.push(auto_name.clone());
    }
    for (sg_name, _) in &sub_group_map {
        direct_first_proxies.push(sg_name.clone());
    }
    for name in &all_node_names {
        direct_first_proxies.push(name.clone());
    }
    direct_first_proxies.retain(|s| !s.trim().is_empty());
    direct_first_proxies.dedup();

    // 3.6 Scenario groups
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
    for kw in &config.direct_keywords {
        if !kw.trim().is_empty() { rules.push(format!("DOMAIN-KEYWORD,{},DIRECT", kw.trim())); }
    }
    for dom in &config.direct_domains {
        if !dom.trim().is_empty() { rules.push(format!("DOMAIN-SUFFIX,{},DIRECT", dom.trim())); }
    }

    // High priority proxy
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

    // Scenario rules
    if config.enable_ai_group {
        for d in &["openai.com", "chatgpt.com", "anthropic.com", "claude.ai", "oaistatic.com", "oaiusercontent.com", "gemini.google.com", "x.ai", "grok.com", "mistral.ai", "copilot.microsoft.com", "perplexity.ai"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🤖 AI 专线", d));
        }
    }
    if config.enable_media_group {
        for d in &["youtube.com", "googlevideo.com", "ytimg.com", "netflix.com", "nflxvideo.net", "disneyplus.com", "spotify.com", "hulu.com", "hbo.com", "max.com", "primevideo.com", "bilibili.tv", "bahamut.com.tw"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🎬 国际流媒体", d));
        }
    }
    if config.enable_telegram_group {
        for ip in &["91.108.4.0/22", "91.108.8.0/22", "91.108.12.0/22", "91.108.16.0/22", "91.108.20.0/22", "91.108.56.0/22", "149.154.160.0/20", "149.154.164.0/22", "149.154.168.0/22", "149.154.172.0/22"] {
            rules.push(format!("IP-CIDR,{},📲 Telegram{}", ip, no_resolve));
        }
        for d in &["t.me", "telegram.org", "telegram.me", "tdesktop.com", "telesco.pe"] {
            rules.push(format!("DOMAIN-SUFFIX,{},📲 Telegram", d));
        }
    }
    if config.enable_game_group {
        for d in &["steampowered.com", "steamcommunity.com", "steamgames.com", "epicgames.com", "ea.com", "origin.com", "playstation.com", "playstation.net", "xboxlive.com", "battle.net", "riotgames.com"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🎮 游戏平台", d));
        }
    }
    if config.enable_apple_group {
        for d in &["apple.com", "icloud.com", "itunes.com", "apple-cloudkit.com", "microsoft.com", "windowsupdate.com", "office.com", "live.com", "azure.com"] {
            rules.push(format!("DOMAIN-SUFFIX,{},🍎 Apple / 微软", d));
        }
    }

    if config.enable_loyalsoldier {
        rules.push("RULE-SET,applications,DIRECT".into());
        rules.push("RULE-SET,reject,REJECT".into());
        rules.push(format!("RULE-SET,proxy,{}", main_proxy_group));
        rules.push(format!("RULE-SET,gfw,{}", main_proxy_group));
        rules.push(format!("RULE-SET,tld-not-cn,{}", main_proxy_group));
        if config.enable_telegram_group {
            rules.push("RULE-SET,telegramcidr,📲 Telegram".into());
        }
        rules.push("RULE-SET,direct,DIRECT".into());
        rules.push("RULE-SET,lancidr,DIRECT".into());
        rules.push("RULE-SET,cncidr,DIRECT".into());
    }

    rules.push("GEOSITE,private,DIRECT".into());
    if config.enable_geo_site_cn {
        rules.push("GEOSITE,cn,DIRECT".into());
    }
    rules.push(format!("GEOIP,LAN,DIRECT{}", no_resolve));
    if config.enable_geo_ip_cn {
        rules.push(format!("GEOIP,CN,DIRECT{}", no_resolve));
    }

    let final_group = if config.enable_final_group { "🐟 漏网之鱼" } else if config.fallback_rule == "DIRECT" { "DIRECT" } else { main_proxy_group };
    rules.push(format!("MATCH,{}", final_group));

    let mut clash_map = serde_json::Map::new();
    clash_map.insert("mixed-port".into(), serde_json::json!(7890));
    clash_map.insert("allow-lan".into(), serde_json::json!(true));
    clash_map.insert("mode".into(), serde_json::json!("rule"));
    clash_map.insert("log-level".into(), serde_json::json!("info"));
    clash_map.insert("ipv6".into(), serde_json::json!(false));

    if config.enable_tcp_concurrent {
        clash_map.insert("tcp-concurrent".into(), serde_json::json!(true));
    }
    if config.enable_unified_delay {
        clash_map.insert("unified-delay".into(), serde_json::json!(true));
    }
    if config.enable_process_strict {
        clash_map.insert("find-process-mode".into(), serde_json::json!("strict"));
    }

    let mut dns_cfg = serde_json::Map::new();
    dns_cfg.insert("enable".into(), serde_json::json!(true));
    dns_cfg.insert("ipv6".into(), serde_json::json!(false));
    dns_cfg.insert("enhanced-mode".into(), serde_json::json!("fake-ip"));
    dns_cfg.insert("fake-ip-range".into(), serde_json::json!("198.18.0.1/16"));
    dns_cfg.insert("default-nameserver".into(), serde_json::json!(["223.5.5.5", "119.29.29.29", "1.1.1.1"]));
    dns_cfg.insert("nameserver".into(), serde_json::to_value(&config.nameservers).unwrap_or_default());
    dns_cfg.insert("fallback".into(), serde_json::to_value(&config.fallback_dns).unwrap_or_default());
    dns_cfg.insert("fake-ip-filter".into(), serde_json::to_value(if config.fake_ip_filter.is_empty() {
        vec!["*.lan".to_string(), "*.local".to_string()]
    } else {
        config.fake_ip_filter.clone()
    }).unwrap_or_default());
    dns_cfg.insert("fallback-filter".into(), serde_json::json!({
        "geoip": true,
        "geoip-code": "CN",
        "ipcidr": ["240.0.0.0/4"]
    }));
    dns_cfg.insert("nameserver-policy".into(), serde_json::json!({
        "geosite:cn,private": serde_json::to_value(&config.nameservers).unwrap_or_default()
    }));
    clash_map.insert("dns".into(), serde_json::Value::Object(dns_cfg));

    if config.enable_sniffer {
        clash_map.insert("sniffer".into(), serde_json::json!({
            "enable": true,
            "sniff": {
                "TLS": { "ports": [443, 8443] },
                "HTTP": { "ports": [80, "8080-8880"], "override-destination": true },
                "QUIC": { "ports": [443, 8443] }
            },
            "skip-domain": ["Mijia Cloud", "dlg.io.mi.com", "+.apple.com"]
        }));
    }

    if config.enable_loyalsoldier {
        let rule_providers = serde_json::json!({
            "applications": { "type": "http", "behavior": "classical", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/applications.txt", "path": "./ruleset/loyalsoldier/applications.txt", "interval": 86400 },
            "reject": { "type": "http", "behavior": "domain", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/reject.txt", "path": "./ruleset/loyalsoldier/reject.txt", "interval": 86400 },
            "proxy": { "type": "http", "behavior": "domain", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/proxy.txt", "path": "./ruleset/loyalsoldier/proxy.txt", "interval": 86400 },
            "gfw": { "type": "http", "behavior": "domain", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/gfw.txt", "path": "./ruleset/loyalsoldier/gfw.txt", "interval": 86400 },
            "tld-not-cn": { "type": "http", "behavior": "domain", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/tld-not-cn.txt", "path": "./ruleset/loyalsoldier/tld-not-cn.txt", "interval": 86400 },
            "telegramcidr": { "type": "http", "behavior": "ipcidr", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/telegramcidr.txt", "path": "./ruleset/loyalsoldier/telegramcidr.txt", "interval": 86400 },
            "direct": { "type": "http", "behavior": "domain", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/direct.txt", "path": "./ruleset/loyalsoldier/direct.txt", "interval": 86400 },
            "cncidr": { "type": "http", "behavior": "ipcidr", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/cncidr.txt", "path": "./ruleset/loyalsoldier/cncidr.txt", "interval": 86400 },
            "lancidr": { "type": "http", "behavior": "ipcidr", "format": "text", "url": "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/lancidr.txt", "path": "./ruleset/loyalsoldier/lancidr.txt", "interval": 86400 }
        });
        clash_map.insert("rule-providers".into(), rule_providers);
    }

    let cleaned_proxies: Vec<serde_json::Value> = active_proxies.iter().map(sanitize_proxy_node_for_clash).collect();
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

        // Check YAML contains info nodes
        assert!(res.yaml.contains("📊 流量:"));
        assert!(res.yaml.contains("⏰ 到期:"));
        assert!(res.yaml.contains("type: compatible"));
    }
}
