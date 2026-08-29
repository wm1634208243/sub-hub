/**
 * Clash / Mihomo Subscription Aggregator
 * Merges nodes from multiple airport providers and node subscriptions,
 * builds smart proxy groups, attaches Rule Hub routing rules & DNS/Sniffer tuning,
 * and compiles into a production-ready Clash YAML configuration.
 */

import YAML from 'yaml';
import { fetchSubscription } from './subscription-fetcher.js';
import { PRESET_SCENARIOS } from './presets.js';
import { formatNodeName, prewarmDnsForProxies, REGION_FLAGS, detectNodePrimaryRegion } from './node-renamer.js';
import { batchProbeProxies, applyLatencyFilterAndSort } from './latency-tester.js';

export async function aggregateClashYaml(userConfig, clientUa = '') {
  const {
    subscriptions = [],
    enableAutoFlags = true,
    enableCleanAdAndRate = true,
    enableGeoIpLookup = true,
    enableDeadNodeFilter = false,
    enableLatencySort = false,
    latencyTimeoutMs = 2000,
    customRenameRules = [],
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
    fallbackRule = 'DIRECT',
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
  } = userConfig;

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

  // 1. Fetch all enabled upstream subscriptions
  const enabledSubs = subscriptions.filter(s => s.enabled !== false && s.url);
  const fetchedResults = await Promise.allSettled(
    enabledSubs.map(s => fetchSubscription(s.url, s.prefix || s.name || ''))
  );

  // Pre-resolve DNS for any domain hosts for GeoIP lookup
  await Promise.allSettled(
    fetchedResults.map(r => r.status === 'fulfilled' && r.value.nodes ? prewarmDnsForProxies(r.value.nodes) : null)
  );

  const allProxies = [];
  const nameCountMap = new Map();
  const subGroupMap = []; // { name, nodeNames }
  let aggUpload = 0, aggDownload = 0, aggTotal = 0, minExpire = null;

  fetchedResults.forEach((res, idx) => {
    if (res.status === 'fulfilled') {
      const data = res.value;
      const subInfo = enabledSubs[idx];
      const subNodes = [];

      // Collect nodes and deduplicate names
      data.nodes.forEach(p => {
        let name = formatNodeName(p, {
          enableAutoFlags,
          enableCleanAdAndRate,
          enableGeoIpLookup,
          customRenameRules,
          defaultRegion: subInfo.defaultRegion || ''
        });
        if (nameCountMap.has(name)) {
          const c = nameCountMap.get(name) + 1;
          nameCountMap.set(name, c);
          name = `${name} (${c})`;
        } else {
          nameCountMap.set(name, 0);
        }

        const cleanProxy = { ...p, name, _defaultRegion: subInfo.defaultRegion || '' };
        allProxies.push(cleanProxy);
        subNodes.push(name);
      });

      if (subNodes.length > 0) {
        subGroupMap.push({
          name: `📦 订阅源 · ${subInfo.name || subInfo.prefix || '上游订阅 ' + (idx + 1)}`,
          nodeNames: subNodes
        });
      }

      // Aggregate userinfo
      if (data.userInfo) {
        aggUpload += data.userInfo.upload || 0;
        aggDownload += data.userInfo.download || 0;
        aggTotal += data.userInfo.total || 0;
      }
      let exp = null;
      if (subInfo.customExpire) {
        exp = Math.floor(new Date(subInfo.customExpire + 'T23:59:59').getTime() / 1000);
      } else if (data.userInfo?.expire) {
        exp = Math.floor(data.userInfo.expire / 1000);
      }
      if (exp && exp > 0) {
        if (!minExpire || exp < minExpire) minExpire = exp;
      }
    }
  });

  // 1.5 Latency testing & Dead node filter
  let activeProxies = allProxies;
  if (enableDeadNodeFilter || enableLatencySort) {
    const probeRes = await batchProbeProxies(allProxies, {
      timeoutMs: Number(latencyTimeoutMs) || 2000
    });
    activeProxies = applyLatencyFilterAndSort(probeRes.proxies, {
      enableDeadNodeFilter,
      enableLatencySort
    });
  }

  // Filter sub groups to only include active nodes
  const activeNodeNameSet = new Set(activeProxies.map(p => p.name));
  const activeSubGroupMap = subGroupMap.map(sg => ({
    name: sg.name,
    nodeNames: sg.nodeNames.filter(n => activeNodeNameSet.has(n))
  })).filter(sg => sg.nodeNames.length > 0);

  // If no proxies available, inject dummy DIRECT proxy for syntactical completeness
  const allNodeNames = activeProxies.map(p => p.name);
  const mainProxyGroup = customProxyGroupName || '🚀 节点选择';

  // 1.6 精准单地区归属判定与地区自动优选/故障转移组生成
  const nodeRegionMap = new Map(); // nodeName -> primary region
  activeProxies.forEach(p => {
    const reg = detectNodePrimaryRegion(p.name, p.server, p._defaultRegion);
    if (reg) {
      nodeRegionMap.set(p.name, reg);
    }
  });

  const regionGroupMap = [];
  if (Array.isArray(REGION_FLAGS)) {
    for (const reg of REGION_FLAGS) {
      const matched = allNodeNames.filter(name => {
        const nodeReg = nodeRegionMap.get(name);
        return nodeReg && nodeReg.code === reg.code;
      });
      if (matched.length > 0) {
        regionGroupMap.push({
          name: `${reg.flag} ${reg.name}自动`,
          fallbackName: `${reg.flag} ${reg.name}故障转移`,
          flag: reg.flag,
          code: reg.code,
          regionName: reg.name,
          nodeNames: matched
        });
      }
    }
  }

  // 2. Build Proxy Groups (统一主控架构 · 自动优选整合至节点选择)
  const proxyGroups = [];

  // 节点选择主控内部可选的所有项目 (默认第 1 项为 ⚡ 自动优选，同时提供全部与各地区的自动优选与故障转移)
  const regionalAutoAndFallback = [];
  regionGroupMap.forEach(rg => {
    regionalAutoAndFallback.push(rg.name);
    regionalAutoAndFallback.push(rg.fallbackName);
  });

  const masterSelectorProxies = [
    '⚡ 自动优选 (全部源)',
    '🛡️ 故障转移 (全部源)',
    ...regionalAutoAndFallback,
    ...activeSubGroupMap.map(sg => sg.name),
    ...allNodeNames,
    'DIRECT'
  ].filter((v, i, arr) => arr.indexOf(v) === i);

  // 所有分流场景统一默认以 [🚀 节点选择] 为第 1 项，保证全局一呼百应
  const scenarioProxies = [
    mainProxyGroup,
    '⚡ 自动优选 (全部源)',
    '🛡️ 故障转移 (全部源)',
    ...regionGroupMap.map(rg => rg.name),
    ...activeSubGroupMap.map(sg => sg.name),
    ...allNodeNames,
    'DIRECT'
  ].filter((v, i, arr) => arr.indexOf(v) === i);

  const directFirstProxies = [
    'DIRECT',
    mainProxyGroup,
    '⚡ 自动优选 (全部源)',
    '🛡️ 故障转移 (全部源)',
    ...regionGroupMap.map(rg => rg.name),
    ...activeSubGroupMap.map(sg => sg.name),
    ...allNodeNames
  ].filter((v, i, arr) => arr.indexOf(v) === i);

  // 1. 🚀 节点选择 (主控总选 · 完美整合全局自动优选、地区自动与全部节点)
  proxyGroups.push({
    name: mainProxyGroup,
    type: 'select',
    proxies: [...masterSelectorProxies]
  });

  // 2. ⚡ 自动优选 (全部源) - 独立顶层卡片，实时测速自动切换最低延迟节点
  proxyGroups.push({
    name: '⚡ 自动优选 (全部源)',
    type: 'url-test',
    url: 'http://www.gstatic.com/generate_204',
    interval: 300,
    tolerance: 50,
    proxies: allNodeNames.length > 0 ? [...allNodeNames] : ['DIRECT']
  });

  // 3. 🛡️ 故障转移 (全部源) - 独立顶层卡片，主节点断连时自动顺位切换
  proxyGroups.push({
    name: '🛡️ 故障转移 (全部源)',
    type: 'fallback',
    url: 'http://www.gstatic.com/generate_204',
    interval: 300,
    proxies: allNodeNames.length > 0 ? [...allNodeNames] : ['DIRECT']
  });

  // 4. 国家/地区自动优选与故障转移组 (香港/日本/美国/新加坡自动优选与故障转移) - 标记 hidden: true
  regionGroupMap.forEach(rg => {
    // 地区自动优选 (URLTest)
    proxyGroups.push({
      name: rg.name,
      type: 'url-test',
      url: 'http://www.gstatic.com/generate_204',
      interval: 300,
      tolerance: 50,
      hidden: true,
      proxies: rg.nodeNames
    });
    // 地区故障转移 (Fallback)
    proxyGroups.push({
      name: rg.fallbackName,
      type: 'fallback',
      url: 'http://www.gstatic.com/generate_204',
      interval: 300,
      hidden: true,
      proxies: rg.nodeNames
    });
  });

  // 5. 🤖 AI 专线 (ChatGPT / Claude / Gemini) -> 统一默认跟随 🚀 节点选择
  if (enableAiGroup !== false) {
    proxyGroups.push({
      name: '🤖 AI 专线',
      type: 'select',
      proxies: [...scenarioProxies]
    });
  }

  // 6. 🎬 国际流媒体 (YouTube / Netflix / Disney+) -> 统一默认跟随 🚀 节点选择
  if (enableMediaGroup !== false) {
    proxyGroups.push({
      name: '🎬 国际流媒体',
      type: 'select',
      proxies: [...scenarioProxies]
    });
  }

  // 7. 📲 Telegram 消息 -> 统一默认跟随 🚀 节点选择
  if (enableTelegramGroup !== false) {
    proxyGroups.push({
      name: '📲 Telegram',
      type: 'select',
      proxies: [...scenarioProxies]
    });
  }

  // 8. 🎮 游戏平台 -> 默认 DIRECT 或 🚀 节点选择
  if (enableGameGroup !== false) {
    proxyGroups.push({
      name: '🎮 游戏平台',
      type: 'select',
      proxies: [...directFirstProxies]
    });
  }

  // 9. 🍎 Apple / 微软服务 -> 默认 DIRECT 或 🚀 节点选择
  if (enableAppleGroup !== false) {
    proxyGroups.push({
      name: '🍎 Apple / 微软',
      type: 'select',
      proxies: [...directFirstProxies]
    });
  }

  // 10. 🐟 漏网之鱼 (兜底组) -> 默认根据 fallbackRule 设定
  if (enableFinalGroup !== false) {
    proxyGroups.push({
      name: '🐟 漏网之鱼',
      type: 'select',
      proxies: fallbackRule === 'DIRECT' ? [...directFirstProxies] : [...scenarioProxies]
    });
  }

  // 11. 独立上游订阅专属组 (每个上游源一个独立分组) - 严格仅包含该订阅自身节点
  activeSubGroupMap.forEach(sg => {
    proxyGroups.push({
      name: sg.name,
      type: 'select',
      hidden: true,
      proxies: [...sg.nodeNames, 'DIRECT']
    });
  });

  // 3. Build Rules List
  const rules = [];

  // High priority user rules (Direct)
  directIps.forEach(ip => {
    if (ip.trim()) rules.push(`IP-CIDR,${ip.trim()},DIRECT${noResolveSuffix}`);
  });
  if (allowProcessRules) {
    directProcesses.forEach(p => {
      if (p.trim()) rules.push(`PROCESS-NAME,${p.trim()},DIRECT`);
    });
  }
  directKeywords.forEach(kw => {
    if (kw.trim()) rules.push(`DOMAIN-KEYWORD,${kw.trim()},DIRECT`);
  });
  directDomains.forEach(d => {
    if (d.trim()) rules.push(`DOMAIN-SUFFIX,${d.trim()},DIRECT`);
  });

  // High priority user rules (Proxy)
  proxyIps.forEach(ip => {
    if (ip.trim()) rules.push(`IP-CIDR,${ip.trim()},${mainProxyGroup}${noResolveSuffix}`);
  });
  if (allowProcessRules) {
    proxyProcesses.forEach(p => {
      if (p.trim()) rules.push(`PROCESS-NAME,${p.trim()},${mainProxyGroup}`);
    });
  }
  proxyKeywords.forEach(kw => {
    if (kw.trim()) rules.push(`DOMAIN-KEYWORD,${kw.trim()},${mainProxyGroup}`);
  });
  proxyDomains.forEach(d => {
    if (d.trim()) rules.push(`DOMAIN-SUFFIX,${d.trim()},${mainProxyGroup}`);
  });

  // 🛑 广告拦截规则
  if (enableAdBlock !== false && PRESET_SCENARIOS.adblock) {
    PRESET_SCENARIOS.adblock.domains.forEach(d => rules.push(`DOMAIN-SUFFIX,${d},REJECT`));
    PRESET_SCENARIOS.adblock.keywords?.forEach(kw => rules.push(`DOMAIN-KEYWORD,${kw},REJECT`));
  }

  // 🤖 AI 专线规则
  if (enableAiGroup !== false && PRESET_SCENARIOS.ai) {
    PRESET_SCENARIOS.ai.domains.forEach(d => rules.push(`DOMAIN-SUFFIX,${d},🤖 AI 专线`));
    PRESET_SCENARIOS.ai.keywords?.forEach(kw => rules.push(`DOMAIN-KEYWORD,${kw},🤖 AI 专线`));
  }

  // 🎬 国际流媒体规则
  if (enableMediaGroup !== false && PRESET_SCENARIOS.media) {
    PRESET_SCENARIOS.media.domains.forEach(d => rules.push(`DOMAIN-SUFFIX,${d},🎬 国际流媒体`));
    PRESET_SCENARIOS.media.keywords?.forEach(kw => rules.push(`DOMAIN-KEYWORD,${kw},🎬 国际流媒体`));
  }

  // 📲 Telegram 消息规则
  if (enableTelegramGroup !== false && PRESET_SCENARIOS.telegram) {
    PRESET_SCENARIOS.telegram.ips.forEach(ip => rules.push(`IP-CIDR,${ip},📲 Telegram${noResolveSuffix}`));
    PRESET_SCENARIOS.telegram.domains.forEach(d => rules.push(`DOMAIN-SUFFIX,${d},📲 Telegram`));
  }

  // 🎮 游戏平台规则
  if (enableGameGroup !== false && PRESET_SCENARIOS.games) {
    PRESET_SCENARIOS.games.domains.forEach(d => rules.push(`DOMAIN-SUFFIX,${d},🎮 游戏平台`));
  }

  // 🍎 Apple / 微软服务规则
  if (enableAppleGroup !== false && PRESET_SCENARIOS.apple) {
    PRESET_SCENARIOS.apple.domains.forEach(d => rules.push(`DOMAIN-SUFFIX,${d},🍎 Apple / 微软`));
  }

  // 🛡️ Loyalsoldier 高速规则集
  if (enableLoyalsoldier !== false) {
    rules.push('RULE-SET,applications,DIRECT');
    rules.push('RULE-SET,reject,REJECT');
    rules.push(`RULE-SET,proxy,${mainProxyGroup}`);
    rules.push(`RULE-SET,gfw,${mainProxyGroup}`);
    rules.push(`RULE-SET,tld-not-cn,${mainProxyGroup}`);
    if (enableTelegramGroup !== false) {
      rules.push(`RULE-SET,telegramcidr,📲 Telegram${noResolveSuffix}`);
    }
    rules.push('RULE-SET,direct,DIRECT');
    rules.push(`RULE-SET,lancidr,DIRECT${noResolveSuffix}`);
    rules.push(`RULE-SET,cncidr,DIRECT${noResolveSuffix}`);
  }

  // Tail Rules
  rules.push('GEOSITE,private,DIRECT');
  if (enableGeoSiteCn) rules.push('GEOSITE,cn,DIRECT');
  rules.push(`GEOIP,LAN,DIRECT${noResolveSuffix}`);
  if (enableGeoIpCn) rules.push(`GEOIP,CN,DIRECT${noResolveSuffix}`);

  const finalGroup = enableFinalGroup !== false ? '🐟 漏网之鱼' : (fallbackRule === 'DIRECT' ? 'DIRECT' : mainProxyGroup);
  rules.push(`MATCH,${finalGroup}`);

  // 4. Construct Full Clash Config Object
  const clashConfig = {
    'mixed-port': 7890,
    'allow-lan': true,
    mode: 'rule',
    'log-level': 'info',
    ipv6: false,
    'tcp-concurrent': enableTcpConcurrent,
    'unified-delay': enableUnifiedDelay,
    'find-process-mode': enableProcessStrict ? 'strict' : 'always',
    dns: {
      enable: true,
      ipv6: false,
      'enhanced-mode': 'fake-ip',
      'fake-ip-range': '198.18.0.1/16',
      'default-nameserver': ['223.5.5.5', '119.29.29.29', '1.1.1.1'],
      'fake-ip-filter': fakeIpFilter,
      nameserver: nameservers,
      fallback: fallbackDns,
      'fallback-filter': {
        geoip: true,
        'geoip-code': 'CN',
        ipcidr: ['240.0.0.0/4']
      },
      'nameserver-policy': {
        'geosite:cn,private': nameservers
      }
    },
    proxies: allProxies,
    'proxy-groups': proxyGroups,
    rules
  };

  if (enableLoyalsoldier !== false) {
    clashConfig['rule-providers'] = {
      applications: {
        type: 'http',
        behavior: 'classical',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/applications.txt',
        path: './ruleset/loyalsoldier/applications.txt',
        interval: 86400
      },
      reject: {
        type: 'http',
        behavior: 'domain',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/reject.txt',
        path: './ruleset/loyalsoldier/reject.txt',
        interval: 86400
      },
      proxy: {
        type: 'http',
        behavior: 'domain',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/proxy.txt',
        path: './ruleset/loyalsoldier/proxy.txt',
        interval: 86400
      },
      gfw: {
        type: 'http',
        behavior: 'domain',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/gfw.txt',
        path: './ruleset/loyalsoldier/gfw.txt',
        interval: 86400
      },
      'tld-not-cn': {
        type: 'http',
        behavior: 'domain',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/tld-not-cn.txt',
        path: './ruleset/loyalsoldier/tld-not-cn.txt',
        interval: 86400
      },
      telegramcidr: {
        type: 'http',
        behavior: 'ipcidr',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/telegramcidr.txt',
        path: './ruleset/loyalsoldier/telegramcidr.txt',
        interval: 86400
      },
      direct: {
        type: 'http',
        behavior: 'domain',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/direct.txt',
        path: './ruleset/loyalsoldier/direct.txt',
        interval: 86400
      },
      cncidr: {
        type: 'http',
        behavior: 'ipcidr',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/cncidr.txt',
        path: './ruleset/loyalsoldier/cncidr.txt',
        interval: 86400
      },
      lancidr: {
        type: 'http',
        behavior: 'ipcidr',
        format: 'text',
        url: 'https://testingcf.jsdelivr.net/gh/Loyalsoldier/clash-rules@release/lancidr.txt',
        path: './ruleset/loyalsoldier/lancidr.txt',
        interval: 86400
      }
    };
  }

  if (enableSniffer) {
    clashConfig.sniffer = {
      enable: true,
      sniff: {
        TLS: { ports: [443, 8443] },
        HTTP: { ports: [80, '8080-8880'], 'override-destination': true },
        QUIC: { ports: [443, 8443] }
      },
      'skip-domain': ['Mijia Cloud', 'dlg.io.mi.com', '+.apple.com']
    };
  }

  // Generate Userinfo Header String
  let aggregatedUserinfo = '';
  if (aggTotal > 0 || aggUpload > 0 || aggDownload > 0) {
    aggregatedUserinfo = `upload=${aggUpload}; download=${aggDownload}; total=${aggTotal}`;
    if (minExpire) aggregatedUserinfo += `; expire=${minExpire}`;
  }

  const yamlString = YAML.stringify(clashConfig, { aliasDuplicateObjects: false });

  return {
    yaml: yamlString,
    userinfo: aggregatedUserinfo,
    totalNodes: activeProxies.length,
    activeSubsCount: activeSubGroupMap.length,
    proxies: activeProxies
  };
}

