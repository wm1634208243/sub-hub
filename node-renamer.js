/**
 * SubHub Node Renamer & Flag Injector
 * Sub-Store styled node processing pipeline:
 * 1. Cleans advertising, traffic info, and rate keywords (e.g. 1.5x, 剩余流量)
 * 2. Injects national/regional flag emoji automatically based on:
 *    - Node name keywords (HK, JP, US, Singapore, Tokyo, etc.)
 *    - Node server domain keywords (hk.node.com -> HK)
 *    - GeoIP offline lookup based on server IP / resolved domain IP (MaxMind GeoLite)
 *    - Subscription-level default region fallback (manually configured)
 * 3. Applies user-defined custom regex/string find-and-replace rules
 */

import geoip from 'geoip-lite';
import dns from 'dns';

export const REGION_FLAGS = [
  { code: 'HK', flag: '🇭🇰', name: '香港', regex: /(香港|hong\s*kong|hkg|🇭🇰|\b(hk|hkg)\b)/i },
  { code: 'TW', flag: '🇹🇼', name: '台湾', regex: /(台湾|taiwan|tpe|hsinchu|🇹🇼|\b(tw|twn)\b)/i },
  { code: 'JP', flag: '🇯🇵', name: '日本', regex: /(日本|japan|tokyo|osaka|nrt|hnd|kix|东京|大阪|🇯🇵|\b(jp|jpn)\b)/i },
  { code: 'SG', flag: '🇸🇬', name: '新加坡', regex: /(新加坡|singapore|sin|狮城|🇸🇬|\b(sg|sgp)\b)/i },
  { code: 'US', flag: '🇺🇸', name: '美国', regex: /(美国|united\s*states|usa|lax|sfo|nyc|sjc|iad|ord|sea|pdx|dfw|atl|ewr|洛杉矶|圣何塞|西雅图|纽约|芝加哥|硅谷|波特兰|达拉斯|亚特兰大|🇺🇸|\b(us|usa)\b)/i },
  { code: 'KR', flag: '🇰🇷', name: '韩国', regex: /(韩国|korea|sel|seoul|icn|首尔|仁川|🇰🇷|\b(kr|kor)\b)/i },
  { code: 'GB', flag: '🇬🇧', name: '英国', regex: /(英国|united\s*kingdom|great\s*britain|london|lhr|伦敦|英格兰|🇬🇧|\b(uk|gbr)\b|(?<![0-9.])\bgb\b(?![0-9]))/i },
  { code: 'DE', flag: '🇩🇪', name: '德国', regex: /(德国|germany|fra|frankfurt|法兰克福|柏林|🇩🇪|\b(de|deu)\b)/i },
  { code: 'FR', flag: '🇫🇷', name: '法国', regex: /(法国|france|paris|cdg|巴黎|🇫🇷|\b(fr|fra)\b)/i },
  { code: 'CA', flag: '🇨🇦', name: '加拿大', regex: /(加拿大|canada|toronto|vancouver|yyz|yvr|多伦多|温哥华|🇨🇦|\b(ca|can)\b)/i },
  { code: 'AU', flag: '🇦🇺', name: '澳大利亚', regex: /(澳大利亚|澳洲|australia|sydney|melbourne|syd|mel|悉尼|墨尔本|🇦🇺|\b(au|aus)\b)/i },
  { code: 'RU', flag: '🇷🇺', name: '俄罗斯', regex: /(俄罗斯|russia|moscow|svo|莫斯科|圣彼得堡|🇷🇺)/i },
  { code: 'IN', flag: '🇮🇳', name: '印度', regex: /(印度|india|delhi|mumbai|孟买|德里|🇮🇳|\b(ind)\b)/i },
  { code: 'TH', flag: '🇹🇭', name: '泰国', regex: /(泰国|thailand|bangkok|bkk|曼谷|🇹🇭|\b(th|tha)\b)/i },
  { code: 'VN', flag: '🇻🇳', name: '越南', regex: /(越南|vietnam|hanoi|sgn|河内|胡志明|🇻🇳|\b(vn|vnm)\b)/i },
  { code: 'MY', flag: '🇲🇾', name: '马来西亚', regex: /(马来西亚|malaysia|kuala|kul|吉隆坡|大马|🇲🇾|\b(mys)\b)/i },
  { code: 'PH', flag: '🇵🇭', name: '菲律宾', regex: /(菲律宾|philippines|manila|马尼拉|🇵🇭|\b(phl)\b)/i },
  { code: 'TR', flag: '🇹🇷', name: '土耳其', regex: /(土耳其|turkey|istanbul|ist|伊斯坦布尔|🇹🇷|\b(tur)\b)/i },
  { code: 'AR', flag: '🇦🇷', name: '阿根廷', regex: /(阿根廷|argentina|🇦🇷|\b(arg)\b)/i },
  { code: 'BR', flag: '🇧🇷', name: '巴西', regex: /(巴西|brazil|sao\s*paulo|sao|圣保罗|🇧🇷|\b(bra)\b)/i },
  { code: 'NL', flag: '🇳🇱', name: '荷兰', regex: /(荷兰|netherlands|amsterdam|ams|阿姆斯特丹|🇳🇱|\b(nld)\b)/i },
  { code: 'CH', flag: '🇨🇭', name: '瑞士', regex: /(瑞士|switzerland|zurich|苏黎世|🇨🇭|\b(che)\b)/i },
  { code: 'SE', flag: '🇸🇪', name: '瑞典', regex: /(瑞典|sweden|stockholm|斯德哥尔摩|🇸🇪|\b(swe)\b)/i },
  { code: 'IT', flag: '🇮🇹', name: '意大利', regex: /(意大利|italy|milan|rome|米兰|罗马|🇮🇹|\b(ita)\b)/i },
  { code: 'ES', flag: '🇪🇸', name: '西班牙', regex: /(西班牙|spain|madrid|barcelona|马德里|巴塞罗那|🇪🇸|\b(esp)\b)/i },
  { code: 'AE', flag: '🇦🇪', name: '阿联酋', regex: /(阿联酋|迪拜|uae|dubai|dxb|🇦🇪|\b(are)\b)/i }
];

