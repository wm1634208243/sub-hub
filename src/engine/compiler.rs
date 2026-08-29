use crate::models::UserConfig;

pub fn compile_config_to_js(data: &UserConfig, client_ua: &str) -> String {
    if data.mode == "custom" {
        if let Some(custom) = &data.custom_script {
            if !custom.trim().is_empty() {
                return custom.clone();
            }
        }
    }

    let no_resolve_suffix = if data.enable_no_resolve { ",no-resolve" } else { "" };

    let is_desktop_selected = data.target_platforms.iter().any(|p| p == "macos" || p == "windows");
    let mut allow_process_rules = is_desktop_selected;

    if data.enable_auto_platform_detect && !client_ua.is_empty() {
        let is_mobile = client_ua.contains("iPhone") || client_ua.contains("iPad")
            || client_ua.contains("iPod") || client_ua.contains("iOS")
            || client_ua.contains("Stash") || client_ua.contains("Android")
            || client_ua.contains("CFNetwork");
        if is_mobile {
            allow_process_rules = false;
        }
    }

    let mut direct_rules_code = Vec::new();
    for ip in &data.direct_ips {
        if !ip.trim().is_empty() {
            direct_rules_code.push(format!("    \"IP-CIDR,{},DIRECT{}\",", ip.trim(), no_resolve_suffix));
        }
    }
    if allow_process_rules {
        direct_rules_code.push("    // 强制直连进程列表".into());
        for p in &data.direct_processes {
            if !p.trim().is_empty() {
                direct_rules_code.push(format!("    \"PROCESS-NAME,{},DIRECT\",", p.trim()));
            }
        }
    } else {
        direct_rules_code.push("    // 当前客户端平台自动跳过 PROCESS-NAME 进程规则".into());
    }
    for kw in &data.direct_keywords {
        if !kw.trim().is_empty() {
            direct_rules_code.push(format!("    \"DOMAIN-KEYWORD,{},DIRECT\",", kw.trim()));
        }
    }
    for d in &data.direct_domains {
        if !d.trim().is_empty() {
            direct_rules_code.push(format!("    \"DOMAIN-SUFFIX,{},DIRECT\",", d.trim()));
        }
    }

    let mut proxy_rules_code = Vec::new();
    for ip in &data.proxy_ips {
        if !ip.trim().is_empty() {
            proxy_rules_code.push(format!("    `IP-CIDR,{},${{targetProxyName}}{}`,", ip.trim(), no_resolve_suffix));
        }
    }
    if allow_process_rules {
        proxy_rules_code.push("    // 强制代理进程列表 (macOS + Windows)".into());
        for p in &data.proxy_processes {
            if !p.trim().is_empty() {
                proxy_rules_code.push(format!("    `PROCESS-NAME,{},${{targetProxyName}}`,", p.trim()));
            }
        }
    } else {
        proxy_rules_code.push("    // 当前客户端平台自动跳过 PROCESS-NAME 进程规则".into());
    }
    for kw in &data.proxy_keywords {
        if !kw.trim().is_empty() {
            proxy_rules_code.push(format!("    `DOMAIN-KEYWORD,{},${{targetProxyName}}`,", kw.trim()));
        }
    }
    for d in &data.proxy_domains {
        if !d.trim().is_empty() {
            proxy_rules_code.push(format!("    `DOMAIN-SUFFIX,{},${{targetProxyName}}`,", d.trim()));
        }
    }

    let mut scenario_rules_code: Vec<String> = Vec::new();
    if data.enable_ad_block {
        scenario_rules_code.push("    // 🛑 广告拦截规则\n    \"DOMAIN-SUFFIX,adservice.google.com,REJECT\",\n    \"DOMAIN-SUFFIX,googleadservices.com,REJECT\",\n    \"DOMAIN-SUFFIX,doubleclick.net,REJECT\",\n    \"DOMAIN-SUFFIX,googlesyndication.com,REJECT\",\n    \"DOMAIN-KEYWORD,adservice,REJECT\",\n    \"DOMAIN-KEYWORD,adserver,REJECT\",\n    \"DOMAIN-KEYWORD,telemetry,REJECT\",".into());
    }
    if data.enable_ai_group {
        scenario_rules_code.push("    // 🤖 AI 专线规则\n    \"DOMAIN-SUFFIX,openai.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,chatgpt.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,ai.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,oaistatic.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,oaiusercontent.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,anthropic.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,claude.ai,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,gemini.google.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,copilot.microsoft.com,🤖 AI 专线\",\n    \"DOMAIN-SUFFIX,deepseek.com,🤖 AI 专线\",\n    \"DOMAIN-KEYWORD,openai,🤖 AI 专线\",\n    \"DOMAIN-KEYWORD,anthropic,🤖 AI 专线\",\n    \"DOMAIN-KEYWORD,claude,🤖 AI 专线\",\n    \"DOMAIN-KEYWORD,chatgpt,🤖 AI 专线\",".into());
    }
    if data.enable_media_group {
        scenario_rules_code.push("    // 🎬 国际流媒体规则\n    \"DOMAIN-SUFFIX,youtube.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,googlevideo.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,ytimg.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,netflix.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,netflix.net,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,nflxvideo.net,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,disneyplus.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,spotify.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,tiktok.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,hbo.com,🎬 国际流媒体\",\n    \"DOMAIN-SUFFIX,twitch.tv,🎬 国际流媒体\",".into());
    }
    if data.enable_telegram_group {
        scenario_rules_code.push(format!("    // 📲 Telegram 规则\n    \"IP-CIDR,91.108.4.0/22,📲 Telegram{}\",\n    \"IP-CIDR,91.108.8.0/22,📲 Telegram{}\",\n    \"IP-CIDR,91.108.12.0/22,📲 Telegram{}\",\n    \"IP-CIDR,91.108.56.0/22,📲 Telegram{}\",\n    \"IP-CIDR,149.154.160.0/20,📲 Telegram{}\",\n    \"IP-CIDR,149.154.164.0/22,📲 Telegram{}\",\n    \"DOMAIN-SUFFIX,telegram.org,📲 Telegram\",\n    \"DOMAIN-SUFFIX,t.me,📲 Telegram\",\n    \"DOMAIN-SUFFIX,telegram.me,📲 Telegram\",", no_resolve_suffix, no_resolve_suffix, no_resolve_suffix, no_resolve_suffix, no_resolve_suffix, no_resolve_suffix));
    }
    if data.enable_game_group {
        scenario_rules_code.push("    // 🎮 游戏平台规则\n    \"DOMAIN-SUFFIX,steampowered.com,🎮 游戏平台\",\n    \"DOMAIN-SUFFIX,steamcommunity.com,🎮 游戏平台\",\n    \"DOMAIN-SUFFIX,epicgames.com,🎮 游戏平台\",\n    \"DOMAIN-SUFFIX,ea.com,🎮 游戏平台\",\n    \"DOMAIN-SUFFIX,riotgames.com,🎮 游戏平台\",\n    \"DOMAIN-SUFFIX,blizzard.com,🎮 游戏平台\",\n    \"DOMAIN-SUFFIX,playstation.com,🎮 游戏平台\",".into());
    }
    if data.enable_apple_group {
        scenario_rules_code.push("    // 🍎 Apple / 微软服务规则\n    \"DOMAIN-SUFFIX,apple.com,🍎 Apple / 微软\",\n    \"DOMAIN-SUFFIX,icloud.com,🍎 Apple / 微软\",\n    \"DOMAIN-SUFFIX,microsoft.com,🍎 Apple / 微软\",\n    \"DOMAIN-SUFFIX,windowsupdate.com,🍎 Apple / 微软\",\n    \"DOMAIN-SUFFIX,github.com,🍎 Apple / 微软\",\n    \"DOMAIN-SUFFIX,githubusercontent.com,🍎 Apple / 微软\",".into());
    }

    let fake_ip_filter_json = serde_json::to_string_pretty(&data.fake_ip_filter).unwrap_or_else(|_| "[]".into());
    let nameservers_json = serde_json::to_string(&data.nameservers).unwrap_or_else(|_| "[\"223.5.5.5\", \"119.29.29.29\"]".into());
    let fallback_dns_json = serde_json::to_string(&data.fallback_dns).unwrap_or_else(|_| "[\"https://1.1.1.1/dns-query\", \"https://8.8.8.8/dns-query\"]".into());

    let target_proxy_init = match &data.custom_proxy_group_name {
        Some(custom) if !custom.trim().is_empty() => format!("let targetProxyName = \"{}\";", custom.trim()),
        _ => r#"let targetProxyName = (profileName || "Proxy");
  if (config["proxy-groups"] && Array.isArray(config["proxy-groups"]) && config["proxy-groups"].length > 0) {
    const preferred = config["proxy-groups"].find(g => 
      /节点选择|手动选择|PROXY|Proxy|PROXIES|🚀|主代理|选择节点/i.test(g.name)
    );
    targetProxyName = preferred ? preferred.name : config["proxy-groups"][0].name;
  }"#.into(),
    };

    let mut scenario_groups_defs = Vec::new();
    if data.enable_ai_group {
        scenario_groups_defs.push("      { name: \"🤖 AI 专线\", type: \"select\", proxies: [targetProxyName, \"DIRECT\", ...allProxiesList] },");
    }
    if data.enable_media_group {
        scenario_groups_defs.push("      { name: \"🎬 国际流媒体\", type: \"select\", proxies: [targetProxyName, \"DIRECT\", ...allProxiesList] },");
    }
    if data.enable_telegram_group {
        scenario_groups_defs.push("      { name: \"📲 Telegram\", type: \"select\", proxies: [targetProxyName, \"DIRECT\", ...allProxiesList] },");
    }
    if data.enable_game_group {
        scenario_groups_defs.push("      { name: \"🎮 游戏平台\", type: \"select\", proxies: [\"DIRECT\", targetProxyName, ...allProxiesList] },");
    }
    if data.enable_apple_group {
        scenario_groups_defs.push("      { name: \"🍎 Apple / 微软\", type: \"select\", proxies: [\"DIRECT\", targetProxyName, ...allProxiesList] },");
    }
    if data.enable_final_group {
        scenario_groups_defs.push("      { name: \"🐟 漏网之鱼\", type: \"select\", proxies: [targetProxyName, \"DIRECT\", ...allProxiesList] },");
    }

    let tcp_concurrent_str = if data.enable_tcp_concurrent { "  config[\"tcp-concurrent\"] = true; // 开启 TCP 并发握手连接加速\n" } else { "" };
    let unified_delay_str = if data.enable_unified_delay { "  config[\"unified-delay\"] = true;  // 统一 RTT 延迟测速标准\n" } else { "" };
    let process_strict_str = if data.enable_process_strict { "  config[\"find-process-mode\"] = \"strict\"; // 严格匹配本地进程规则\n" } else { "" };

    let sniffer_str = if data.enable_sniffer {
        r#"
  // 5. 域名嗅探器 (精准分流直连 IP 流量)
  config.sniffer = {
    enable: true,
    sniff: {
      TLS: { ports: [443, 8443] },
      HTTP: { ports: [80, "8080-8880"], "override-destination": true },
      QUIC: { ports: [443, 8443] }
    },
    "skip-domain": ["Mijia Cloud", "dlg.io.mi.com", "+.apple.com"]
  };"#
    } else {
        ""
    };

    let loyalsoldier_providers_str = if data.enable_loyalsoldier {
        r#"
  // 6. 远程规则集提供者 (Loyalsoldier 高速镜像库)
  config["rule-providers"] = config["rule-providers"] || {};
  Object.assign(config["rule-providers"], {
    applications: {
      type: "http",
      behavior: "classical",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/applications.txt",
      path: "./ruleset/loyalsoldier/applications.txt",
      interval: 86400,
    },
    reject: {
      type: "http",
      behavior: "domain",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/reject.txt",
      path: "./ruleset/loyalsoldier/reject.txt",
      interval: 86400,
    },
    proxy: {
      type: "http",
      behavior: "domain",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/proxy.txt",
      path: "./ruleset/loyalsoldier/proxy.txt",
      interval: 86400,
    },
    gfw: {
      type: "http",
      behavior: "domain",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/gfw.txt",
      path: "./ruleset/loyalsoldier/gfw.txt",
      interval: 86400,
    },
    "tld-not-cn": {
      type: "http",
      behavior: "domain",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/tld-not-cn.txt",
      path: "./ruleset/loyalsoldier/tld-not-cn.txt",
      interval: 86400,
    },
    telegramcidr: {
      type: "http",
      behavior: "ipcidr",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/telegramcidr.txt",
      path: "./ruleset/loyalsoldier/telegramcidr.txt",
      interval: 86400,
    },
    direct: {
      type: "http",
      behavior: "domain",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/direct.txt",
      path: "./ruleset/loyalsoldier/direct.txt",
      interval: 86400,
    },
    cncidr: {
      type: "http",
      behavior: "ipcidr",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/cncidr.txt",
      path: "./ruleset/loyalsoldier/cncidr.txt",
      interval: 86400,
    },
    lancidr: {
      type: "http",
      behavior: "ipcidr",
      format: "text",
      url: "https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/lancidr.txt",
      path: "./ruleset/loyalsoldier/lancidr.txt",
      interval: 86400,
    },
  });"#
    } else {
        ""
    };

    let loyalsoldier_tail = if data.enable_loyalsoldier {
        format!("    \"RULE-SET,applications,DIRECT\",\n    \"RULE-SET,reject,REJECT\",\n    `RULE-SET,proxy,${{targetProxyName}}`,\n    `RULE-SET,gfw,${{targetProxyName}}`,\n    `RULE-SET,tld-not-cn,${{targetProxyName}}`,\n    \"RULE-SET,direct,DIRECT\",\n    \"RULE-SET,lancidr,DIRECT{}\",\n    \"RULE-SET,cncidr,DIRECT{}\",\n", no_resolve_suffix, no_resolve_suffix)
    } else {
        "".into()
    };

    let geosite_cn_str = if data.enable_geo_site_cn { "    \"GEOSITE,cn,DIRECT\",\n" } else { "" };
    let geoip_cn_str = if data.enable_geo_ip_cn { format!("    \"GEOIP,CN,DIRECT{}\",\n", no_resolve_suffix) } else { "".into() };
    let fallback_rule_str = if data.fallback_rule == "DIRECT" { "\"MATCH,DIRECT\"" } else { "`MATCH,${targetProxyName}`" };

    format!(r#"// =========================================================================
// Clash / Mihomo 预处理脚本 (由 Sub Hub 自动化生成 v2.0.1)
// 生成时间: {timestamp}
// =========================================================================

function main(config, profileName) {{
  if (!config || typeof config !== "object") {{
    console.error("config 未正确传入");
    return config || {{}};
  }}

  // 1. 核心加速与全局配置调优
{tcp_concurrent_str}{unified_delay_str}{process_strict_str}
  // 2. 动态智能识别主代理策略组名称
  {target_proxy_init}

  // 3. 动态注入精细化场景策略组
  if (Array.isArray(config["proxy-groups"])) {{
    const existingGroupNames = new Set(config["proxy-groups"].map(g => g.name));
    const allProxiesList = config.proxies ? config.proxies.map(p => p.name) : [];

    const scenarioGroups = [
{scenario_groups}
    ];

    scenarioGroups.forEach(sg => {{
      if (!existingGroupNames.has(sg.name)) {{
        config["proxy-groups"].push(sg);
      }}
    }});
  }}

  // 4. 强制用户自定义高优先规则
  const forceDirectRules = [
{direct_rules}
  ].filter(Boolean);

  const scenarioRules = [
{scenario_rules}
  ].filter(Boolean);

  const forceProxyRules = [
{proxy_rules}
  ].filter(Boolean);

  if (!Array.isArray(config.rules)) {{
    config.rules = [];
  }}

  // 🌟 剔除原订阅配置文件中夹在半路上的旧 MATCH 规则
  config.rules = config.rules.filter(rule => typeof rule === 'string' ? !rule.startsWith("MATCH,") : true);

  // 将用户直连规则、场景规则、用户代理规则插入最上方
  const myRules = [...forceDirectRules, ...scenarioRules, ...forceProxyRules];
  config.rules.unshift(...myRules);

  // 4. DNS 与 Fake-IP 防泄漏高级配置
  config.dns = config.dns || {{}};
  Object.assign(config.dns, {{
    enable: true,
    ipv6: false,
    "enhanced-mode": "fake-ip",
    "fake-ip-range": "198.18.0.1/16",
    "default-nameserver": ["223.5.5.5", "119.29.29.29", "1.1.1.1"],
    "fake-ip-filter": {fake_ip_filter_json},
    nameserver: {nameservers_json},
    fallback: {fallback_dns_json},
    "fallback-filter": {{
      geoip: true,
      "geoip-code": "CN",
      ipcidr: ["240.0.0.0/4"],
    }},
    "nameserver-policy": {{
      "geosite:cn,private": {nameservers_json},
    }},
  }});{sniffer_str}{loyalsoldier_providers_str}

  // 7. 规则末尾追加
  const tailRules = [
{loyalsoldier_tail}    "GEOSITE,private,DIRECT",
{geosite_cn_str}    "GEOIP,LAN,DIRECT{no_resolve_suffix}",
{geoip_cn_str}    {fallback_rule_str}
  ].filter(Boolean);

  config.rules.push(...tailRules);

  return config;
}}
"#,
        timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        tcp_concurrent_str = tcp_concurrent_str,
        unified_delay_str = unified_delay_str,
        process_strict_str = process_strict_str,
        target_proxy_init = target_proxy_init,
        scenario_groups = scenario_groups_defs.join("\n"),
        direct_rules = direct_rules_code.join("\n"),
        scenario_rules = scenario_rules_code.join("\n"),
        proxy_rules = proxy_rules_code.join("\n"),
        fake_ip_filter_json = fake_ip_filter_json,
        nameservers_json = nameservers_json,
        fallback_dns_json = fallback_dns_json,
        sniffer_str = sniffer_str,
        loyalsoldier_providers_str = loyalsoldier_providers_str,
        loyalsoldier_tail = loyalsoldier_tail,
        geosite_cn_str = geosite_cn_str,
        no_resolve_suffix = no_resolve_suffix,
        geoip_cn_str = geoip_cn_str,
        fallback_rule_str = fallback_rule_str,
    )
}