export async function fetchAllUserProxies(userConfig, options = {}) {
  const {
    subscriptions = [],
    enableAutoFlags = true,
    enableCleanAdAndRate = true,
    enableGeoIpLookup = true,
    enableDeadNodeFilter = false,
    enableLatencySort = false,
    latencyTimeoutMs = 2000,
    customRenameRules = []
  } = userConfig;
  const enabledSubs = subscriptions.filter(s => s.enabled !== false && s.url);
  const fetchedResults = await Promise.allSettled(
    enabledSubs.map(s => fetchSubscription(s.url, s.prefix || s.name || ''))
  );

  // Pre-resolve DNS for any domain hosts for GeoIP lookup
  await Promise.allSettled(
    fetchedResults.map(r => r.status === 'fulfilled' && r.value.nodes ? prewarmDnsForProxies(r.value.nodes) : null)
  );

  const allProxies = [];
  const nameCountMap = new Map();
  let aggUpload = 0, aggDownload = 0, aggTotal = 0, minExpire = null;

  fetchedResults.forEach((res, idx) => {
    if (res.status === 'fulfilled') {
      const data = res.value;
      const subInfo = enabledSubs[idx];
      data.nodes.forEach(p => {
        let name = formatNodeName(p, {
          enableAutoFlags,
          enableCleanAdAndRate,
          enableGeoIpLookup,
          customRenameRules,
          defaultRegion: subInfo.defaultRegion || ''
        });
        if (nameCountMap.has(name)) {
          const c = nameCountMap.get(name) + 1;
          nameCountMap.set(name, c);
          name = `${name} (${c})`;
        } else {
          nameCountMap.set(name, 0);
        }
        allProxies.push({ ...p, name });
      });

      if (data.userInfo) {
        aggUpload += data.userInfo.upload || 0;
        aggDownload += data.userInfo.download || 0;
        aggTotal += data.userInfo.total || 0;
        if (data.userInfo.expire) {
          const exp = Math.floor(data.userInfo.expire / 1000);
          if (!minExpire || exp < minExpire) minExpire = exp;
        }
      }
    }
  });

  // Apply Dead node filter & Latency sorting if requested or configured
  let activeProxies = allProxies;
  const shouldFilterDead = options.enableDeadNodeFilter !== undefined ? options.enableDeadNodeFilter : enableDeadNodeFilter;
  const shouldSortLatency = options.enableLatencySort !== undefined ? options.enableLatencySort : enableLatencySort;

  if (shouldFilterDead || shouldSortLatency || options.runLatencyProbe) {
    const probeRes = await batchProbeProxies(allProxies, {
      timeoutMs: Number(options.timeoutMs || latencyTimeoutMs) || 2000,
      forceRefresh: options.forceRefresh === true
    });
    activeProxies = applyLatencyFilterAndSort(probeRes.proxies, {
      enableDeadNodeFilter: shouldFilterDead,
      enableLatencySort: shouldSortLatency
    });
  }

  let userinfo = '';
  if (aggTotal > 0 || aggUpload > 0 || aggDownload > 0) {
    userinfo = `upload=${aggUpload}; download=${aggDownload}; total=${aggTotal}`;
    if (minExpire) userinfo += `; expire=${minExpire}`;
  }

  return {
    proxies: activeProxies,
    userinfo
  };
}
