/**
 * format-converter.js
 * Multi-format converter for SubHub
 * Converts Clash proxy objects into Base64 URI lists, Sing-Box JSON, and Surge Proxy Lists.
 */

// ── 1. Clash Proxy -> Single Node URI ─────────────────────────────────────────

export function proxyToUri(proxy) {
  if (!proxy || !proxy.type) return null;
  const name = encodeURIComponent(proxy.name || 'Node');

  switch (proxy.type.toLowerCase()) {
    case 'vless': {
      const uuid = proxy.uuid || '';
      const server = proxy.server || '';
      const port = proxy.port || 443;
      const params = new URLSearchParams();

      params.set('encryption', proxy.cipher || 'none');

      const net = proxy.network || 'tcp';
      params.set('type', net);

      if (proxy['reality-opts']) {
        params.set('security', 'reality');
        params.set('pbk', proxy['reality-opts']['public-key'] || '');
        if (proxy['reality-opts']['short-id']) params.set('sid', proxy['reality-opts']['short-id']);
        if (proxy.servername) params.set('sni', proxy.servername);
        if (proxy['client-fingerprint']) params.set('fp', proxy['client-fingerprint']);
      } else if (proxy.tls) {
        params.set('security', 'tls');
        if (proxy.servername) params.set('sni', proxy.servername);
        if (proxy['client-fingerprint']) params.set('fp', proxy['client-fingerprint']);
        if (proxy['skip-cert-verify']) params.set('allowInsecure', '1');
      }

      if (net === 'ws' && proxy['ws-opts']) {
        if (proxy['ws-opts'].path) params.set('path', proxy['ws-opts'].path);
        if (proxy['ws-opts'].headers && proxy['ws-opts'].headers.Host) {
          params.set('host', proxy['ws-opts'].headers.Host);
        }
      } else if (net === 'grpc' && proxy['grpc-opts']) {
        if (proxy['grpc-opts']['grpc-service-name']) {
          params.set('serviceName', proxy['grpc-opts']['grpc-service-name']);
          params.set('mode', 'gun');
        }
      }

      return `vless://${uuid}@${server}:${port}?${params.toString()}#${name}`;
    }

    case 'vmess': {
      const vmessObj = {
        v: '2',
        ps: proxy.name || 'VMess',
        add: proxy.server,
        port: proxy.port,
        id: proxy.uuid,
        aid: proxy.alterId || 0,
        scy: proxy.cipher || 'auto',
        net: proxy.network || 'tcp',
        type: 'none',
        host: (proxy['ws-opts'] && proxy['ws-opts'].headers && proxy['ws-opts'].headers.Host) || proxy.servername || '',
        path: (proxy['ws-opts'] && proxy['ws-opts'].path) || '',
        tls: proxy.tls ? 'tls' : '',
        sni: proxy.servername || '',
        alpn: Array.isArray(proxy.alpn) ? proxy.alpn.join(',') : (proxy.alpn || '')
      };
      const b64 = Buffer.from(JSON.stringify(vmessObj)).toString('base64');
      return `vmess://${b64}`;
    }

    case 'trojan': {
      const password = encodeURIComponent(proxy.password || '');
      const server = proxy.server || '';
      const port = proxy.port || 443;
      const params = new URLSearchParams();

      if (proxy.servername) params.set('sni', proxy.servername);
      if (proxy.tls !== false) params.set('security', 'tls');
      if (proxy['skip-cert-verify']) params.set('allowInsecure', '1');

      const net = proxy.network || 'tcp';
      params.set('type', net);
      if (net === 'ws' && proxy['ws-opts']) {
        if (proxy['ws-opts'].path) params.set('path', proxy['ws-opts'].path);
        if (proxy['ws-opts'].headers && proxy['ws-opts'].headers.Host) {
          params.set('host', proxy['ws-opts'].headers.Host);
        }
      } else if (net === 'grpc' && proxy['grpc-opts'] && proxy['grpc-opts']['grpc-service-name']) {
        params.set('serviceName', proxy['grpc-opts']['grpc-service-name']);
      }

      return `trojan://${password}@${server}:${port}?${params.toString()}#${name}`;
    }

    case 'ss':
    case 'shadowsocks': {
      const cipher = proxy.cipher || 'aes-256-gcm';
      const password = proxy.password || '';
      const server = proxy.server || '';
      const port = proxy.port || 8388;
      const auth = Buffer.from(`${cipher}:${password}`).toString('base64');
      return `ss://${auth}@${server}:${port}#${name}`;
    }

    case 'hysteria2':
    case 'hy2': {
      const auth = encodeURIComponent(proxy.password || proxy.auth || '');
      const server = proxy.server || '';
      const port = proxy.port || 443;
      const params = new URLSearchParams();
      if (proxy.servername || proxy.sni) params.set('sni', proxy.servername || proxy.sni);
      if (proxy['skip-cert-verify'] || proxy.insecure) params.set('insecure', '1');
      return `hysteria2://${auth}@${server}:${port}?${params.toString()}#${name}`;
    }

    case 'tuic': {
      const uuid = proxy.uuid || '';
      const password = proxy.password || '';
      const server = proxy.server || '';
      const port = proxy.port || 443;
      const params = new URLSearchParams();
      if (proxy.servername || proxy.sni) params.set('sni', proxy.servername || proxy.sni);
      if (proxy['congestion-controller']) params.set('congestion_control', proxy['congestion-controller']);
      params.set('alpn', 'h3');
      return `tuic://${uuid}:${encodeURIComponent(password)}@${server}:${port}?${params.toString()}#${name}`;
    }

    default:
      return null;
  }
}

