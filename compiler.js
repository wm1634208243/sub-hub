/**
 * Rule Hub - JavaScript Override Script Compiler v2.3
 * Compiles GUI structured rules into a clean, high-performance Clash / Mihomo main(config, profileName) script.
 * Features:
 * - Multi-platform selective export & Client User-Agent auto-detection (macOS, Windows, iOS, Android)
 * - Smart Proxy Group selection & custom group support
 * - DNS / Fake-IP tuning, Sniffer, TCP Concurrent, and no-resolve optimization
 */

export function compileConfigToJs(data, clientUa = '') {
  if (data.mode === 'custom' && data.customScript && data.customScript.trim()) {
    return data.customScript;
  }

  const {
    proxyIps = [],
    proxyDomains = [],
    proxyKeywords = [],
    proxyProcesses = [],
    directIps = [],
    directDomains = [],
    directKeywords = [],
    directProcesses = [],
    fakeIpFilter = [],
    nameservers = ['223.5.5.5', '119.29.29.29'],
    fallbackDns = ['https://1.1.1.1/dns-query', 'https://8.8.8.8/dns-query'],
    fallbackRule = 'DIRECT', // 'DIRECT' or 'PROXY'
    enableGeoSiteCn = true,
    enableGeoIpCn = true,
    enableSniffer = true,
    enableTcpConcurrent = true,
    enableNoResolve = true,
    enableUnifiedDelay = true,
    enableProcessStrict = true,
    customProxyGroupName = '',
    enableAiGroup = true,
    enableMediaGroup = true,
    enableTelegramGroup = true,
    enableGameGroup = true,
    enableAppleGroup = true,
    enableAdBlock = true,
    enableFinalGroup = true,
    enableLoyalsoldier = true,
    targetPlatforms = ['macos', 'windows', 'ios', 'android'],
    enableAutoPlatformDetect = true
  } = data;

  const noResolveSuffix = enableNoResolve ? ',no-resolve' : '';

  // Determine if process rules should be emitted
  const isDesktopPlatformSelected = targetPlatforms.includes('macos') || targetPlatforms.includes('windows');
  let allowProcessRules = isDesktopPlatformSelected;

  if (enableAutoPlatformDetect && clientUa) {
    const isMobileUa = /iPhone|iPad|iPod|iOS|Stash|Android|CFNetwork/i.test(clientUa);
    if (isMobileUa) {
      allowProcessRules = false;
    }
  }

  const script = `// =========================================================================
// Clash / Mihomo 预处理脚本 (由 Sub Hub 自动化生成 v3.0)
// 生成时间: ${new Date().toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' })}
// 目标平台: ${targetPlatforms.join(', ')} (UA自动适配: ${enableAutoPlatformDetect ? '开启' : '关闭'})
// =========================================================================

function main(config, profileName) {
  if (!config || typeof config !== "object") {
    console.error("config 未正确传入");
    return config || {};
  }

  // 1. 核心加速与全局配置调优
${enableTcpConcurrent ? '  config["tcp-concurrent"] = true; // 开启 TCP 并发握手连接加速\n' : ''}${enableUnifiedDelay ? '  config["unified-delay"] = true;  // 统一 RTT 延迟测速标准\n' : ''}${enableProcessStrict ? '  config["find-process-mode"] = "strict"; // 严格匹配本地进程规则\n' : ''}
  // 2. 动态智能识别主代理策略组名称
  let targetProxyName = ${customProxyGroupName ? JSON.stringify(customProxyGroupName) : '(profileName || "Proxy")'};
${!customProxyGroupName ? `  if (config["proxy-groups"] && Array.isArray(config["proxy-groups"]) && config["proxy-groups"].length > 0) {
    const preferred = config["proxy-groups"].find(g => 
      /节点选择|手动选择|PROXY|Proxy|PROXIES|🚀|主代理|选择节点/i.test(g.name)
    );
    targetProxyName = preferred ? preferred.name : config["proxy-groups"][0].name;
  }` : ''}

  // 3. 动态注入精细化场景策略组
  if (Array.isArray(config["proxy-groups"])) {
    const existingGroupNames = new Set(config["proxy-groups"].map(g => g.name));
    const allProxiesList = config.proxies ? config.proxies.map(p => p.name) : [];

    const scenarioGroups = [
${enableAiGroup ? '      { name: "🤖 AI 专线", type: "select", proxies: [targetProxyName, "DIRECT", ...allProxiesList] },\n' : ''}${enableMediaGroup ? '      { name: "🎬 国际流媒体", type: "select", proxies: [targetProxyName, "DIRECT", ...allProxiesList] },\n' : ''}${enableTelegramGroup ? '      { name: "📲 Telegram", type: "select", proxies: [targetProxyName, "DIRECT", ...allProxiesList] },\n' : ''}${enableGameGroup ? '      { name: "🎮 游戏平台", type: "select", proxies: ["DIRECT", targetProxyName, ...allProxiesList] },\n' : ''}${enableAppleGroup ? '      { name: "🍎 Apple / 微软", type: "select", proxies: ["DIRECT", targetProxyName, ...allProxiesList] },\n' : ''}${enableFinalGroup ? '      { name: "🐟 漏网之鱼", type: "select", proxies: [targetProxyName, "DIRECT", ...allProxiesList] },\n' : ''}    ];

    scenarioGroups.forEach(sg => {
      if (!existingGroupNames.has(sg.name)) {
        config["proxy-groups"].push(sg);
      }
    });
  }

  // 4. 强制用户自定义高优先规则
  const forceDirectRules = [
    // 强制直连 IP 列表
${directIps.map(ip => `    "IP-CIDR,${ip.trim()},DIRECT${noResolveSuffix}",`).join('\n')}

${allowProcessRules ? `    // 强制直连进程列表\n` + directProcesses.map(p => `    "PROCESS-NAME,${p.trim()},DIRECT",`).join('\n') : '    // 当前客户端平台自动跳过 PROCESS-NAME 进程规则'}

    // 强制直连关键词与域名后缀
${directKeywords.map(kw => `    "DOMAIN-KEYWORD,${kw.trim()},DIRECT",`).join('\n')}
${directDomains.map(d => `    "DOMAIN-SUFFIX,${d.trim()},DIRECT",`).join('\n')}
  ].filter(Boolean);

  const scenarioRules = [
${enableAdBlock ? `    // 🛑 广告拦截规则
    "DOMAIN-SUFFIX,adservice.google.com,REJECT",
    "DOMAIN-SUFFIX,googleadservices.com,REJECT",
    "DOMAIN-SUFFIX,doubleclick.net,REJECT",
    "DOMAIN-SUFFIX,googlesyndication.com,REJECT",
    "DOMAIN-KEYWORD,adservice,REJECT",
    "DOMAIN-KEYWORD,adserver,REJECT",
    "DOMAIN-KEYWORD,telemetry,REJECT",\n` : ''}${enableAiGroup ? `    // 🤖 AI 专线规则
    "DOMAIN-SUFFIX,openai.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,chatgpt.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,ai.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,oaistatic.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,oaiusercontent.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,anthropic.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,claude.ai,🤖 AI 专线",
    "DOMAIN-SUFFIX,gemini.google.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,copilot.microsoft.com,🤖 AI 专线",
    "DOMAIN-SUFFIX,deepseek.com,🤖 AI 专线",
    "DOMAIN-KEYWORD,openai,🤖 AI 专线",
    "DOMAIN-KEYWORD,anthropic,🤖 AI 专线",
    "DOMAIN-KEYWORD,claude,🤖 AI 专线",
    "DOMAIN-KEYWORD,chatgpt,🤖 AI 专线",\n` : ''}${enableMediaGroup ? `    // 🎬 国际流媒体规则
    "DOMAIN-SUFFIX,youtube.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,googlevideo.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,ytimg.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,netflix.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,netflix.net,🎬 国际流媒体",
    "DOMAIN-SUFFIX,nflxvideo.net,🎬 国际流媒体",
    "DOMAIN-SUFFIX,disneyplus.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,spotify.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,tiktok.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,hbo.com,🎬 国际流媒体",
    "DOMAIN-SUFFIX,twitch.tv,🎬 国际流媒体",\n` : ''}${enableTelegramGroup ? `    // 📲 Telegram 规则
    "IP-CIDR,91.108.4.0/22,📲 Telegram${noResolveSuffix}",
    "IP-CIDR,91.108.8.0/22,📲 Telegram${noResolveSuffix}",
    "IP-CIDR,91.108.12.0/22,📲 Telegram${noResolveSuffix}",
    "IP-CIDR,91.108.56.0/22,📲 Telegram${noResolveSuffix}",
    "IP-CIDR,149.154.160.0/20,📲 Telegram${noResolveSuffix}",
    "IP-CIDR,149.154.164.0/22,📲 Telegram${noResolveSuffix}",
    "DOMAIN-SUFFIX,telegram.org,📲 Telegram",
    "DOMAIN-SUFFIX,t.me,📲 Telegram",
    "DOMAIN-SUFFIX,telegram.me,📲 Telegram",\n` : ''}${enableGameGroup ? `    // 🎮 游戏平台规则
    "DOMAIN-SUFFIX,steampowered.com,🎮 游戏平台",
    "DOMAIN-SUFFIX,steamcommunity.com,🎮 游戏平台",
    "DOMAIN-SUFFIX,epicgames.com,🎮 游戏平台",
    "DOMAIN-SUFFIX,ea.com,🎮 游戏平台",
    "DOMAIN-SUFFIX,riotgames.com,🎮 游戏平台",
    "DOMAIN-SUFFIX,blizzard.com,🎮 游戏平台",
    "DOMAIN-SUFFIX,playstation.com,🎮 游戏平台",\n` : ''}${enableAppleGroup ? `    // 🍎 Apple / 微软服务规则
    "DOMAIN-SUFFIX,apple.com,🍎 Apple / 微软",
    "DOMAIN-SUFFIX,icloud.com,🍎 Apple / 微软",
    "DOMAIN-SUFFIX,microsoft.com,🍎 Apple / 微软",
    "DOMAIN-SUFFIX,windowsupdate.com,🍎 Apple / 微软",
    "DOMAIN-SUFFIX,github.com,🍎 Apple / 微软",
    "DOMAIN-SUFFIX,githubusercontent.com,🍎 Apple / 微软",\n` : ''}  ].filter(Boolean);

  const forceProxyRules = [
    // 强制代理 IP 列表
${proxyIps.map(ip => `    \`IP-CIDR,${ip.trim()},$\{targetProxyName\}${noResolveSuffix}\`,`).join('\n')}

${allowProcessRules ? `    // 强制代理进程列表 (macOS + Windows)\n` + proxyProcesses.map(p => `    \`PROCESS-NAME,${p.trim()},$\{targetProxyName\}\`,`).join('\n') : '    // 当前客户端平台自动跳过 PROCESS-NAME 进程规则'}

    // 强制代理域名与关键词列表
${proxyKeywords.map(kw => `    \`DOMAIN-KEYWORD,${kw.trim()},$\{targetProxyName\}\`,`).join('\n')}
${proxyDomains.map(d => `    \`DOMAIN-SUFFIX,${d.trim()},$\{targetProxyName\}\`,`).join('\n')}
  ].filter(Boolean);

  if (!Array.isArray(config.rules)) {
    config.rules = [];
  }

  // 🌟 剔除原订阅配置文件中夹在半路上的旧 MATCH 规则
  config.rules = config.rules.filter(rule => typeof rule === 'string' ? !rule.startsWith("MATCH,") : true);

  // 将用户直连规则、场景规则、用户代理规则插入最上方
  const myRules = [...forceDirectRules, ...scenarioRules, ...forceProxyRules];
  config.rules.unshift(...myRules);

  // 4. DNS 与 Fake-IP 防泄漏高级配置
  config.dns = config.dns || {};
  Object.assign(config.dns, {
    enable: true,
    ipv6: false,
    "enhanced-mode": "fake-ip",
    "fake-ip-range": "198.18.0.1/16",
    "default-nameserver": ["223.5.5.5", "119.29.29.29", "1.1.1.1"],
    "fake-ip-filter": ${JSON.stringify(fakeIpFilter, null, 6)},
    nameserver: ${JSON.stringify(nameservers)},
    fallback: ${JSON.stringify(fallbackDns)},
    "fallback-filter": {
      geoip: true,
      "geoip-code": "CN",
      ipcidr: ["240.0.0.0/4"],
    },
    "nameserver-policy": {
      "geosite:cn,private": ${JSON.stringify(nameservers)},
    },
  });