const CODE_TO_FLAG = new Map();
REGION_FLAGS.forEach(r => CODE_TO_FLAG.set(r.code, r.flag));

const FLAG_EMOJI_REGEX = /^[\uD83C][\uDDE6-\uDDFF][\uD83C][\uDDE6-\uDDFF]/;
const ANY_FLAG_EMOJI_REGEX = /[\uD83C][\uDDE6-\uDDFF][\uD83C][\uDDE6-\uDDFF]/g;

// In-memory DNS cache to avoid resolving the same domain repeatedly
const dnsCache = new Map();

/**
 * Universal ISO-3166-1 alpha-2 -> Emoji Flag conversion
 * e.g. "US" -> "🇺🇸", "JP" -> "🇯🇵", "IS" -> "🇮🇸"
 */
export function isoCodeToFlagEmoji(countryCode) {
  if (!countryCode || countryCode.length !== 2) return null;
  const code = countryCode.toUpperCase();
  if (CODE_TO_FLAG.has(code)) return CODE_TO_FLAG.get(code);
  const codePoints = [...code].map(c => 127397 + c.charCodeAt(0));
  return String.fromCodePoint(...codePoints);
}

/**
 * Look up country code from IP string (synchronous offline MaxMind database)
 */
export function getCountryCodeFromIp(ip) {
  if (!ip || typeof ip !== 'string') return null;
  try {
    const cleanIp = ip.trim();
    const geo = geoip.lookup(cleanIp);
    if (geo && geo.country) {
      return geo.country.toUpperCase();
    }
  } catch {}
  return null;
}

/**
 * Clean & format a single proxy node name
 * Supports:
 * - string input: formatNodeName(name, options)
 * - proxy object input: formatNodeName(proxy, options)
 *
 * @param {string|Object} rawInput 
 * @param {Object} options 
 * @returns {string} Cleaned node name with flag emoji
 */