// ── 2. Convert Proxies Array to Base64 ─────────────────────────────────────────

export function convertToBase64(proxies) {
  if (!Array.isArray(proxies) || proxies.length === 0) return '';
  const links = proxies.map(proxyToUri).filter(Boolean);
  return Buffer.from(links.join('\n')).toString('base64');
}

// ── 3. Convert Proxies to Sing-Box JSON ────────────────────────────────────────

export function convertToSingBoxJson(proxies, userConfig = {}) {
  const outbounds = [];
  const nodeTags = [];

  for (const p of proxies) {
    const tag = p.name;
    let ob = null;

    switch ((p.type || '').toLowerCase()) {
      case 'vless': {
        ob = {
          type: 'vless',
          tag,
          server: p.server,
          server_port: p.port,
          uuid: p.uuid
        };
        if (p['reality-opts']) {
          ob.tls = {
            enabled: true,
            server_name: p.servername || '',
            reality: {
              enabled: true,
              public_key: p['reality-opts']['public-key'] || '',
              short_id: p['reality-opts']['short-id'] || ''
            },
            utls: {
              enabled: true,
              fingerprint: p['client-fingerprint'] || 'chrome'
            }
          };
        } else if (p.tls) {
          ob.tls = {
            enabled: true,
            server_name: p.servername || '',
            insecure: !!p['skip-cert-verify']
          };
        }
        if (p.network === 'ws' && p['ws-opts']) {
          ob.transport = {
            type: 'ws',
            path: p['ws-opts'].path || '/',
            headers: p['ws-opts'].headers || {}
          };
        } else if (p.network === 'grpc' && p['grpc-opts']) {
          ob.transport = {
            type: 'grpc',
            service_name: p['grpc-opts']['grpc-service-name'] || ''
          };
        }
        break;
      }

      case 'vmess': {
        ob = {
          type: 'vmess',
          tag,
          server: p.server,
          server_port: p.port,
          uuid: p.uuid,
          security: p.cipher || 'auto',
          alter_id: p.alterId || 0
        };
        if (p.tls) {
          ob.tls = {
            enabled: true,
            server_name: p.servername || '',
            insecure: !!p['skip-cert-verify']
          };
        }
        if (p.network === 'ws' && p['ws-opts']) {
          ob.transport = {
            type: 'ws',
            path: p['ws-opts'].path || '/',
            headers: p['ws-opts'].headers || {}
          };
        }
        break;
      }

      case 'trojan': {
        ob = {
          type: 'trojan',
          tag,
          server: p.server,
          server_port: p.port,
          password: p.password,
          tls: {
            enabled: true,
            server_name: p.servername || '',
            insecure: !!p['skip-cert-verify']
          }
        };
        if (p.network === 'ws' && p['ws-opts']) {
          ob.transport = {
            type: 'ws',
            path: p['ws-opts'].path || '/',
            headers: p['ws-opts'].headers || {}
          };
        }
        break;
      }

      case 'ss':
      case 'shadowsocks': {
        ob = {
          type: 'shadowsocks',
          tag,
          server: p.server,
          server_port: p.port,
          method: p.cipher || 'aes-256-gcm',
          password: p.password
        };
        break;
      }

      case 'hysteria2':
      case 'hy2': {
        ob = {
          type: 'hysteria2',
          tag,
          server: p.server,
          server_port: p.port,
          password: p.password || p.auth,
          tls: {
            enabled: true,
            server_name: p.servername || p.sni || '',
            insecure: !!p['skip-cert-verify']
          }
        };
        break;
      }

      case 'tuic': {
        ob = {
          type: 'tuic',
          tag,
          server: p.server,
          server_port: p.port,
          uuid: p.uuid,
          password: p.password,
          congestion_controller: p['congestion-controller'] || 'bbr',
          tls: {
            enabled: true,
            server_name: p.servername || p.sni || '',
            alpn: ['h3']
          }
        };
        break;
      }
    }

    if (ob) {
      outbounds.push(ob);
      nodeTags.push(tag);
    }
  }

  // 构造标准 selector 策略组
  const allOutbounds = [];

  if (nodeTags.length > 0) {
    // 1. 🚀 节点选择 (Selector)
    allOutbounds.push({
      type: 'selector',
      tag: '🚀 节点选择',
      outbounds: ['⚡ 自动优选 (全部源)', 'DIRECT', ...nodeTags],
      default: '⚡ 自动优选 (全部源)'
    });

    // 2. ⚡ 自动优选 (全部源) (URLTest)
    allOutbounds.push({
      type: 'urltest',
      tag: '⚡ 自动优选 (全部源)',
      outbounds: [...nodeTags],
      url: 'https://www.gstatic.com/generate_204',
      interval: '300s',
      tolerance: 50
    });

    // 3. 🤖 AI 专线
    if (userConfig.enableAiGroup !== false) {
      allOutbounds.push({
        type: 'selector',
        tag: '🤖 AI 专线',
        outbounds: ['🚀 节点选择', '⚡ 自动优选 (全部源)', ...nodeTags, 'DIRECT'],
        default: '🚀 节点选择'
      });
    }

    // 4. 🎬 国际流媒体
    if (userConfig.enableMediaGroup !== false) {
      allOutbounds.push({
        type: 'selector',
        tag: '🎬 国际流媒体',
        outbounds: ['⚡ 自动优选 (全部源)', '🚀 节点选择', ...nodeTags, 'DIRECT'],
        default: '⚡ 自动优选 (全部源)'
      });
    }

    // 5. 📲 Telegram 消息
    if (userConfig.enableTelegramGroup !== false) {
      allOutbounds.push({
        type: 'selector',
        tag: '📲 Telegram',
        outbounds: ['🚀 节点选择', '⚡ 自动优选 (全部源)', ...nodeTags, 'DIRECT'],
        default: '🚀 节点选择'
      });
    }

    // 6. 🎮 游戏平台
    if (userConfig.enableGameGroup !== false) {
      allOutbounds.push({
        type: 'selector',
        tag: '🎮 游戏平台',
        outbounds: ['DIRECT', '🚀 节点选择', '⚡ 自动优选', ...nodeTags],
        default: 'DIRECT'
      });
    }

    // 7. 🍎 Apple / 微软
    if (userConfig.enableAppleGroup !== false) {
      allOutbounds.push({
        type: 'selector',
        tag: '🍎 Apple / 微软',
        outbounds: ['DIRECT', '🚀 节点选择', '⚡ 自动优选', ...nodeTags],
        default: 'DIRECT'
      });
    }

    // 8. 🐟 漏网之鱼
    if (userConfig.enableFinalGroup !== false) {
      allOutbounds.push({
        type: 'selector',
        tag: '🐟 漏网之鱼',
        outbounds: ['🚀 节点选择', 'DIRECT', '⚡ 自动优选', ...nodeTags],
        default: userConfig.fallbackRule === 'DIRECT' ? 'DIRECT' : '🚀 节点选择'
      });
    }
  }

  // 节点实体
  allOutbounds.push(...outbounds);

  // 基础兜底直连与阻断
  allOutbounds.push(
    { type: 'direct', tag: 'DIRECT' },
    { type: 'block', tag: 'REJECT' },
    { type: 'dns', tag: 'dns-out' }
  );

  // 构造路由规则
  const routeRules = [
    { protocol: 'dns', outbound: 'dns-out' }
  ];

  if (userConfig.enableAdBlock !== false) {
    routeRules.push({ geosite: 'category-ads-all', outbound: 'REJECT' });
  }

  if (userConfig.enableAiGroup !== false) {
    routeRules.push({ geosite: 'openai', outbound: '🤖 AI 专线' });
    routeRules.push({ domain_suffix: ['openai.com', 'chatgpt.com', 'anthropic.com', 'claude.ai', 'deepseek.com'], outbound: '🤖 AI 专线' });
  }

  if (userConfig.enableMediaGroup !== false) {
    routeRules.push({ geosite: ['youtube', 'netflix', 'disney', 'spotify', 'tiktok'], outbound: '🎬 国际流媒体' });
  }

  if (userConfig.enableTelegramGroup !== false) {
    routeRules.push({ geosite: 'telegram', outbound: '📲 Telegram' });
    routeRules.push({ geoip: 'telegram', outbound: '📲 Telegram' });
  }

  routeRules.push(
    { geosite: 'cn', outbound: 'DIRECT' },
    { geoip: 'cn', outbound: 'DIRECT' },
    { geoip: 'private', outbound: 'DIRECT' }
  );

  const finalOutbound = userConfig.enableFinalGroup !== false ? '🐟 漏网之鱼' : (userConfig.fallbackRule === 'DIRECT' ? 'DIRECT' : '🚀 节点选择');

  // 构造 Sing-Box 完整 JSON
  return {
    version: 1,
    log: {
      level: 'info',
      timestamp: true
    },
    dns: {
      servers: [
        { tag: 'dns_direct', address: userConfig.nameservers?.[0] || '223.5.5.5', detour: 'DIRECT' },
        { tag: 'dns_proxy', address: userConfig.fallbackDns?.[0] || 'https://1.1.1.1/dns-query', detour: '🚀 节点选择' }
      ],
      rules: [
        { outbound: 'any', server: 'dns_direct' },
        { geosite: 'cn', server: 'dns_direct' },
        { site: ['category-games@cn'], server: 'dns_direct' }
      ]
    },
    inbounds: [
      {
        type: 'mixed',
        tag: 'mixed-in',
        listen: '127.0.0.1',
        listen_port: 2080,
        sniff: true,
        sniff_override_destination: true
      }
    ],
    outbounds: allOutbounds,
    route: {
      rules: routeRules,
      auto_detect_interface: true,
      final: finalOutbound
    }
  };
}