${enableSniffer ? `
  // 5. 域名嗅探器 (精准分流直连 IP 流量)
  config.sniffer = {
    enable: true,
    sniff: {
      TLS: { ports: [443, 8443] },
      HTTP: { ports: [80, "8080-8880"], "override-destination": true },
      QUIC: { ports: [443, 8443] }
    },
    "skip-domain": ["Mijia Cloud", "dlg.io.mi.com", "+.apple.com"]
  };` : ''}

${enableLoyalsoldier ? `
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
  });` : ''}

  // 7. 规则末尾追加
  const tailRules = [
${enableLoyalsoldier ? `    "RULE-SET,applications,DIRECT",
    "RULE-SET,reject,REJECT",
    \`RULE-SET,proxy,$\{targetProxyName\}\`,
    \`RULE-SET,gfw,$\{targetProxyName\}\`,
    \`RULE-SET,tld-not-cn,$\{targetProxyName\}\`,
    "RULE-SET,direct,DIRECT",
    "RULE-SET,lancidr,DIRECT${noResolveSuffix}",
    "RULE-SET,cncidr,DIRECT${noResolveSuffix}",\n` : ''}    "GEOSITE,private,DIRECT",
${enableGeoSiteCn ? '    "GEOSITE,cn,DIRECT",' : ''}
    "GEOIP,LAN,DIRECT${noResolveSuffix}",
${enableGeoIpCn ? `    "GEOIP,CN,DIRECT${noResolveSuffix}",` : ''}
    ${fallbackRule === 'DIRECT' ? '"MATCH,DIRECT"' : '`MATCH,${targetProxyName}`'}
  ].filter(Boolean);

  config.rules.push(...tailRules);

  return config;
}
`;

  return script;
}