export function formatNodeName(rawInput, options = {}) {
  let name = '';
  let server = '';

  if (typeof rawInput === 'string') {
    name = rawInput.trim();
    server = options.server || '';
  } else if (rawInput && typeof rawInput === 'object') {
    name = (rawInput.name || 'Node').trim();
    server = rawInput.server || '';
  }

  if (!name) return name;

  const {
    enableAutoFlags = true,
    enableCleanAdAndRate = true,
    enableGeoIpLookup = true,
    customRenameRules = [],
    defaultRegion = '' // e.g. 'HK', 'JP', 'US'
  } = options;

  // 1. Clean advertising & rate keywords
  if (enableCleanAdAndRate !== false) {
    // Remove rate tags like (1.5x), [0.2倍率], | 1.0X |
    name = name.replace(/[(（\[【\s]*(?:[xX*×]\s*\d+(?:\.\d+)?|\d+(?:\.\d+)?\s*(?:倍率?|[xX*×]))[)）\]】\s]*/g, ' ');
    // Remove traffic announcements like (剩余流量: 200G), [到期: 2026-12-31]
    name = name.replace(/[(（\[【\s]*(?:剩余流量|已用流量|到期时间|剩余|到期)[\s:：]*[^\s,|)）\]】]+[)）\]】\s]*/gi, ' ');
    name = name.replace(/[-_–—\s]*\d+(?:\.\d+)?\s*(?:KB|MB|GB|TB|PB|K|M|G|T)\b(?:-∞|\s*[-_–—]\s*∞)?/gi, ' ');
    // Remove website links / ads like 官网: xxx.com, 发布页: xxx
    name = name.replace(/[(（\[【\s]*(?:官网地址|官方网站|最新地址|官网|发布页)[\s:：]*[a-zA-Z0-9_\-\.\:\/]+[)）\]】\s]*/gi, ' ');
    // Clean empty parentheses or brackets left behind
    name = name.replace(/[(（\[【]\s*[)）\]】]/g, ' ');
    // Clean excessive spaces and hyphens
    name = name.replace(/\s+/g, ' ').trim();
  }

  // 2. Apply Custom Regex/String Rename Rules
  if (Array.isArray(customRenameRules) && customRenameRules.length > 0) {
    for (const rule of customRenameRules) {
      if (!rule || rule.enabled === false || !rule.search) continue;
      try {
        if (rule.isRegex) {
          const re = new RegExp(rule.search, rule.flags || 'g');
          name = name.replace(re, rule.replace || '');
        } else {
          name = name.replaceAll(rule.search, rule.replace || '');
        }
      } catch (err) {
        // Silently skip broken custom regex to prevent aggregator crashes
      }
    }
    name = name.replace(/\s+/g, ' ').trim();
  }

  // 3. Auto Inject Region Flag Emoji
  if (enableAutoFlags !== false) {
    const hasLeadingFlag = FLAG_EMOJI_REGEX.test(name);
    if (!hasLeadingFlag) {
      let matchedFlag = null;

      // (A) Priority 1: Match keywords in Node Name
      for (const reg of REGION_FLAGS) {
        if (reg.regex.test(name)) {
          matchedFlag = reg.flag;
          break;
        }
      }

      // (B) Priority 2: Match keywords in Server Domain (e.g. hk.node.com)
      if (!matchedFlag && server) {
        for (const reg of REGION_FLAGS) {
          if (reg.regex.test(server)) {
            matchedFlag = reg.flag;
            break;
          }
        }
      }

      // (C) Priority 3: GeoIP Lookup from Server IP
      if (!matchedFlag && enableGeoIpLookup !== false && server) {
        // If server is an IP, do immediate synchronous GeoIP lookup
        if (/^(\d{1,3}\.){3}\d{1,3}$/.test(server) || server.includes(':')) {
          const countryCode = getCountryCodeFromIp(server);
          if (countryCode) {
            matchedFlag = isoCodeToFlagEmoji(countryCode);
          }
        } else if (dnsCache.has(server)) {
          // If server was already resolved and cached in dnsCache
          const resolvedIp = dnsCache.get(server);
          const countryCode = getCountryCodeFromIp(resolvedIp);
          if (countryCode) {
            matchedFlag = isoCodeToFlagEmoji(countryCode);
          }
        }
      }

      // (D) Priority 4: Manual Default Region configured for this subscription
      if (!matchedFlag && defaultRegion) {
        matchedFlag = isoCodeToFlagEmoji(defaultRegion);
      }

      // Prepend the identified flag
      if (matchedFlag) {
        name = name.replace(ANY_FLAG_EMOJI_REGEX, '').trim();
        name = `${matchedFlag} ${name}`;
      }
    }
  }

  return name || (typeof rawInput === 'string' ? rawInput : rawInput.name);
}

/**
 * Pre-resolve DNS for any domains in proxy list to enable instant GeoIP lookup
 */
export async function prewarmDnsForProxies(proxies = []) {
  if (!Array.isArray(proxies) || proxies.length === 0) return;
  const domainHosts = new Set();

  for (const p of proxies) {
    if (p && p.server && typeof p.server === 'string') {
      const s = p.server.trim();
      if (!/^(\d{1,3}\.){3}\d{1,3}$/.test(s) && !s.includes(':') && !dnsCache.has(s)) {
        domainHosts.add(s);
      }
    }
  }

  if (domainHosts.size === 0) return;

  await Promise.allSettled(
    Array.from(domainHosts).map(async (domain) => {
      try {
        const res = await dns.promises.lookup(domain);
        if (res && res.address) {
          dnsCache.set(domain, res.address);
        }
      } catch {}
    })
  );
}