// ── 4. Convert Proxies to Surge Proxy List ─────────────────────────────────────

export function convertToSurgeList(proxies) {
  if (!Array.isArray(proxies) || proxies.length === 0) return '';
  const lines = ['# Surge Proxy List Generated by SubHub'];

  for (const p of proxies) {
    const name = (p.name || 'Node').replace(/,/g, ' ');
    switch ((p.type || '').toLowerCase()) {
      case 'trojan': {
        const parts = [
          `${name} = trojan`,
          p.server,
          p.port,
          `password=${p.password}`,
          `sni=${p.servername || p.server}`,
          p['skip-cert-verify'] ? 'skip-cert-verify=true' : 'skip-cert-verify=false'
        ];
        if (p.network === 'ws' && p['ws-opts']) {
          parts.push('ws=true');
          if (p['ws-opts'].path) parts.push(`ws-path=${p['ws-opts'].path}`);
          if (p['ws-opts'].headers?.Host) parts.push(`ws-headers=Host:${p['ws-opts'].headers.Host}`);
        }
        lines.push(parts.join(', '));
        break;
      }

      case 'vmess': {
        const parts = [
          `${name} = vmess`,
          p.server,
          p.port,
          `username=${p.uuid}`,
          p.tls ? 'tls=true' : 'tls=false',
          `sni=${p.servername || p.server}`
        ];
        if (p.network === 'ws' && p['ws-opts']) {
          parts.push('ws=true');
          if (p['ws-opts'].path) parts.push(`ws-path=${p['ws-opts'].path}`);
          if (p['ws-opts'].headers?.Host) parts.push(`ws-headers=Host:${p['ws-opts'].headers.Host}`);
        }
        lines.push(parts.join(', '));
        break;
      }

      case 'ss':
      case 'shadowsocks': {
        lines.push(`${name} = custom, ${p.server}, ${p.port}, ${p.cipher}, ${p.password}`);
        break;
      }

      case 'hysteria2':
      case 'hy2': {
        lines.push(`${name} = hysteria2, ${p.server}, ${p.port}, password=${p.password || p.auth}, sni=${p.servername || p.server}, skip-cert-verify=${!!p['skip-cert-verify']}`);
        break;
      }
    }
  }

  return lines.join('\n');
}

// ── 5. Detect Client Target Format by User-Agent ──────────────────────────────

export function detectClientTarget(ua = '', targetParam = '') {
  if (targetParam) {
    const t = targetParam.toLowerCase().trim();
    if (t.includes('clash') || t.includes('meta') || t.includes('mihomo')) return 'clash';
    if (t.includes('base64') || t.includes('nodes') || t.includes('v2ray') || t.includes('rocket')) return 'base64';
    if (t.includes('singbox') || t.includes('sing-box') || t.includes('sb') || t.includes('json')) return 'singbox';
    if (t.includes('surge')) return 'surge';
  }

  const u = (ua || '').toLowerCase();
  if (u.includes('sing-box') || u.includes('singbox') || u.includes('nekobox') || u.includes('karing')) {
    return 'singbox';
  }
  if (u.includes('shadowrocket') || u.includes('quantumult') || u.includes('loon') || u.includes('v2ray') || u.includes('matsuri')) {
    return 'base64';
  }
  if (u.includes('surge')) {
    return 'surge';
  }
  // Default to Clash YAML
  return 'clash';
}
