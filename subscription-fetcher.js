/**
 * Subscription Fetcher & Userinfo Parser
 * Handles upstream 3X-UI and Airport subscriptions:
 * - Extracts `Subscription-Userinfo` HTTP header (upload, download, total, expire)
 * - Decodes Base64 / Clash YAML formats
 * - In-memory caching with TTL
 */

import YAML from 'yaml';
import { parseNodeLink } from './protocol-parser.js';

// In-memory cache for fetched subscriptions: subUrl -> { nodes, userInfo, timestamp }
const subCache = new Map();
const CACHE_TTL_MS = 10 * 60 * 1000; // 10 minutes default cache

/**
 * Parse `Subscription-Userinfo` header into a structured object:
 * e.g. "upload=2147483648; download=10737418240; total=107374182400; expire=1798732800"
 */
export function parseSubscriptionUserInfo(headerStr) {
  if (!headerStr || typeof headerStr !== 'string') return null;

  const result = {
    upload: 0,
    download: 0,
    total: 0,
    expire: null,
    used: 0,
    remaining: 0,
    percentUsed: 0
  };

  const parts = headerStr.split(';');
  for (const part of parts) {
    const [key, val] = part.split('=').map(s => s.trim().toLowerCase());
    if (!key || !val) continue;

    const num = parseInt(val, 10);
    if (isNaN(num)) continue;

    if (key === 'upload') result.upload = num;
    else if (key === 'download') result.download = num;
    else if (key === 'total') result.total = num;
    else if (key === 'expire') {
      // timestamp in seconds
      result.expire = num * 1000;
    }
  }

  result.used = result.upload + result.download;
  if (result.total > 0) {
    result.remaining = Math.max(0, result.total - result.used);
    result.percentUsed = Math.min(100, Math.round((result.used / result.total) * 100));
  }

  return result;
}

/**
 * Fetch and parse an upstream subscription URL
 */
export async function fetchSubscription(subUrl, prefix = '', forceRefresh = false) {
  if (!subUrl || typeof subUrl !== 'string') {
    throw new Error('无效的订阅链接');
  }

  subUrl = subUrl.trim();

  // Check in-memory cache if not forced
  if (!forceRefresh && subCache.has(subUrl)) {
    const cached = subCache.get(subUrl);
    if (Date.now() - cached.timestamp < CACHE_TTL_MS) {
      return cached;
    }
  }

  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 12000); // 12s timeout

  try {
    const response = await fetch(subUrl, {
      signal: controller.signal,
      headers: {
        'User-Agent': 'ClashMeta/v1.18.0 (Clash.Meta; Mihomo; RuleHub)',
        'Accept': '*/*'
      }
    });

    clearTimeout(timeoutId);

    if (!response.ok) {
      throw new Error(`上游订阅响应错误 (HTTP ${response.status})`);
    }

    // 1. Extract Subscription-Userinfo Header
    const userinfoHeader = response.headers.get('subscription-userinfo') || 
                           response.headers.get('Subscription-Userinfo') || '';
    const userInfo = parseSubscriptionUserInfo(userinfoHeader);

    const bodyText = await response.text();
    const nodes = parseSubscriptionContent(bodyText, prefix);
    const sourceType = detectSourceType(subUrl, response.headers, bodyText, nodes);

    const result = {
      url: subUrl,
      prefix,
      nodes,
      nodesCount: nodes.length,
      userInfo,
      rawUserinfoHeader: userinfoHeader,
      sourceType,
      timestamp: Date.now(),
      updatedAt: new Date().toISOString()
    };

    subCache.set(subUrl, result);
    return result;
  } catch (err) {
    clearTimeout(timeoutId);
    if (err.name === 'AbortError') {
      throw new Error('连接上游订阅超时 (12秒)');
    }
    throw err;
  }
}

/**
 * Automatically identifies the source platform/type of the subscription URL
 */
export function detectSourceType(subUrl = '', headers = null, bodyText = '', nodes = []) {
  const url = (subUrl || '').toLowerCase();

  if (url.includes(':2096') || url.includes(':2053') || url.includes(':54321') || url.includes('/sub/') || url.includes('/clash/') || url.includes('/xui') || url.includes('3x-ui')) {
    return '3X-UI / X-UI';
  }
  if (url.includes('v2board') || url.includes('/api/v1/client/subscribe') || url.includes('sspanel') || url.includes('mod_sub')) {
    return '商业机场 (V2board/SSPanel)';
  }
  if (url.includes('github.com') || url.includes('raw.githubusercontent.com') || url.includes('gitlab.com')) {
    return 'GitHub 托管源';
  }
  if (url.includes('sub?target=') || url.includes('subconverter')) {
    return 'Subconverter 转换';
  }

  const trimmed = (bodyText || '').trim();
  if (trimmed.startsWith('proxies:') || trimmed.includes('proxy-groups:') || trimmed.includes('port:')) {
    return 'Clash YAML';
  }

  if (nodes.length > 0) {
    const firstType = (nodes[0].type || '').toUpperCase();
    if (firstType) return `${firstType} 节点池`;
  }

  return '标准代理订阅';
}

/**
 * Parses body content which might be YAML or Base64 / plain text protocol links
 */
export function parseSubscriptionContent(content, prefix = '') {
  if (!content || typeof content !== 'string') return [];
  const trimmed = content.trim();

  // 1. Try parsing as Clash YAML
  if (trimmed.includes('proxies:') || trimmed.startsWith('port:') || trimmed.startsWith('mixed-port:')) {
    try {
      const parsedYaml = YAML.parse(trimmed);
      if (parsedYaml && Array.isArray(parsedYaml.proxies)) {
        return parsedYaml.proxies.map(p => {
          if (!p || typeof p !== 'object') return null;
          const nodeName = p.name ? (prefix ? `[${prefix}] ${p.name}` : p.name) : 'Node';
          return { ...p, name: nodeName };
        }).filter(Boolean);
      }
    } catch {}
  }

  // 2. Try Base64 decoding
  let decoded = '';
  try {
    const cleanB64 = trimmed.replace(/\s+/g, '');
    const buff = Buffer.from(cleanB64, 'base64');
    decoded = buff.toString('utf-8');
  } catch {}

  // If base64 decoding gave valid protocol strings, use it; otherwise fallback to raw text
  const targetText = (decoded && (decoded.includes('://') || decoded.includes('vmess://'))) ? decoded : trimmed;
  const lines = targetText.split(/[\r\n]+/).map(s => s.trim()).filter(Boolean);

  const nodes = [];
  for (const line of lines) {
    const node = parseNodeLink(line, prefix);
    if (node) nodes.push(node);
  }

  return nodes;
}

/**
 * Clear cache for a specific URL or all
 */
export function clearSubCache(subUrl) {
  if (subUrl) subCache.delete(subUrl);
  else subCache.clear();
}