/**
 * Batch format an array of proxy objects
/**
 * Identify country information (code, flag, name) for a proxy node
 */
export function identifyNodeCountry(rawInput, options = {}) {
  let name = '';
  let server = '';

  if (typeof rawInput === 'string') {
    name = rawInput.trim();
    server = options.server || '';
  } else if (rawInput && typeof rawInput === 'object') {
    name = (rawInput.name || 'Node').trim();
    server = rawInput.server || '';
  }

  const { enableGeoIpLookup = true, defaultRegion = '' } = options;

  // 1. Keyword in Name
  for (const reg of REGION_FLAGS) {
    if (reg.regex.test(name)) {
      return { code: reg.code, flag: reg.flag, name: reg.name, method: 'keyword' };
    }
  }

  // 2. Keyword in Server Domain
  if (server) {
    for (const reg of REGION_FLAGS) {
      if (reg.regex.test(server)) {
        return { code: reg.code, flag: reg.flag, name: reg.name, method: 'domain' };
      }
    }
  }

  // 3. GeoIP Lookup from Server IP
  if (enableGeoIpLookup !== false && server) {
    let ip = server;
    if (!/^(\d{1,3}\.){3}\d{1,3}$/.test(server) && !server.includes(':')) {
      ip = dnsCache.get(server);
    }
    if (ip) {
      const code = getCountryCodeFromIp(ip);
      if (code) {
        const flag = isoCodeToFlagEmoji(code);
        const matchReg = REGION_FLAGS.find(r => r.code === code);
        return { code, flag, name: matchReg ? matchReg.name : code, method: 'geoip' };
      }
    }
  }

  // 4. Default Region fallback
  if (defaultRegion) {
    const code = defaultRegion.toUpperCase();
    const flag = isoCodeToFlagEmoji(code);
    const matchReg = REGION_FLAGS.find(r => r.code === code);
    return { code, flag, name: matchReg ? matchReg.name : code, method: 'default' };
  }

  return null;
}

/**
 * Batch format an array of proxy objects
 */
export function formatProxiesList(proxies = [], options = {}) {
  if (!Array.isArray(proxies)) return [];
  return proxies.map(p => {
    if (!p || typeof p !== 'object') return p;
    const newName = formatNodeName(p, options);
    return { ...p, name: newName };
  });
}


/**
 * Detect single primary region for a proxy node
 * Returns { code, flag, name } or null
 */
export function detectNodePrimaryRegion(rawName, server = '', defaultRegion = '') {
  if (defaultRegion) {
    const reg = REGION_FLAGS.find(r => r.code.toUpperCase() === defaultRegion.toUpperCase());
    if (reg) return reg;
  }

  let cleanName = (rawName || '').trim();
  // Strip traffic suffix first to avoid false positives (e.g. 243.13GB matching GB)
  cleanName = cleanName.replace(/[-_–—\s]*\d+(?:\.\d+)?\s*(?:KB|MB|GB|TB|PB|K|M|G|T)\b(?:-∞|\s*[-_–—]\s*∞)?/gi, ' ');

  // 1. Match keywords in node name
  for (const reg of REGION_FLAGS) {
    if (reg.regex.test(cleanName)) {
      return reg;
    }
  }

  // 2. Match keywords in server domain
  if (server) {
    for (const reg of REGION_FLAGS) {
      if (reg.regex.test(server)) {
        return reg;
      }
    }

    // 3. GeoIP lookup
    const cleanServer = server.trim();
    if (/^(\d{1,3}\.){3}\d{1,3}$/.test(cleanServer) || cleanServer.includes(':')) {
      const code = getCountryCodeFromIp(cleanServer);
      if (code) {
        const reg = REGION_FLAGS.find(r => r.code === code);
        if (reg) return reg;
      }
    } else if (dnsCache.has(cleanServer)) {
      const resolvedIp = dnsCache.get(cleanServer);
      const code = getCountryCodeFromIp(resolvedIp);
      if (code) {
        const reg = REGION_FLAGS.find(r => r.code === code);
        if (reg) return reg;
      }
    }
  }

  return null;
}
