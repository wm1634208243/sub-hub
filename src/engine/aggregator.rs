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

    for sub in &config.subscriptions {
        if !sub.enabled || sub.url.is_empty() {
            continue;
        }

        let prefix = sub.prefix.as_deref().or(Some(&sub.name)).unwrap_or_default();
        match fetcher.fetch(&sub.url, prefix, false).await {
            Ok(res) => {
                // Aggregated userinfo
                if let Some(ui) = &res.user_info {
                    if let Some(up) = ui.get("upload").and_then(|v| v.as_u64()) { agg_upload += up; }
                    if let Some(down) = ui.get("download").and_then(|v| v.as_u64()) { agg_download += down; }
                    if let Some(tot) = ui.get("total").and_then(|v| v.as_u64()) { agg_total += tot; }
                    if let Some(exp) = ui.get("expire").and_then(|v| v.as_u64()) {
                        if exp > 0 {
                            min_expire = Some(min_expire.map_or(exp, |m| m.min(exp)));
                        }
                    }
                }

                let mut current_sub_nodes = Vec::new();
                for mut node in res.nodes {
                    let formatted = format_node_name(
                        &node.name,
                        &node.server,
                        config.enable_auto_flags,
                        config.enable_clean_ad_and_rate,
                        &config.custom_rename_rules,
                        sub.default_region.as_deref(),
                    );
                    node.name = formatted.clone();
                    current_sub_nodes.push(formatted);
                    all_proxies.push(node);
                }

                if !current_sub_nodes.is_empty() {
                    let sub_group_name = format!("📦 订阅源 · {}", sub.name);
                    sub_group_map.push((sub_group_name, current_sub_nodes));
                }
            }
            Err(e) => {
                tracing::warn!("Failed to fetch subscription {}: {}", sub.name, e);
            }
        }
    }

    // De-duplicate node names to ensure valid Clash YAML
    let mut seen_names = HashSet::new();
    let mut active_proxies = Vec::new();
    for mut p in all_proxies {
        let mut unique_name = p.name.clone();
        let mut counter = 2;
        while seen_names.contains(&unique_name) {
            unique_name = format!("{} ({})", p.name, counter);
            counter += 1;
        }
        seen_names.insert(unique_name.clone());
        p.name = unique_name;
        active_proxies.push(p);
    }

    let all_node_names: Vec<String> = active_proxies.iter().map(|p| p.name.clone()).collect();
    let main_proxy_group = config.custom_proxy_group_name.as_deref().unwrap_or("🚀 节点选择");

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

    // 3. Construct Proxy Groups
    let mut proxy_groups: Vec<serde_json::Value> = Vec::new();

    // masterSelectorProxies for 🚀 节点选择
    let mut master_selector_proxies = Vec::new();
    master_selector_proxies.push("⚡ 自动优选 (全部源)".to_string());
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
    master_selector_proxies.dedup();

    // 1. 🚀 节点选择
    proxy_groups.push(serde_json::json!({
        "name": main_proxy_group,
        "type": "select",
        "proxies": master_selector_proxies
    }));

    // 2. ⚡ 自动优选 (全部源)
    proxy_groups.push(serde_json::json!({
        "name": "⚡ 自动优选 (全部源)",
        "type": "url-test",
        "url": "http://www.gstatic.com/generate_204",
        "interval": 300,
        "tolerance": 50,
        "lazy": true,
        "proxies": final_auto_test_proxies
    }));

    // 3. 🛡️ 故障转移 (全部源)
    proxy_groups.push(serde_json::json!({
        "name": "🛡️ 故障转移 (全部源)",
        "type": "fallback",
        "url": "http://www.gstatic.com/generate_204",
        "interval": 300,
        "lazy": true,
        "proxies": final_auto_test_proxies
    }));

    // 4. Regional URLTest & Fallback groups
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

    // Common scenario proxies list
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
    scenario_proxies.dedup();

    let mut direct_first_proxies = Vec::new();
    direct_first_proxies.push("DIRECT".to_string());
    direct_first_proxies.push(main_proxy_group.to_string());
    direct_first_proxies.push("⚡ 自动优选 (全部源)".to_string());
    direct_first_proxies.push("🛡️ 故障转移 (全部源)".to_string());
    for (auto_name, _, _) in &region_groups {
        direct_first_proxies.push(auto_name.clone());
    }
    for name in &all_node_names {
        direct_first_proxies.push(name.clone());
    }
    direct_first_proxies.dedup();

    // 5. Scenario groups
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

    // 6. Sub-specific source groups (strictly only this sub's nodes)
    for (sg_name, node_names) in &sub_group_map {
        let mut sub_nodes = node_names.clone();
        sub_nodes.push("DIRECT".to_string());
        proxy_groups.push(serde_json::json!({
            "name": sg_name,
            "type": "select",
            "hidden": true,
            "proxies": sub_nodes
        }));
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
            rules.push(format!("RULE-SET,telegramcidr,📲 Telegram{}", no_resolve));
        }
        rules.push("RULE-SET,direct,DIRECT".into());
        rules.push(format!("RULE-SET,lancidr,DIRECT{}", no_resolve));
        rules.push(format!("RULE-SET,cncidr,DIRECT{}", no_resolve));
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

    // 5. Construct Complete Clash Object
    let mut clash_map = serde_json::Map::new();
    clash_map.insert("mixed-port".into(), serde_json::json!(7890));
    clash_map.insert("allow-lan".into(), serde_json::json!(true));
    clash_map.insert("mode".into(), serde_json::json!("rule"));
    clash_map.insert("log-level".into(), serde_json::json!("info"));
    clash_map.insert("ipv6".into(), serde_json::json!(false));
    clash_map.insert("tcp-concurrent".into(), serde_json::json!(config.enable_tcp_concurrent));
    clash_map.insert("unified-delay".into(), serde_json::json!(config.enable_unified_delay));
    clash_map.insert("find-process-mode".into(), serde_json::json!(if config.enable_process_strict { "strict" } else { "always" }));

    // DNS config
    let dns_cfg = serde_json::json!({
        "enable": true,
        "ipv6": false,
        "enhanced-mode": "fake-ip",
        "fake-ip-range": "198.18.0.1/16",
        "default-nameserver": ["223.5.5.5", "119.29.29.29", "1.1.1.1"],
        "fake-ip-filter": if config.fake_ip_filter.is_empty() { vec!["*.lan".into(), "*.local".into()] } else { config.fake_ip_filter.clone() },
        "nameserver": config.nameservers,
        "fallback": config.fallback_dns,
        "fallback-filter": {
            "geoip": true,
            "geoip-code": "CN",
            "ipcidr": ["240.0.0.0/4"]
        },
        "nameserver-policy": {
            "geosite:cn,private": config.nameservers.clone()
        }
    });
    clash_map.insert("dns".into(), dns_cfg);

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

    clash_map.insert("proxies".into(), serde_json::to_value(&active_proxies).unwrap_or_default());
    clash_map.insert("proxy-groups".into(), serde_json::Value::Array(proxy_groups));
    clash_map.insert("rules".into(), serde_json::to_value(rules).unwrap_or_default());

    let yaml_string = serde_yaml::to_string(&clash_map).map_err(|e| format!("YAML 序列化失败: {}", e))?;

    let mut userinfo_header = None;
    if agg_total > 0 || agg_upload > 0 || agg_download > 0 {
        let mut s = format!("upload={}; download={}; total={}", agg_upload, agg_download, agg_total);
        if let Some(exp) = min_expire {
            s.push_str(&format!("; expire={}", exp));
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
