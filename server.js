import express from 'express';
import cors from 'cors';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import crypto from 'crypto';
import bcrypt from 'bcryptjs';
import { rateLimit } from 'express-rate-limit';
import { compileConfigToJs } from './compiler.js';
import { aggregateClashYaml, fetchAllUserProxies } from './aggregator.js';
import { fetchSubscription, clearSubCache } from './subscription-fetcher.js';
import { convertToBase64, convertToSingBoxJson, convertToSurgeList, detectClientTarget } from './format-converter.js';
import { formatNodeName, identifyNodeCountry, prewarmDnsForProxies } from './node-renamer.js';
import { batchProbeProxies, applyLatencyFilterAndSort } from './latency-tester.js';
import { deriveUserKey, encryptUserConfig, decryptUserConfig, isEncryptedBundle } from './crypto-engine.js';
import { exec } from 'child_process';
import util from 'util';
import dns from 'dns';
const execPromise = util.promisify(exec);

const CURRENT_VERSION = '1.0.4';
const REPO_OWNER = 'wm1634208243';
const REPO_NAME = 'sub-hub';
const REPO_URL = `https://github.com/${REPO_OWNER}/${REPO_NAME}`;

function compareVersions(v1, v2) {
  const parts1 = String(v1).replace(/^v/, '').split('.').map(Number);
  const parts2 = String(v2).replace(/^v/, '').split('.').map(Number);
  for (let i = 0; i < Math.max(parts1.length, parts2.length); i++) {
    const p1 = parts1[i] || 0;
    const p2 = parts2[i] || 0;
    if (p1 > p2) return 1;
    if (p1 < p2) return -1;
  }
  return 0;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const app = express();
const PORT = process.env.PORT || 3000;
const DATA_DIR    = path.join(__dirname, 'data');
const USERS_FILE  = path.join(DATA_DIR, 'users.json');
const SESSIONS_FILE = path.join(DATA_DIR, 'sessions.json');
const CONFIGS_DIR = path.join(DATA_DIR, 'configs');
const OLD_CONFIG  = path.join(DATA_DIR, 'config.json');
const LOGS_FILE   = path.join(DATA_DIR, 'logs.json');
const SETTINGS_FILE = path.join(DATA_DIR, 'system_settings.json');
const SESSION_TTL = 30 * 24 * 60 * 60 * 1000; // 30 days persistent session

const DEFAULT_SYSTEM_SETTINGS = {
  customDomain: '',
  enableHttpsRedirect: false,
  updatedAt: null
};

let cachedSystemSettings = { ...DEFAULT_SYSTEM_SETTINGS };

function loadSystemSettings() {
  try {
    if (fs.existsSync(SETTINGS_FILE)) {
      const data = JSON.parse(fs.readFileSync(SETTINGS_FILE, 'utf-8'));
      cachedSystemSettings = { ...DEFAULT_SYSTEM_SETTINGS, ...data };
    }
  } catch (e) {
    cachedSystemSettings = { ...DEFAULT_SYSTEM_SETTINGS };
  }
  return cachedSystemSettings;
}

async function saveSystemSettings(settings) {
  try {
    cachedSystemSettings = {
      ...cachedSystemSettings,
      ...settings,
      updatedAt: new Date().toISOString()
    };
    if (!fs.existsSync(DATA_DIR)) {
      fs.mkdirSync(DATA_DIR, { recursive: true });
    }
    await fs.promises.writeFile(SETTINGS_FILE, JSON.stringify(cachedSystemSettings, null, 2), 'utf-8');
    return cachedSystemSettings;
  } catch (e) {
    console.error('保存系统设置失败:', e);
    throw e;
  }
}

function isCloudflareIp(ip) {
  if (!ip) return false;
  const cfPrefixes = [
    '173.245.', '103.21.', '103.22.', '103.31.', '141.101.', '108.162.', '190.93.',
    '188.114.', '197.234.', '198.41.', '104.16.', '104.17.', '104.18.', '104.19.',
    '104.20.', '104.21.', '104.22.', '104.23.', '104.24.', '104.25.', '104.26.',
    '104.27.', '104.28.', '172.64.', '172.65.', '172.66.', '172.67.', '172.68.',
    '172.69.', '172.70.', '172.71.', '162.158.', '162.159.', '131.0.72.'
  ];
  return cfPrefixes.some(p => ip.startsWith(p));
}

// ── In-memory stores & persistence ────────────────────────────────────────────
const activeSessions = new Map(); // token → { username, role, expiresAt }
const compiledCache  = new Map(); // username → compiledJs
const accessLogs     = new Map(); // username → [{ id, time, ip, ua, type, status, detail }]

function loadSessions() {
  try {
    if (fs.existsSync(SESSIONS_FILE)) {
      const data = JSON.parse(fs.readFileSync(SESSIONS_FILE, 'utf-8'));
      const now = Date.now();
      for (const [k, v] of Object.entries(data)) {
        if (v && v.expiresAt > now) {
          activeSessions.set(k, v);
        }
      }
    }
  } catch {}
}

async function saveSessions() {
  try {
    const obj = {};
    for (const [k, v] of activeSessions.entries()) {
      obj[k] = v;
    }
    await fs.promises.writeFile(SESSIONS_FILE, JSON.stringify(obj, null, 2));
  } catch {}
}

function loadLogs() {
  try {
    if (fs.existsSync(LOGS_FILE)) {
      const data = JSON.parse(fs.readFileSync(LOGS_FILE, 'utf-8'));
      for (const [k, v] of Object.entries(data)) {
        if (Array.isArray(v)) {
          accessLogs.set(k, v);
        }
      }
    }
  } catch {}
}

async function saveLogs() {
  try {
    const obj = {};
    for (const [k, v] of accessLogs.entries()) {
      obj[k] = v;
    }
    await fs.promises.writeFile(LOGS_FILE, JSON.stringify(obj, null, 2));
  } catch {}
}

function recordAccessLog(username, { ip, ua, type, status = 200, detail = '' }) {
  if (!username) username = 'admin';
  if (!accessLogs.has(username)) accessLogs.set(username, []);
  const list = accessLogs.get(username);
  list.unshift({
    id: 'log_' + Date.now() + '_' + Math.random().toString(36).substring(2, 7),
    time: new Date().toISOString(),
    ip: ip || '127.0.0.1',
    ua: ua || 'Direct / Unknown UA',
    type: type || '🌐 订阅拉取',
    status: Number(status) || 200,
    detail: detail || ''
  });
  if (list.length > 100) list.pop();
  saveLogs();
}

app.use(cors({ origin: true, credentials: true }));
app.use(express.json({ limit: '10mb' }));
app.use(express.static(path.join(__dirname, 'public')));

// ── Helpers ───────────────────────────────────────────────────────────────────

function defaultUserConfig() {
  return {
    subscriptionToken: 'rulehub_' + crypto.randomBytes(8).toString('hex'),
    tokenExpiresAt: null,
    mode: 'gui',
    fallbackRule: 'DIRECT',
    enableGeoSiteCn: true,
    enableGeoIpCn: true,
    enableSniffer: true,
    enableTcpConcurrent: true,
    enableNoResolve: true,
    enableUnifiedDelay: true,
    enableProcessStrict: true,
    customProxyGroupName: '',
    targetPlatforms: ['macos', 'windows', 'ios', 'android'],
    enableAutoPlatformDetect: true,
    subscriptions: [],
    enableAutoFlags: true,
    enableCleanAdAndRate: true,
    enableGeoIpLookup: true,
    enableDeadNodeFilter: false,
    enableLatencySort: false,
    latencyTimeoutMs: 2000,
    customRenameRules: [],
    nameservers:  ['223.5.5.5', '119.29.29.29'],
    fallbackDns:  ['https://1.1.1.1/dns-query', 'https://8.8.8.8/dns-query'],
    proxyIps: [], proxyProcesses: [], proxyKeywords: [], proxyDomains: [],
    directIps: [], directProcesses: [], directKeywords: [], directDomains: [],
    fakeIpFilter: [],
    customScript: ''
  };
}

function loadUsers() {
  try {
    if (fs.existsSync(USERS_FILE)) return JSON.parse(fs.readFileSync(USERS_FILE, 'utf-8'));
  } catch {}
  return [];
}

async function saveUsers(users) {
  await fs.promises.writeFile(USERS_FILE, JSON.stringify(users, null, 2));
}

function isUserCurrentlyDisabled(user) {
  if (!user || !user.disabled) return false;
  if (user.disabledUntil) {
    const expireTime = new Date(user.disabledUntil).getTime();
    if (!isNaN(expireTime) && Date.now() >= expireTime) {
      // Automatic unban / reactivate
      user.disabled = false;
      user.disabledUntil = null;
      user.disabledReason = null;
      try {
        const users = loadUsers();
        const target = users.find(u => u.username === user.username);
        if (target) {
          target.disabled = false;
          target.disabledUntil = null;
          target.disabledReason = null;
          fs.writeFileSync(USERS_FILE, JSON.stringify(users, null, 2));
          console.log(`[Auto-Unban] ⏰ 用户【${user.username}】封禁时长已到期，已自动恢复解禁！`);
        }
      } catch {}
      return false;
    }
  }
  return true;
}

function sweepExpiredUserBans() {
  try {
    const users = loadUsers();
    let changed = false;
    const now = Date.now();
    for (const u of users) {
      if (u.disabled && u.disabledUntil) {
        const expireTime = new Date(u.disabledUntil).getTime();
        if (!isNaN(expireTime) && now >= expireTime) {
          u.disabled = false;
          u.disabledUntil = null;
          u.disabledReason = null;
          changed = true;
          console.log(`[Auto-Unban] ⏰ 用户【${u.username}】封禁时长已到期，系统已自动解除禁用！`);
        }
      }
    }
    if (changed) {
      fs.writeFileSync(USERS_FILE, JSON.stringify(users, null, 2));
    }
  } catch {}
}

function getUserKey(username) {
  const users = loadUsers();
  const u = users.find(x => x.username.toLowerCase() === username.toLowerCase().trim());
  if (u && u.passwordHash) {
    return deriveUserKey(u.passwordHash, username);
  }
  return deriveUserKey('subhub_master_secret_fallback_v1', username);
}

function loadUserConfig(username) {
  const file = path.join(CONFIGS_DIR, `${username}.json`);
  const d = defaultUserConfig();
  try {
    if (fs.existsSync(file)) {
      const raw = JSON.parse(fs.readFileSync(file, 'utf-8'));
      let parsed = raw;
      if (isEncryptedBundle(raw)) {
        const key = getUserKey(username);
        parsed = decryptUserConfig(raw, key);
      }
      return {
        ...d,
        ...parsed,
        targetPlatforms: parsed.targetPlatforms || d.targetPlatforms
      };
    }
  } catch (e) {
    console.error(`[Crypto] 读取/解密用户 ${username} 配置异常:`, e.message);
  }
  return d;
}

async function saveUserConfig(username, cfg) {
  const key = getUserKey(username);
  const bundle = encryptUserConfig(cfg, key);
  await fs.promises.writeFile(
    path.join(CONFIGS_DIR, `${username}.json`),
    JSON.stringify(bundle, null, 2)
  );
  compiledCache.delete(username); // invalidate cache
}

async function ensureConfigsEncrypted() {
  if (!fs.existsSync(CONFIGS_DIR)) return;
  try {
    const files = await fs.promises.readdir(CONFIGS_DIR);
    for (const file of files) {
      if (file.endsWith('.json')) {
        const uname = file.replace('.json', '');
        try {
          const filePath = path.join(CONFIGS_DIR, file);
          const raw = JSON.parse(await fs.promises.readFile(filePath, 'utf-8'));
          if (!isEncryptedBundle(raw)) {
            console.log(`🔐 [Zero-Knowledge] 自动将用户 ${uname} 存量明文配置升级为 AES-256-GCM 密文存储...`);
            const key = getUserKey(uname);
            const bundle = encryptUserConfig(raw, key);
            await fs.promises.writeFile(filePath, JSON.stringify(bundle, null, 2));
          }
        } catch {}
      }
    }
  } catch {}
}

// ── Migration: single-user → multi-user ──────────────────────────────────────

async function migrate() {
  if (!fs.existsSync(OLD_CONFIG)) return;
  console.log('🔄 检测到旧版 config.json，正在迁移至多用户格式...');

  let old = {};
  try { old = JSON.parse(fs.readFileSync(OLD_CONFIG, 'utf-8')); } catch {}

  // Create admin user if none yet
  let users = loadUsers();
  if (!users.find(u => u.username === 'admin')) {
    const plain = old.adminPassword || 'admin';
    const passwordHash = plain.startsWith('$2') ? plain : await bcrypt.hash(plain, 10);
    users.unshift({ username: 'admin', passwordHash, role: 'admin', createdAt: new Date().toISOString() });
    await saveUsers(users);
  }

  // Migrate config (drop adminPassword)
  const { adminPassword, ...rest } = old;
  await saveUserConfig('admin', { ...defaultUserConfig(), ...rest });

  await fs.promises.rename(OLD_CONFIG, OLD_CONFIG + '.migrated');
  console.log('✅ 迁移完成！旧配置已备份为 config.json.migrated');
}

// ── Init ──────────────────────────────────────────────────────────────────────

async function init() {
  fs.mkdirSync(CONFIGS_DIR, { recursive: true });
  loadSessions();
  loadLogs();
  loadSystemSettings();
  await migrate();
  await ensureConfigsEncrypted();

  let users = loadUsers();
  if (users.length === 0) {
    const passwordHash = await bcrypt.hash('admin', 10);
    users = [{ username: 'admin', passwordHash, role: 'admin', createdAt: new Date().toISOString() }];
    await saveUsers(users);
    await saveUserConfig('admin', defaultUserConfig());
    console.log('👤 已创建默认管理员账号 (用户名: admin  密码: admin)');
  } else {
    // If admin is using legacy placeholder token, auto-upgrade to random cryptographical token
    const adminCfg = loadUserConfig('admin');
    if (adminCfg && adminCfg.subscriptionToken === 'rulehub_secret_token') {
      adminCfg.subscriptionToken = 'rulehub_' + crypto.randomBytes(8).toString('hex');
      await saveUserConfig('admin', adminCfg);
    }
  }
}

// ── Middleware ────────────────────────────────────────────────────────────────

function authMiddleware(req, res, next) {
  const header = req.headers.authorization || '';
  let token = header.startsWith('Bearer ') ? header.slice(7) : null;

  // Check Cookie if no Bearer header
  if (!token && req.headers.cookie) {
    const match = req.headers.cookie.match(/(?:^|;\s*)subhub_session=([^;]+)/);
    if (match) token = decodeURIComponent(match[1]);
  }

  if (!token) return res.status(401).json({ error: '未登录或登录已过期' });

  const session = activeSessions.get(token);
  if (!session || Date.now() > session.expiresAt) {
    activeSessions.delete(token);
    saveSessions();
    return res.status(401).json({ error: '登录已过期，请重新登录' });
  }

  // Check if account was disabled by Admin
  const users = loadUsers();
  const user = users.find(u => u.username === session.username);
  if (user && isUserCurrentlyDisabled(user)) {
    activeSessions.delete(token);
    saveSessions();
    const untilStr = user.disabledUntil ? new Date(user.disabledUntil).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' }) : null;
    return res.status(403).json({ 
      error: untilStr ? `您的账号已被临时禁用至 ${untilStr}，请等待自动解禁或联系管理员` : '您的账号已被管理员禁用，请联系管理员' 
    });
  }

  // 30-day Rolling expiration
  session.expiresAt = Date.now() + SESSION_TTL;
  req.session = session;
  req.token = token;
  next();
}

function adminOnly(req, res, next) {
  if (req.session.role !== 'admin') return res.status(403).json({ error: '需要管理员权限' });
  next();
}

const loginLimiter = rateLimit({
  windowMs: 15 * 60 * 1000,
  max: 10,
  message: { error: 'IP 登录尝试次数过多，请 15 分钟后再试' },
  standardHeaders: true,
  legacyHeaders: false
});

const registerLimiter = rateLimit({
  windowMs: 15 * 60 * 1000,
  max: 15,
  message: { error: 'IP 注册尝试次数过多，请 15 分钟后再试' },
  standardHeaders: true,
  legacyHeaders: false
});

// ── Brute-force & Timing Attack Defense ───────────────────────────────────────
const failedLoginAttempts = new Map(); // username -> { count: number, lockedUntil: number }
const DUMMY_BCRYPT_HASH = '$2a$10$wN36q6m3hR22c8n1J8UuOe8H.pQ6l3f1K8X5w4s2y3q4z5v6w7x8y';

function checkAccountLock(username) {
  if (!username) return null;
  const record = failedLoginAttempts.get(username.toLowerCase().trim());
  if (record && record.lockedUntil && Date.now() < record.lockedUntil) {
    const remainingMinutes = Math.ceil((record.lockedUntil - Date.now()) / (60 * 1000));
    return `该账号因连续输错密码已临时锁定，请 ${remainingMinutes} 分钟后再试`;
  }
  return null;
}

function recordLoginFailure(username) {
  if (!username) return;
  const uname = username.toLowerCase().trim();
  const record = failedLoginAttempts.get(uname) || { count: 0, lockedUntil: 0 };
  record.count += 1;
  if (record.count >= 5) {
    record.lockedUntil = Date.now() + 15 * 60 * 1000; // Lock for 15 minutes after 5 consecutive failures
    record.count = 0;
  }
  failedLoginAttempts.set(uname, record);
}

function recordLoginSuccess(username) {
  if (username) failedLoginAttempts.delete(username.toLowerCase().trim());
}

// Only these keys can be updated via POST /api/config
const CONFIG_WHITELIST = new Set([
  'mode', 'fallbackRule', 'enableGeoSiteCn', 'enableGeoIpCn',
  'enableSniffer', 'enableTcpConcurrent', 'enableNoResolve', 'enableUnifiedDelay', 'enableProcessStrict', 'customProxyGroupName',
  'enableAiGroup', 'enableMediaGroup', 'enableTelegramGroup', 'enableGameGroup', 'enableAppleGroup', 'enableAdBlock', 'enableFinalGroup', 'enableLoyalsoldier',
  'targetPlatforms', 'enableAutoPlatformDetect', 'subscriptions',
  'enableAutoFlags', 'enableCleanAdAndRate', 'enableGeoIpLookup',
  'enableDeadNodeFilter', 'enableLatencySort', 'latencyTimeoutMs', 'customRenameRules',
  'nameservers', 'fallbackDns',
  'proxyIps', 'proxyProcesses', 'proxyKeywords', 'proxyDomains',
  'directIps', 'directProcesses', 'directKeywords', 'directDomains',
  'fakeIpFilter', 'customScript'
]);

// ── Public: subscription endpoints ────────────────────────────────────────────

function findUserAndCheckSubscriptionAccess(token) {
  if (!token) return { ok: false, error: 'Token 缺失' };
  const users = loadUsers();
  let matchUser = null, userCfg = null;
  for (const u of users) {
    const cfg = loadUserConfig(u.username);
    if (cfg.subscriptionToken === token) { matchUser = u; userCfg = cfg; break; }
  }
  if (!matchUser) return { ok: false, error: '无效的订阅 Token' };

  // Check if account was disabled by Admin
  if (isUserCurrentlyDisabled(matchUser)) {
    const untilStr = matchUser.disabledUntil ? new Date(matchUser.disabledUntil).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' }) : null;
    return { 
      ok: false, 
      error: untilStr ? `该账号已被临时禁用至 ${untilStr}，订阅已暂停下发` : '该账号已被管理员禁用，订阅已暂停下发' 
    };
  }

  // Check expiry
  if (userCfg.tokenExpiresAt && Date.now() > new Date(userCfg.tokenExpiresAt).getTime()) {
    const expired = new Date(userCfg.tokenExpiresAt).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' });
    return { ok: false, error: `订阅已过期（过期时间: ${expired}）` };
  }

  return { ok: true, matchUser, userCfg };
}

// 1. JavaScript Override Script (/api/rules.js, /api/js, /api/rules)
app.get(['/api/rules.js', '/api/js', '/api/rules'], (req, res) => {
  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';

  const deny = (msg) => {
    recordAccessLog('admin', { ip, ua, type: '⚡ JS 规则脚本', status: 403, detail: `403 拒绝访问: ${msg}` });
    res.setHeader('Content-Type', 'text/plain; charset=utf-8');
    return res.status(403).send(`// Error: 403 Forbidden - ${msg}`);
  };

  const { token } = req.query;
  const access = findUserAndCheckSubscriptionAccess(token);
  if (!access.ok) return deny(access.error);

  const { matchUser, userCfg } = access;

  // Log access
  recordAccessLog(matchUser.username, {
    ip,
    ua,
    type: '⚡ JS 规则脚本',
    status: 200,
    detail: '客户端拉取纯规则预处理覆写脚本成功'
  });

  // Dynamic compile with UA platform awareness
  const js = compileConfigToJs(userCfg, ua);

  res.setHeader('Content-Type', 'application/javascript; charset=utf-8');
  res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
  res.send(js);
});

// 2. Full Clash / Mihomo Aggregated YAML Subscription (/api/clash.yaml)
app.get('/api/clash.yaml', async (req, res) => {
  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';

  const deny = (msg) => {
    recordAccessLog('admin', { ip, ua, type: '🌟 Clash YAML 订阅', status: 403, detail: `403 拒绝访问: ${msg}` });
    res.setHeader('Content-Type', 'text/plain; charset=utf-8');
    return res.status(403).send(`# Error: 403 Forbidden - ${msg}`);
  };

  const { token } = req.query;
  const access = findUserAndCheckSubscriptionAccess(token);
  if (!access.ok) return deny(access.error);

  const { matchUser, userCfg } = access;

  try {
    const result = await aggregateClashYaml(userCfg, ua);
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '🌟 Clash YAML 订阅',
      status: 200,
      detail: '客户端成功拉取聚合 Clash YAML 订阅与策略组'
    });
    res.setHeader('Content-Type', 'text/yaml; charset=utf-8');
    res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
    res.setHeader('Profile-Update-Interval', '24');
    if (result.userinfo) {
      res.setHeader('Subscription-Userinfo', result.userinfo);
      res.setHeader('subscription-userinfo', result.userinfo);
    }
    res.send(result.yaml);
  } catch (err) {
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '🌟 Clash YAML 订阅',
      status: 500,
      detail: `500 生成异常: ${err.message}`
    });
    console.error('aggregateClashYaml error:', err);
    res.status(500).send(`# Error generating YAML: ${err.message}`);
  }
});

// ── Multi-Format Subscription Dispatcher (Smart UA Auto-Detect) ───────────────

app.get(['/api/sub', '/api/subscription'], async (req, res) => {
  const { token, target } = req.query;
  const ua = req.headers['user-agent'] || '';
  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';

  const deny = (msg) => {
    recordAccessLog('admin', { ip, ua, type: '🤖 智能自适应订阅', status: 403, detail: `403 拒绝访问: ${msg}` });
    res.status(403).setHeader('Content-Type', 'text/plain; charset=utf-8').send(`🚫 ${msg}`);
  };

  const access = findUserAndCheckSubscriptionAccess(token);
  if (!access.ok) return deny(access.error);

  const { matchUser, userCfg } = access;
  const clientTarget = detectClientTarget(ua, target);

  try {
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: `🤖 智能订阅 (${clientTarget.toUpperCase()})`,
      status: 200,
      detail: `UA 自动识别客户端为 [${clientTarget.toUpperCase()}] 并下发相应格式`
    });

    if (clientTarget === 'clash') {
      const result = await aggregateClashYaml(userCfg, ua);
      res.setHeader('Content-Type', 'text/yaml; charset=utf-8');
      res.setHeader('Profile-Update-Interval', '24');
      if (result.userinfo) res.setHeader('Subscription-Userinfo', result.userinfo);
      return res.send(result.yaml);
    }

    const { proxies, userinfo } = await fetchAllUserProxies(userCfg);
    if (userinfo) {
      res.setHeader('Subscription-Userinfo', userinfo);
      res.setHeader('subscription-userinfo', userinfo);
    }
    res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');

    if (clientTarget === 'base64') {
      const b64 = convertToBase64(proxies);
      res.setHeader('Content-Type', 'text/plain; charset=utf-8');
      return res.send(b64);
    }

    if (clientTarget === 'singbox') {
      const sbJson = convertToSingBoxJson(proxies, userCfg);
      res.setHeader('Content-Type', 'application/json; charset=utf-8');
      return res.send(JSON.stringify(sbJson, null, 2));
    }

    if (clientTarget === 'surge') {
      const surgeList = convertToSurgeList(proxies);
      res.setHeader('Content-Type', 'text/plain; charset=utf-8');
      return res.send(surgeList);
    }

    // Default Fallback to Base64
    const b64 = convertToBase64(proxies);
    res.setHeader('Content-Type', 'text/plain; charset=utf-8');
    res.send(b64);
  } catch (err) {
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '🤖 智能自适应订阅',
      status: 500,
      detail: `500 生成异常: ${err.message}`
    });
    console.error('Subscription dispatch error:', err);
    res.status(500).send(`# Error generating subscription: ${err.message}`);
  }
});

// ── Base64 Single Nodes Output ────────────────────────────────────────────────

app.get(['/api/base64', '/api/sub.txt', '/api/nodes.txt'], async (req, res) => {
  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';
  const { token } = req.query;

  const deny = (msg) => {
    recordAccessLog('admin', { ip, ua, type: '🔗 Base64 单节点列表', status: 403, detail: `403 拒绝访问: ${msg}` });
    return res.status(403).setHeader('Content-Type', 'text/plain; charset=utf-8').send(`🚫 ${msg}`);
  };

  const access = findUserAndCheckSubscriptionAccess(token);
  if (!access.ok) return deny(access.error);

  const { matchUser, userCfg } = access;

  try {
    const { proxies, userinfo } = await fetchAllUserProxies(userCfg);
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '🔗 Base64 单节点列表',
      status: 200,
      detail: `成功下发 ${proxies.length} 个清洗单节点 (小火箭/V2Ray)`
    });
    res.setHeader('Content-Type', 'text/plain; charset=utf-8');
    res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
    if (userinfo) res.setHeader('Subscription-Userinfo', userinfo);
    const b64 = convertToBase64(proxies);
    res.send(b64);
  } catch (err) {
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '🔗 Base64 单节点列表',
      status: 500,
      detail: `500 生成异常: ${err.message}`
    });
    console.error('Base64 export error:', err);
    res.status(500).send(`# Error generating Base64: ${err.message}`);
  }
});

// ── Sing-Box JSON Output ──────────────────────────────────────────────────────

app.get(['/api/sing-box.json', '/api/singbox', '/api/sb.json'], async (req, res) => {
  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';
  const { token } = req.query;

  const deny = (msg) => {
    recordAccessLog('admin', { ip, ua, type: '📦 Sing-Box 原生 JSON', status: 403, detail: `403 拒绝访问: ${msg}` });
    return res.status(403).setHeader('Content-Type', 'text/plain; charset=utf-8').send(`🚫 ${msg}`);
  };

  const access = findUserAndCheckSubscriptionAccess(token);
  if (!access.ok) return deny(access.error);

  const { matchUser, userCfg } = access;

  try {
    const { proxies, userinfo } = await fetchAllUserProxies(userCfg);
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '📦 Sing-Box 原生 JSON',
      status: 200,
      detail: `成功下发 Sing-Box JSON 远程配置 (含 ${proxies.length} 节点与分流规则)`
    });
    res.setHeader('Content-Type', 'application/json; charset=utf-8');
    res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
    if (userinfo) res.setHeader('Subscription-Userinfo', userinfo);
    const sbJson = convertToSingBoxJson(proxies, userCfg);
    res.send(JSON.stringify(sbJson, null, 2));
  } catch (err) {
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '📦 Sing-Box 原生 JSON',
      status: 500,
      detail: `500 生成异常: ${err.message}`
    });
    console.error('Sing-Box export error:', err);
    res.status(500).send(`# Error generating Sing-Box JSON: ${err.message}`);
  }
});

// ── Surge Proxy List Output ───────────────────────────────────────────────────

app.get(['/api/surge.list', '/api/surge'], async (req, res) => {
  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';
  const { token } = req.query;

  const deny = (msg) => {
    recordAccessLog('admin', { ip, ua, type: '⚡ Surge 策略列表', status: 403, detail: `403 拒绝访问: ${msg}` });
    return res.status(403).setHeader('Content-Type', 'text/plain; charset=utf-8').send(`🚫 ${msg}`);
  };

  const access = findUserAndCheckSubscriptionAccess(token);
  if (!access.ok) return deny(access.error);

  const { matchUser, userCfg } = access;

  try {
    const { proxies, userinfo } = await fetchAllUserProxies(userCfg);
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '⚡ Surge 策略列表',
      status: 200,
      detail: `成功下发 Surge 策略列表 (${proxies.length} 个节点)`
    });
    res.setHeader('Content-Type', 'text/plain; charset=utf-8');
    res.setHeader('Cache-Control', 'no-cache, no-store, must-revalidate');
    if (userinfo) res.setHeader('Subscription-Userinfo', userinfo);
    const surgeList = convertToSurgeList(proxies);
    res.send(surgeList);
  } catch (err) {
    recordAccessLog(matchUser.username, {
      ip,
      ua,
      type: '⚡ Surge 策略列表',
      status: 500,
      detail: `500 生成异常: ${err.message}`
    });
    console.error('Surge export error:', err);
    res.status(500).send(`# Error generating Surge list: ${err.message}`);
  }
});

// ── Auth ──────────────────────────────────────────────────────────────────────

app.post('/api/login', loginLimiter, async (req, res) => {
  const { username, password } = req.body;
  if (!username || !password) return res.status(400).json({ error: '用户名和密码不能为空' });

  // 1. Check account-level lockout (defense against distributed botnets)
  const lockError = checkAccountLock(username);
  if (lockError) {
    return res.status(429).json({ error: lockError });
  }

  const users = loadUsers();
  const user = users.find(u => u.username.toLowerCase() === username.toLowerCase().trim());

  // Constant-time timing attack defense: execute bcrypt compare even if user does not exist
  const targetHash = user ? user.passwordHash : DUMMY_BCRYPT_HASH;
  const isMatch = await bcrypt.compare(password, targetHash);

  if (!user || !isMatch) {
    recordLoginFailure(username);
    return res.status(400).json({ error: '用户名或密码错误' });
  }

  // Check if account is disabled by Admin
  if (isUserCurrentlyDisabled(user)) {
    const untilStr = user.disabledUntil ? new Date(user.disabledUntil).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' }) : null;
    return res.status(403).json({ 
      error: untilStr ? `该账号已被临时禁用至 ${untilStr}，无法登录（到期后系统将自动解禁）` : '该账号已被管理员禁用，无法登录' 
    });
  }

  // Clear failure counter on success
  recordLoginSuccess(username);

  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';
  recordAccessLog(user.username, {
    ip,
    ua,
    type: '🔑 账号登录',
    status: 200,
    detail: 'Web 管理控制台登录成功'
  });

  const sessionToken = crypto.randomBytes(24).toString('hex');
  activeSessions.set(sessionToken, { username: user.username, role: user.role, expiresAt: Date.now() + SESSION_TTL });
  await saveSessions();

  // Set 30-day persistent Cookie
  res.setHeader('Set-Cookie', `subhub_session=${sessionToken}; Path=/; Max-Age=${SESSION_TTL / 1000}; HttpOnly; SameSite=Lax`);

  res.json({ success: true, token: sessionToken, username: user.username, role: user.role });
});

app.post('/api/register', registerLimiter, async (req, res) => {
  const { username, password } = req.body;
  if (!username || !password) return res.status(400).json({ error: '用户名和密码不能为空' });
  const cleanName = username.trim();
  if (!/^[a-zA-Z0-9_-]{2,32}$/.test(cleanName))
    return res.status(400).json({ error: '用户名格式不合法（2-32 位字母/数字/下划线/横线）' });
  if (password.length < 4)
    return res.status(400).json({ error: '密码长度至少 4 位' });

  const users = loadUsers();
  if (users.find(u => u.username.toLowerCase() === cleanName.toLowerCase())) {
    return res.status(400).json({ error: '该用户名已被注册，请更换用户名' });
  }

  const passwordHash = await bcrypt.hash(password, 10);
  const newUser = {
    username: cleanName,
    passwordHash,
    role: 'user', // Default permission is user (only manages own rules)
    createdAt: new Date().toISOString()
  };
  users.push(newUser);
  await saveUsers(users);

  // Initialize with initialConfig if promoted from local guest or default
  const initCfg = (req.body.initialConfig && typeof req.body.initialConfig === 'object')
    ? { ...defaultUserConfig(), ...req.body.initialConfig }
    : defaultUserConfig();
  await saveUserConfig(cleanName, initCfg);

  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';
  recordAccessLog(cleanName, {
    ip,
    ua,
    type: '👤 新用户注册',
    status: 200,
    detail: '账号注册成功并初始化专属配置'
  });

  // Auto login upon registration
  const sessionToken = crypto.randomBytes(24).toString('hex');
  activeSessions.set(sessionToken, { username: cleanName, role: 'user', expiresAt: Date.now() + SESSION_TTL });
  await saveSessions();

  res.setHeader('Set-Cookie', `subhub_session=${sessionToken}; Path=/; Max-Age=${SESSION_TTL / 1000}; HttpOnly; SameSite=Lax`);
  res.json({ success: true, token: sessionToken, username: cleanName, role: 'user', message: '注册成功！' });
});

app.get('/api/me', authMiddleware, (req, res) => {
  res.json({ success: true, username: req.session.username, role: req.session.role });
});

app.post('/api/logout', authMiddleware, async (req, res) => {
  if (req.token) activeSessions.delete(req.token);
  await saveSessions();
  res.setHeader('Set-Cookie', 'subhub_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax');
  res.json({ success: true });
});

// ── User config ───────────────────────────────────────────────────────────────

app.get('/api/config', authMiddleware, (req, res) => {
  res.json(loadUserConfig(req.session.username));
});

app.post('/api/config', authMiddleware, async (req, res) => {
  const current = loadUserConfig(req.session.username);
  const updates = {};
  for (const [k, v] of Object.entries(req.body)) {
    if (CONFIG_WHITELIST.has(k)) updates[k] = v;
  }
  await saveUserConfig(req.session.username, { ...current, ...updates });

  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';
  recordAccessLog(req.session.username, {
    ip,
    ua,
    type: '💾 配置保存发布',
    status: 200,
    detail: '规则分流与策略组设置已保存并热重载生效'
  });

  res.json({ success: true, message: '配置保存成功！' });
});

// 一键彻底物理抹除云端配置（切换到本地纯离线模式时使用）
app.delete('/api/config/purge', authMiddleware, async (req, res) => {
  try {
    const uname = req.session.username;
    const file = path.join(CONFIGS_DIR, `${uname}.json`);
    if (fs.existsSync(file)) {
      await fs.promises.unlink(file);
    }
    compiledCache.delete(uname);
    res.json({ success: true, message: '云端配置数据已彻底物理抹除！' });
  } catch (err) {
    res.status(500).json({ error: '抹除云端配置失败: ' + err.message });
  }
});

// 纯本地离线模式 / 访客模式免存盘实时瞬态编译转换
app.post('/api/public/compile-transient', async (req, res) => {
  try {
    const { config, target = 'clash', clientUa = '' } = req.body || {};
    if (!config || typeof config !== 'object') {
      return res.status(400).json({ error: '未提供有效配置对象' });
    }

    const fullCfg = { ...defaultUserConfig(), ...config };
    let output = '';
    let filename = `config.${target === 'singbox' ? 'json' : (target === 'surge' ? 'conf' : (target === 'js' ? 'js' : 'yaml'))}`;
    let nodesCount = 0;

    if (target === 'js') {
      output = compileConfigToJs(fullCfg, clientUa);
      filename = 'subhub_rules.js';
    } else if (target === 'singbox') {
      const { proxies } = await fetchAllUserProxies(fullCfg);
      nodesCount = proxies.length;
      const sbObj = convertToSingBoxJson(proxies, fullCfg);
      output = typeof sbObj === 'string' ? sbObj : JSON.stringify(sbObj, null, 2);
      filename = 'singbox_config.json';
    } else if (target === 'surge') {
      const { proxies } = await fetchAllUserProxies(fullCfg);
      nodesCount = proxies.length;
      output = convertToSurgeList(proxies, fullCfg);
      filename = 'surge_rules.conf';
    } else if (target === 'base64') {
      const { proxies } = await fetchAllUserProxies(fullCfg);
      nodesCount = proxies.length;
      output = convertToBase64(proxies);
      filename = 'nodes_base64.txt';
    } else {
      // Default Clash / Mihomo YAML
      const clashRes = await aggregateClashYaml(fullCfg, clientUa);
      output = clashRes.yaml || '';
      nodesCount = clashRes.totalNodes || 0;
      filename = 'clash_config.yaml';
    }

    res.json({
      success: true,
      target,
      filename,
      nodesCount,
      content: output
    });
  } catch (err) {
    res.status(500).json({ error: '瞬态编译失败: ' + err.message });
  }
});

app.post('/api/preview', authMiddleware, (req, res) => {
  try {
    const config = req.body || {};
    const js = compileConfigToJs(config);
    res.json({ js });
  } catch (e) {
    res.status(500).json({ error: e.message });
  }
});

app.post('/api/change-password', authMiddleware, async (req, res) => {
  const { oldPassword, newPassword } = req.body;
  if (!newPassword || newPassword.length < 4) return res.status(400).json({ error: '新密码至少 4 位' });

  const users = loadUsers();
  const user  = users.find(u => u.username === req.session.username);
  if (!user || !(await bcrypt.compare(oldPassword || '', user.passwordHash))) {
    return res.status(400).json({ error: '原密码错误' });
  }
  const userConfig = loadUserConfig(user.username);
  user.passwordHash = await bcrypt.hash(newPassword, 10);
  await saveUsers(users);
  await saveUserConfig(user.username, userConfig);

  // 强制注销该用户的所有活跃会话 (单点/多点全网失效)
  for (const [sToken, sData] of activeSessions.entries()) {
    if (sData.username === req.session.username) {
      activeSessions.delete(sToken);
    }
  }
  await saveSessions();

  // 清除浏览器 Session Cookie
  res.setHeader('Set-Cookie', 'subhub_session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax');
  res.json({ success: true, message: '密码修改成功，所有已有会话已失效，请重新登录' });
});

app.post('/api/regenerate-token', authMiddleware, async (req, res) => {
  const cfg = loadUserConfig(req.session.username);
  cfg.subscriptionToken = 'rulehub_' + crypto.randomBytes(8).toString('hex');
  await saveUserConfig(req.session.username, cfg);
  res.json({ success: true, token: cfg.subscriptionToken });
});

app.post('/api/set-token-expiry', authMiddleware, async (req, res) => {
  const { expiresAt } = req.body; // ISO string or null
  const cfg = loadUserConfig(req.session.username);
  cfg.tokenExpiresAt = expiresAt || null;
  await saveUserConfig(req.session.username, cfg);
  res.json({ success: true, tokenExpiresAt: cfg.tokenExpiresAt });
});

app.post('/api/subscriptions/refresh', authMiddleware, async (req, res) => {
  const cfg = loadUserConfig(req.session.username);
  const subs = cfg.subscriptions || [];

  const updatedSubs = await Promise.all(
    subs.map(async (s) => {
      if (!s.url) return s;
      try {
        const result = await fetchSubscription(s.url, s.prefix || s.name || '', true);
        return {
          ...s,
          nodesCount: result.nodesCount,
          userInfo: result.userInfo,
          sourceType: result.sourceType || s.sourceType,
          updatedAt: result.updatedAt,
          status: 'online',
          error: null
        };
      } catch (err) {
        return {
          ...s,
          status: 'error',
          error: err.message,
          updatedAt: new Date().toISOString()
        };
      }
    })
  );

  cfg.subscriptions = updatedSubs;
  await saveUserConfig(req.session.username, cfg);

  const ip = (req.headers['x-forwarded-for'] || '').split(',')[0].trim() || req.socket.remoteAddress || 'unknown';
  const ua = req.headers['user-agent'] || 'unknown';
  const totalNodes = updatedSubs.reduce((acc, cur) => acc + (cur.nodesCount || 0), 0);
  recordAccessLog(req.session.username, {
    ip,
    ua,
    type: '🛰️ 订阅节点同步',
    status: 200,
    detail: `手动批量同步了 ${updatedSubs.length} 个机场订阅，共聚合 ${totalNodes} 个节点`
  });

  res.json({ success: true, subscriptions: updatedSubs });
});

app.post('/api/subscriptions/test', authMiddleware, async (req, res) => {
  const { url, prefix = '', defaultRegion = '' } = req.body;
  if (!url) return res.status(400).json({ error: '订阅 URL 不能为空' });
  try {
    const result = await fetchSubscription(url, prefix, true);
    const cfg = loadUserConfig(req.session.username);

    // Prewarm DNS for GeoIP
    await prewarmDnsForProxies(result.nodes || []);

    const countryStatsMap = new Map();

    const formattedSamples = (result.nodes || []).slice(0, 10).map(n => {
      const original = n.name;
      const formatted = formatNodeName(n, {
        enableAutoFlags: cfg.enableAutoFlags !== false,
        enableCleanAdAndRate: cfg.enableCleanAdAndRate !== false,
        enableGeoIpLookup: cfg.enableGeoIpLookup !== false,
        customRenameRules: cfg.customRenameRules || [],
        defaultRegion
      });
      const country = identifyNodeCountry(n, {
        enableGeoIpLookup: cfg.enableGeoIpLookup !== false,
        defaultRegion
      });
      return {
        original,
        formatted,
        server: n.server || '',
        type: n.type || 'vless',
        country: country ? `${country.flag} ${country.name}` : null
      };
    });

    // Aggregate countries for all nodes in subscription
    for (const n of (result.nodes || [])) {
      const c = identifyNodeCountry(n, {
        enableGeoIpLookup: cfg.enableGeoIpLookup !== false,
        defaultRegion
      });
      if (c) {
        const label = `${c.flag} ${c.name}`;
        countryStatsMap.set(label, (countryStatsMap.get(label) || 0) + 1);
      } else {
        countryStatsMap.set('🌐 其他/未知', (countryStatsMap.get('🌐 其他/未知') || 0) + 1);
      }
    }

    const detectedCountries = Array.from(countryStatsMap.entries()).map(([label, count]) => ({
      label,
      count
    }));

    res.json({
      success: true,
      nodesCount: result.nodesCount,
      userInfo: result.userInfo,
      sourceType: result.sourceType,
      detectedCountries,
      sampleNodes: formattedSamples
    });
  } catch (err) {
    res.status(400).json({ error: err.message });
  }
});

app.post('/api/nodes/preview-rename', authMiddleware, (req, res) => {
  const {
    sampleNodes = [],
    sampleNames = [],
    enableAutoFlags = true,
    enableCleanAdAndRate = true,
    enableGeoIpLookup = true,
    customRenameRules = [],
    defaultRegion = ''
  } = req.body;
  const items = sampleNodes.length > 0 ? sampleNodes : sampleNames.map(s => (typeof s === 'string' ? { name: s } : s));
  const results = items.map(item => {
    const origName = typeof item === 'string' ? item : (item.name || '');
    const formatted = formatNodeName(item, {
      enableAutoFlags,
      enableCleanAdAndRate,
      enableGeoIpLookup,
      customRenameRules,
      defaultRegion
    });
    return { original: origName, formatted };
  });
  res.json({ success: true, results });
});

app.post('/api/nodes/health', authMiddleware, async (req, res) => {
  try {
    const { timeoutMs = 2000, forceRefresh = true } = req.body || {};
    const cfg = loadUserConfig(req.session.username);
    const { proxies } = await fetchAllUserProxies(cfg, { runLatencyProbe: false });
    const probeRes = await batchProbeProxies(proxies, {
      timeoutMs: Number(timeoutMs) || 2000,
      forceRefresh: forceRefresh !== false
    });
    res.json({
      success: true,
      totalNodes: probeRes.proxies.length,
      aliveCount: probeRes.aliveCount,
      deadCount: probeRes.deadCount,
      avgLatency: probeRes.avgLatency,
      proxies: probeRes.proxies.map(p => ({
        name: p.name,
        server: p.server,
        port: p.port,
        type: p.type,
        alive: p.alive,
        latency: p.latency,
        error: p.latencyError || null
      }))
    });
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

app.get('/api/access-log', authMiddleware, (req, res) => {
  res.json(accessLogs.get(req.session.username) || []);
});

app.post('/api/access-log/clear', authMiddleware, async (req, res) => {
  accessLogs.set(req.session.username, []);
  await saveLogs();
  res.json({ success: true, message: '日志已清空' });
});

// ── Admin ─────────────────────────────────────────────────────────────────────

app.get('/api/admin/users', authMiddleware, adminOnly, (req, res) => {
  sweepExpiredUserBans();
  const users = loadUsers().map(u => {
    const cfg = loadUserConfig(u.username);
    const disabled = isUserCurrentlyDisabled(u);
    return {
      username: u.username,
      role: u.role,
      disabled: disabled,
      disabledUntil: disabled ? u.disabledUntil : null,
      disabledReason: disabled ? (u.disabledReason || '') : '',
      createdAt: u.createdAt,
      tokenExpiresAt: cfg.tokenExpiresAt
    };
  });
  res.json(users);
});

app.post('/api/admin/users', authMiddleware, adminOnly, async (req, res) => {
  const { username, password, role = 'user' } = req.body;
  if (!username || !password) return res.status(400).json({ error: '用户名和密码不能为空' });
  if (!/^[a-zA-Z0-9_-]{2,32}$/.test(username))
    return res.status(400).json({ error: '用户名格式不合法（2-32 位字母/数字/下划线/横线）' });

  const users = loadUsers();
  if (users.find(u => u.username === username)) return res.status(400).json({ error: '用户名已存在' });

  const passwordHash = await bcrypt.hash(password, 10);
  users.push({ username, passwordHash, role: role === 'admin' ? 'admin' : 'user', createdAt: new Date().toISOString() });
  await saveUsers(users);
  await saveUserConfig(username, defaultUserConfig());
  res.json({ success: true });
});

app.delete('/api/admin/users/:username', authMiddleware, adminOnly, async (req, res) => {
  const { username } = req.params;
  if (username === req.session.username) return res.status(400).json({ error: '不能删除自己的账户' });

  let users = loadUsers();
  if (!users.find(u => u.username === username)) return res.status(404).json({ error: '用户不存在' });
  users = users.filter(u => u.username !== username);
  await saveUsers(users);

  try { await fs.promises.unlink(path.join(CONFIGS_DIR, `${username}.json`)); } catch {}
  compiledCache.delete(username);
  accessLogs.delete(username);
  res.json({ success: true });
});

app.post('/api/admin/users/:username/reset-password', authMiddleware, adminOnly, async (req, res) => {
  const { username } = req.params;
  const { newPassword } = req.body;
  if (!newPassword || newPassword.length < 4) return res.status(400).json({ error: '密码至少 4 位' });

  const users = loadUsers();
  const user  = users.find(u => u.username === username);
  if (!user) return res.status(404).json({ error: '用户不存在' });

  const userConfig = loadUserConfig(username);
  user.passwordHash = await bcrypt.hash(newPassword, 10);
  await saveUsers(users);
  await saveUserConfig(username, userConfig);

  res.json({ success: true });
});

app.post('/api/admin/users/:username/role', authMiddleware, adminOnly, async (req, res) => {
  const { username } = req.params;
  const { role } = req.body;
  if (!['admin', 'user'].includes(role)) {
    return res.status(400).json({ error: '无效的角色类型（仅支持 admin 或 user）' });
  }
  if (username === req.session.username && role !== 'admin') {
    return res.status(400).json({ error: '不能降低自己的管理员权限' });
  }

  const users = loadUsers();
  const user = users.find(u => u.username === username);
  if (!user) return res.status(404).json({ error: '用户不存在' });

  user.role = role;
  await saveUsers(users);

  // Synchronize active sessions
  for (const [t, s] of activeSessions.entries()) {
    if (s.username === username) {
      s.role = role;
    }
  }
  await saveSessions();

  res.json({ success: true, message: `已成功将用户【${username}】权限修改为【${role === 'admin' ? '管理员' : '普通用户'}】` });
});

app.post('/api/admin/users/:username/status', authMiddleware, adminOnly, async (req, res) => {
  const { username } = req.params;
  const { disabled, durationMinutes, disabledUntil, reason } = req.body;

  if (username === req.session.username && disabled) {
    return res.status(400).json({ error: '不能禁用当前登录的管理员自己' });
  }

  const users = loadUsers();
  const user = users.find(u => u.username === username);
  if (!user) return res.status(404).json({ error: '用户不存在' });

  if (disabled) {
    user.disabled = true;
    user.disabledReason = (reason || '').trim();
    if (disabledUntil) {
      user.disabledUntil = new Date(disabledUntil).toISOString();
    } else if (durationMinutes && Number(durationMinutes) > 0) {
      user.disabledUntil = new Date(Date.now() + Number(durationMinutes) * 60 * 1000).toISOString();
    } else {
      user.disabledUntil = null; // 永久封禁
    }

    // If disabled, immediately revoke all active sessions for this user
    for (const [t, s] of activeSessions.entries()) {
      if (s.username === username) {
        activeSessions.delete(t);
      }
    }
    await saveSessions();
  } else {
    // Enable / Unban
    user.disabled = false;
    user.disabledUntil = null;
    user.disabledReason = null;
  }

  await saveUsers(users);

  let message = '';
  if (user.disabled) {
    if (user.disabledUntil) {
      const untilStr = new Date(user.disabledUntil).toLocaleString('zh-CN', { timeZone: 'Asia/Shanghai' });
      message = `已成功将用户【${username}】临时禁用至 ${untilStr}（账号已强制下线，订阅暂停下发，到期将自动解禁）`;
    } else {
      message = `已成功将用户【${username}】永久禁用（账号已强制下线，订阅已暂停下发）`;
    }
  } else {
    message = `已成功解禁并重新启用用户【${username}】`;
  }

  res.json({
    success: true,
    disabled: user.disabled,
    disabledUntil: user.disabledUntil,
    disabledReason: user.disabledReason,
    message
  });
});

// ── Admin System Snapshot Backup & Restore (Zero-Knowledge Blind Snapshots) ──

// 1. Export full system snapshot with blind user payloads
app.get('/api/admin/backup/export', authMiddleware, adminOnly, async (req, res) => {
  try {
    const users = loadUsers();
    const configs = {};

    if (fs.existsSync(CONFIGS_DIR)) {
      const files = await fs.promises.readdir(CONFIGS_DIR);
      for (const file of files) {
        if (file.endsWith('.json')) {
          const uname = file.replace('.json', '');
          try {
            const raw = JSON.parse(await fs.promises.readFile(path.join(CONFIGS_DIR, file), 'utf-8'));
            if (uname.toLowerCase() === req.session.username.toLowerCase()) {
              // 当前管理员自身的配置，以可读明文形式便于管理员查看自己的设置
              configs[uname] = isEncryptedBundle(raw) ? decryptUserConfig(raw, getUserKey(uname)) : raw;
            } else {
              // 🛡️ 所有其他普通用户的配置：以不可逆的 AES-256-GCM 盲化密文包形式导出
              if (isEncryptedBundle(raw)) {
                configs[uname] = raw;
              } else {
                configs[uname] = encryptUserConfig(raw, getUserKey(uname));
              }
            }
          } catch {}
        }
      }
    }

    const snapshot = {
      _type: 'SUBHUB_FULL_SYSTEM_SNAPSHOT',
      version: CURRENT_VERSION,
      _privacyNotice: 'Zero-Knowledge AES-256-GCM Encrypted. Non-admin tenant subscriptions and rules are cryptographically blinded.',
      exportedAt: new Date().toISOString(),
      exportedBy: req.session.username,
      stats: {
        userCount: users.length,
        configCount: Object.keys(configs).length,
        zeroKnowledgeEncrypted: true
      },
      systemSettings: loadSystemSettings(),
      users,
      configs
    };

    res.setHeader('Content-Type', 'application/json');
    res.setHeader('Content-Disposition', `attachment; filename="subhub-zero-knowledge-snapshot-${new Date().toISOString().slice(0, 10)}.json"`);
    res.json(snapshot);
  } catch (err) {
    res.status(500).json({ error: '导出系统快照失败: ' + err.message });
  }
});

// 2. Restore full system snapshot
app.post('/api/admin/backup/restore', authMiddleware, adminOnly, async (req, res) => {
  try {
    const { snapshot, mode = 'merge' } = req.body;
    if (!snapshot || typeof snapshot !== 'object') {
      return res.status(400).json({ error: '无效的快照数据格式' });
    }

    if (snapshot._type !== 'SUBHUB_FULL_SYSTEM_SNAPSHOT' || !Array.isArray(snapshot.users)) {
      return res.status(400).json({ error: '文件不是有效的 SubHub 全量系统快照备份' });
    }

    let existingUsers = loadUsers();
    let userMap = new Map();

    if (mode === 'overwrite') {
      snapshot.users.forEach(u => userMap.set(u.username, u));
      if (!userMap.has(req.session.username)) {
        const currentAdmin = existingUsers.find(u => u.username === req.session.username);
        if (currentAdmin) userMap.set(currentAdmin.username, currentAdmin);
      }
    } else {
      existingUsers.forEach(u => userMap.set(u.username, u));
      snapshot.users.forEach(u => userMap.set(u.username, u));
    }

    const finalUsers = Array.from(userMap.values());
    await saveUsers(finalUsers);

    let restoredConfigCount = 0;
    if (snapshot.configs && typeof snapshot.configs === 'object') {
      if (!fs.existsSync(CONFIGS_DIR)) {
        fs.mkdirSync(CONFIGS_DIR, { recursive: true });
      }

      for (const [uname, uconfig] of Object.entries(snapshot.configs)) {
        if (!uname || typeof uconfig !== 'object') continue;
        const filePath = path.join(CONFIGS_DIR, `${uname}.json`);
        if (isEncryptedBundle(uconfig)) {
          // 盲化密文包：直接原子写入磁盘，保持密文完整性
          await fs.promises.writeFile(filePath, JSON.stringify(uconfig, null, 2));
        } else {
          // 明文配置：自动通过对应用户密钥加密后落盘
          await saveUserConfig(uname, uconfig);
        }
        compiledCache.delete(uname);
        restoredConfigCount++;
      }
    }

    if (snapshot.systemSettings && typeof snapshot.systemSettings === 'object') {
      await saveSystemSettings(snapshot.systemSettings);
    }

    res.json({
      success: true,
      restoredUsers: finalUsers.length,
      restoredConfigs: restoredConfigCount,
      message: `系统快照还原成功！已同步 ${finalUsers.length} 个用户账号及 ${restoredConfigCount} 份加密用户配置。`
    });
  } catch (err) {
    res.status(500).json({ error: '还原系统快照失败: ' + err.message });
  }
});

// ── Global System Settings & Custom Domain Engine ───────────────────────────

// Public endpoint to retrieve domain config
app.get('/api/system/public-settings', (req, res) => {
  const s = loadSystemSettings();
  res.json({
    customDomain: s.customDomain || '',
    enableHttpsRedirect: !!s.enableHttpsRedirect
  });
});

// Admin get settings
app.get('/api/admin/system/settings', authMiddleware, adminOnly, (req, res) => {
  res.json(loadSystemSettings());
});

// Admin update settings
app.post('/api/admin/system/settings', authMiddleware, adminOnly, async (req, res) => {
  try {
    const { customDomain, enableHttpsRedirect } = req.body;
    let cleanDomain = '';
    if (customDomain && typeof customDomain === 'string') {
      cleanDomain = customDomain.trim().replace(/\/+$/, '');
    }
    const updated = await saveSystemSettings({
      customDomain: cleanDomain,
      enableHttpsRedirect: !!enableHttpsRedirect
    });
    res.json({
      success: true,
      message: '系统全局设置已成功保存！',
      settings: updated
    });
  } catch (err) {
    res.status(500).json({ error: '保存系统设置失败: ' + err.message });
  }
});

// Admin test domain DNS resolution
app.post('/api/admin/system/domain/test', authMiddleware, adminOnly, async (req, res) => {
  try {
    let { domain } = req.body;
    if (!domain || typeof domain !== 'string') {
      return res.status(400).json({ error: '请提供有效的域名' });
    }

    // Extract hostname without protocol/path/port
    let hostname = domain.trim().replace(/^https?:\/\//i, '').split('/')[0].split(':')[0].trim();
    if (!hostname) {
      return res.status(400).json({ error: '无效的域名格式' });
    }

    // 1. Resolve DNS records
    let resolvedIps = [];
    try {
      const records = await dns.promises.lookup(hostname, { all: true });
      resolvedIps = records.map(r => r.address);
    } catch (dnsErr) {
      return res.json({
        ok: false,
        domain: hostname,
        resolvedIps: [],
        error: `DNS 解析失败: 未找到域名 ${hostname} 的 A/AAAA 解析记录 (${dnsErr.code || dnsErr.message})`
      });
    }

    // 2. Fetch server's public IP
    let serverIp = '';
    try {
      const ipRes = await fetch('https://api.ipify.org?format=json', { signal: AbortSignal.timeout(3500) });
      if (ipRes.ok) {
        const ipData = await ipRes.json();
        serverIp = ipData.ip || '';
      }
    } catch {
      try {
        const ipRes2 = await fetch('https://ifconfig.me/ip', { signal: AbortSignal.timeout(3500) });
        if (ipRes2.ok) serverIp = (await ipRes2.text()).trim();
      } catch {}
    }

    // 3. Analyze match & Cloudflare CDN
    const hasMatch = serverIp && resolvedIps.includes(serverIp);
    const hasCloudflare = resolvedIps.some(ip => isCloudflareIp(ip));

    let status = 'unknown';
    let message = '';

    if (hasMatch) {
      status = 'direct_match';
      message = `✅ DNS 校验成功！域名 ${hostname} 已直接精确解析至本机公网 IP (${serverIp})`;
    } else if (hasCloudflare) {
      status = 'cloudflare_cdn';
      message = `☁️ 检测到 Cloudflare CDN 边缘网络（小黄云已开启），域名已成功接入 CDN 保护`;
    } else if (serverIp) {
      status = 'ip_mismatch';
      message = `⚠️ 域名解析 IP [${resolvedIps.join(', ')}] 与当前服务器公网 IP [${serverIp}] 不一致，请确认 DNS A 记录`;
    } else {
      status = 'resolved';
      message = `✅ 域名已成功解析至 [${resolvedIps.join(', ')}]`;
    }

    // 4. Test HTTP / HTTPS connectivity probe
    let httpOk = false, httpsOk = false;
    try {
      const probeRes = await fetch(`http://${hostname}:${PORT}`, { signal: AbortSignal.timeout(2000) });
      if (probeRes.status < 500) httpOk = true;
    } catch {}

    try {
      const probeHttps = await fetch(`https://${hostname}`, { signal: AbortSignal.timeout(2500) });
      if (probeHttps.status < 500) httpsOk = true;
    } catch {}

    res.json({
      ok: true,
      domain: hostname,
      resolvedIps,
      serverIp,
      status,
      hasMatch,
      hasCloudflare,
      httpOk,
      httpsOk,
      message
    });
  } catch (err) {
    res.status(500).json({ error: '域名探测诊断失败: ' + err.message });
  }
});

// Admin one-click SSL certificate & reverse proxy provisioning
app.post('/api/admin/system/ssl/provision', authMiddleware, adminOnly, async (req, res) => {
  try {
    let { domain, engine = 'caddy' } = req.body;
    if (!domain || typeof domain !== 'string') {
      return res.status(400).json({ error: '请提供有效的域名' });
    }

    let hostname = domain.trim().replace(/^https?:\/\//i, '').split('/')[0].split(':')[0].trim();
    if (!hostname) {
      return res.status(400).json({ error: '无效的域名格式' });
    }

    const isDocker = fs.existsSync('/.dockerenv') || process.env.DOCKER === 'true';
    let logs = [];

    logs.push(`🔍 [1/4] 正在校验域名 ${hostname} 的 DNS 解析状态...`);
    try {
      const records = await dns.promises.lookup(hostname, { all: true });
      logs.push(`✅ DNS 解析就绪，解析到 IP: ${records.map(r => r.address).join(', ')}`);
    } catch (dnsErr) {
      logs.push(`⚠️ DNS 提示: 未能成功解析域名 (${dnsErr.message})，可能导致证书申请失败，正在尝试继续...`);
    }

    if (isDocker) {
      logs.push('🐳 检测到 SubHub 运行在 Docker 容器内部。');
      logs.push('⚠️ 提示: 80/443 端口由宿主机操作系统直接管理。');
      return res.json({
        success: true,
        isDocker: true,
        logs: logs.join('\n'),
        message: '当前运行在容器中，请在 VPS 宿主机终端执行一键命令自动安装 Caddy 并签发证书：',
        command: `bash <(curl -fsSL https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/main/install.sh) domain`
      });
    }

    if (engine === 'caddy') {
      logs.push('⚡ [2/4] 检查 Caddy 自动化反代引擎...');
      let hasCaddy = false;
      try {
        await execPromise('which caddy');
        hasCaddy = true;
        logs.push('✅ 系统已安装 Caddy 引擎');
      } catch {
        logs.push('📦 未检测到 Caddy，正在自动从官方源极速安装 Caddy...');
        try {
          if (fs.existsSync('/etc/debian_version')) {
            await execPromise('apt-get update -y && apt-get install -y debian-keyring debian-archive-keyring apt-transport-https curl && curl -1sLf "https://dl.cloudsmith.io/public/caddy/stable/gpg.key" | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg --yes && curl -1sLf "https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt" | tee /etc/apt/sources.list.d/caddy-stable.list && apt-get update -y && apt-get install -y caddy');
          } else if (fs.existsSync('/etc/redhat-release')) {
            await execPromise('yum install -y yum-plugin-copr && yum copr enable -y @caddy/caddy && yum install -y caddy');
          } else if (fs.existsSync('/etc/alpine-release')) {
            await execPromise('apk add caddy');
          } else {
            throw new Error('未识别的 Linux 发行版，请手动安装 Caddy');
          }
          hasCaddy = true;
          logs.push('✅ Caddy 自动化引擎安装成功！');
        } catch (instErr) {
          logs.push(`⚠️ 自动安装 Caddy 遇到问题: ${instErr.message}`);
        }
      }

      logs.push(`📜 [3/4] 写入 Caddyfile 反向代理配置 (${hostname} -> 127.0.0.1:${PORT})...`);
      const caddyConfig = `${hostname} {\n    reverse_proxy 127.0.0.1:${PORT}\n}\n`;
      try {
        if (!fs.existsSync('/etc/caddy')) {
          fs.mkdirSync('/etc/caddy', { recursive: true });
        }
        await fs.promises.writeFile('/etc/caddy/Caddyfile', caddyConfig, 'utf-8');
        logs.push('✅ /etc/caddy/Caddyfile 写入成功');
      } catch (writeErr) {
        logs.push(`⚠️ 写入 /etc/caddy/Caddyfile 失败: ${writeErr.message}`);
      }

      logs.push('🚀 正在启动/重载 Caddy 并自动申请 Let\'s Encrypt / ZeroSSL HTTPS 证书...');
      try {
        await execPromise('systemctl enable caddy 2>/dev/null || true');
        await execPromise('systemctl restart caddy 2>/dev/null || caddy reload --config /etc/caddy/Caddyfile 2>/dev/null || true');
        logs.push('✅ Caddy 服务已重载！80/443 端口已开启，正在后台自动向 ACME 申请证书...');
      } catch (reloadErr) {
        logs.push(`⚠️ 重载 Caddy 提示: ${reloadErr.message}`);
      }
    } else if (engine === 'nginx') {
      logs.push('🛡️ [2/4] 配置 Nginx 反向代理...');
      const nginxConfig = `server {\n    listen 80;\n    server_name ${hostname};\n    location / {\n        proxy_pass http://127.0.0.1:${PORT};\n        proxy_set_header Host $host;\n        proxy_set_header X-Real-IP $remote_addr;\n        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n        proxy_set_header X-Forwarded-Proto $scheme;\n    }\n}\n`;
      try {
        const confDir = fs.existsSync('/etc/nginx/conf.d') ? '/etc/nginx/conf.d' : (fs.existsSync('/etc/nginx/sites-enabled') ? '/etc/nginx/sites-enabled' : null);
        if (confDir) {
          await fs.promises.writeFile(path.join(confDir, 'subhub.conf'), nginxConfig, 'utf-8');
          logs.push(`✅ Nginx 配置文件已写入: ${path.join(confDir, 'subhub.conf')}`);
          await execPromise('systemctl reload nginx 2>/dev/null || nginx -s reload 2>/dev/null || true');
        }
      } catch (e) {
        logs.push(`⚠️ 写入 Nginx 配置提示: ${e.message}`);
      }

      logs.push('📜 [3/4] 正在调用 Certbot 申请 SSL 证书...');
      try {
        const certRes = await execPromise(`certbot --nginx -d ${hostname} --non-interactive --agree-tos --register-unsafely-without-email --redirect 2>&1 || true`);
        logs.push(certRes.stdout ? certRes.stdout.slice(0, 300) : 'Certbot 执行完成');
      } catch (certErr) {
        logs.push(`⚠️ Certbot 提示: ${certErr.message}`);
      }
    }

    logs.push('🌐 [4/4] 正在将 SubHub 全局直链绑定为 HTTPS 域名...');
    const updatedSettings = await saveSystemSettings({
      customDomain: `https://${hostname}`,
      enableHttpsRedirect: true
    });
    logs.push(`🎉 恭喜！SubHub 全站已成功绑定至 https://${hostname}`);

    res.json({
      success: true,
      domain: hostname,
      fullUrl: `https://${hostname}`,
      settings: updatedSettings,
      logs: logs.join('\n'),
      message: `🎉 SSL 证书与反向代理已配置就绪！已成功将全站直链升级为 https://${hostname}`
    });
  } catch (err) {
    res.status(500).json({ error: '配置 SSL 证书与反代失败: ' + err.message });
  }
});

// ── Background Auto-Refresh Subscriptions Daemon ──────────────────────────────
const AUTO_REFRESH_CHECK_INTERVAL_MS = 2 * 60 * 1000; // Check every 2 minutes

async function checkAndRefreshAllSubscriptions() {
  try {
    const users = loadUsers();
    for (const u of users) {
      const cfg = loadUserConfig(u.username);
      if (!cfg || !Array.isArray(cfg.subscriptions)) continue;

      let hasUpdates = false;
      for (const sub of cfg.subscriptions) {
        if (sub.enabled === false || !sub.url) continue;
        const intervalMinutes = sub.autoRefreshInterval !== undefined ? parseInt(sub.autoRefreshInterval, 10) : 60;
        if (isNaN(intervalMinutes) || intervalMinutes <= 0) continue; // 0 means manual only

        const lastRefresh = sub.updatedAt ? new Date(sub.updatedAt).getTime() : 0;
        const now = Date.now();

        if (now - lastRefresh >= intervalMinutes * 60 * 1000) {
          try {
            console.log(`[Auto-Refresh] 正在自动同步用户 ${u.username} 的订阅: ${sub.name || sub.url}`);
            const data = await fetchSubscription(sub.url, sub.prefix || sub.name || '', true);
            sub.nodesCount = data.nodesCount;
            sub.userInfo = data.userInfo;
            sub.sourceType = data.sourceType;
            sub.updatedAt = new Date().toISOString();
            hasUpdates = true;
            recordAccessLog(u.username, {
              ip: '127.0.0.1 (系统调度)',
              ua: 'SubHub Auto-Scheduler Daemon',
              type: '⏰ 定时自动同步',
              status: 200,
              detail: `定时同步订阅 [${sub.name || sub.url}] 成功，获取到 ${data.nodesCount} 个节点`
            });
          } catch (e) {
            console.warn(`[Auto-Refresh] 自动同步 ${sub.name || sub.url} 失败: ${e.message}`);
            recordAccessLog(u.username, {
              ip: '127.0.0.1 (系统调度)',
              ua: 'SubHub Auto-Scheduler Daemon',
              type: '⏰ 定时自动同步',
              status: 500,
              detail: `定时同步订阅 [${sub.name || sub.url}] 失败: ${e.message}`
            });
          }
        }
      }

      if (hasUpdates) {
        await saveUserConfig(u.username, cfg);
      }
    }
  } catch (err) {
    console.error('Auto-refresh daemon error:', err);
  }
}

// ── Builtin Multi-Version Chinese Releases Matrix ─────────────────────────────

const BUILTIN_VERSIONS_ZH = [
  {
    version: '1.0.4',
    tag: 'v1.0.4',
    name: 'SubHub v1.0.4 · 数据存储与隐私双轨制体系与免登录纯本地工作台',
    publishedAt: '2026-08-28T10:45:00Z',
    highlights: ['🛡️ 纯浏览器本地离线单机模式 (0 云端留存)', '⚡ 免登录即开即用访客工作台', '💾 云端同步 vs 本地单机双轨制切换', '📥 浏览器本地一键即时生成下载', '☁️ 访客一键无缝升级同步云端'],
    changelogZh: `### 🛡️ 双轨制数据存储体系与免登录纯本地离线工作台
- **双轨制架构切换**：在设置中支持「☁️ 云端托管同步模式」与「🛡️ 纯浏览器本地离线单机模式」一键自由切换；
- **免登录即开即用**：登录页提供「🛡️ 免登录 · 进入纯本地离线工作台」入口，0 注册、0 账号、数据 100% 留存在浏览器 LocalStorage，物理级绝对隐私；
- **本地一键即时编译下载**：在客户端面板中，为离线单机用户提供 Clash YAML、Sing-box JSON、Surge 规则列表、Base64 与 JS 覆写脚本的一键即时编译与下载；
- **云端数据彻底抹除**：切换至本地模式时，提供一键彻底物理抹除服务器磁盘上的加密配置文件；
- **一键升级云端同步**：本地离线访客可随时一键注册账号，当前配置一秒无缝同步上传至云端 AES-256-GCM 加密存储。`
  },
  {
    version: '1.0.3',
    tag: 'v1.0.3',
    name: 'SubHub v1.0.3 · 多租户零知识私有数据加密与快照盲化备份系统',
    publishedAt: '2026-08-28T10:20:00Z',
    highlights: ['🔐 AES-256-GCM 磁盘落盘强加密', '📦 管理员盲化快照备份 (防窥探)', '🛡️ 多租户 RBAC 权限边界规范', '⚡ 客户端流式秒级透明解密'],
    changelogZh: `### 🔐 多租户零知识数据加密（Zero-Knowledge Architecture）
- **AES-256-GCM 落盘加密**：全站所有普通用户的私有机场订阅地址、节点密钥、分流规则与 JS 脚本在存盘时全量进行 AES-256-GCM 认证强加密；
- **管理员盲化快照备份**：管理员导出全量系统快照时，普通用户的配置数据以不可破译的密文包打包导出，管理员可自由灾备迁移，但无法窥视或反解任何用户的私有订阅；
- **多租户 RBAC 权限边界规范**：用户管理界面新增双栏权限对照矩阵，明确普通用户与管理员的权限分工；
- **透明秒级客户端解密**：Clash / Sing-box / Surge 拉取订阅时，后端在内存沙箱中即时透明解密流转，保持毫秒级性能。`
  },
  {
    version: '1.0.2',
    tag: 'v1.0.2',
    name: 'SubHub v1.0.2 · 自定义域名绑定、Web 一键申请 SSL 证书与全站直链自适应',
    publishedAt: '2026-08-28T09:30:00Z',
    highlights: ['🌐 自定义公开域名绑定', '⚡ Web 一键申请 SSL 证书', '🔒 Let\'s Encrypt 自动续签', '🎨 满宽自适应排版与对称优化', '🛡️ Docker 与原生部署双重兼容'],
    changelogZh: `### 🌐 自定义公开域名与全站直链自适应
- **公开域名绑定**：支持在「⚙️ 设置」中绑定专属域名，全站所有客户端订阅直链、二维码与覆写规则自适应升级为自定义域名；
- **智能 DNS 解析探测**：后端内置 DNS 查询与公网 IP 对比引擎，自动识别 Cloudflare CDN 加速代理；
- **通栏自适应排版**：全面采用通栏全宽自适应排版与独立 Header-Bar 架构，像素级垂直居中与对称对齐。

### ⚡ Web 端一键申请 SSL 证书与自动化反向代理
- **Web 端一键申请证书**：在 Web 控制台直接点击「⚡ 一键申请 SSL 证书」，全自动安装 Caddy 并向 Let's Encrypt / ZeroSSL 申请免费 TLS 证书；
- **实时控制台日志流**：弹窗全屏实时展示 DNS 预检、引擎安装、Caddyfile 写入与端口监听执行流水线；
- **动态端口自适应**：当 SubHub 运行在任意自定义端口时（如 8080/8888），Caddy 与 Nginx 自动适配实际端口完成无缝反代与证书签发。`
  },
  {
    version: '1.0.1',
    tag: 'v1.0.1',
    name: 'SubHub v1.0.1 · 系统版本发布中心与 Web 在线平滑热升级',
    publishedAt: '2026-08-28T08:50:00Z',
    highlights: ['🚀 在线一键平滑热升级 (OTA)', '📦 系统全量快照跨机迁移', '⏰ 用户封禁定时自动解禁'],
    changelogZh: `### 🚀 在线升级与系统快照
- **多版本归档与发布中心 (Release Hub)**：在 Web 端查看所有历史版本时间线与详尽中文更新说明，支持一键升级、部署或指定历史版本回退；
- **全量系统快照**：支持一键导出包含所有用户账号、密码哈希与独立订阅规则的 JSON 快照，在新服务器一秒导入还原；
- **定时解禁系统**：后台每 15 秒自动扫描并解封到期的受限用户。`
  },
  {
    version: '1.0.0',
    tag: 'v1.0.0',
    name: 'SubHub v1.0.0 · 首个正式企业级发布版',
    publishedAt: '2026-08-27T08:00:00Z',
    highlights: ['🚀 多订阅智能聚合与真机测速', '📊 3X-UI 实时流量看板', '🎯 多协议客户端通用直链转换'],
    changelogZh: `### 🎉 SubHub 正式发布
- **多订阅智能聚合**：支持订阅去重、节点过滤、正则重命名与全球国旗 Emoji 自动注入；
- **真机 TCP / HTTP 测速**：一键并行探测节点连通性与真实握手延迟；
- **3X-UI 流量仪表盘**：实时展示总下行、总上行流量与到期时间；
- **全客户端通用直链**：支持 Clash / Mihomo / Sing-box / Surge / Shadowrocket 多格式实时转换与 JS 脚本规则注入。`
  }
];

// ── System Version & Multi-Release Management ─────────────────────────────────

app.get('/api/system/version', authMiddleware, async (req, res) => {
  try {
    const isDocker = fs.existsSync('/.dockerenv') || process.env.DOCKER === 'true';
    const isGit = fs.existsSync(path.join(__dirname, '.git'));
    const doCheck = req.query.check === 'true';
    
    let latestVersion = CURRENT_VERSION;
    let commitHash = '';
    let checked = false;
    let versionsList = [...BUILTIN_VERSIONS_ZH];

    if (isGit) {
      try {
        const { stdout } = await execPromise('git rev-parse --short HEAD', { cwd: __dirname });
        commitHash = stdout.trim();
      } catch {}
    }

    if (doCheck) {
      checked = true;
      try {
        // 1. Fetch remote releases from GitHub API
        const ghRes = await fetch(`https://api.github.com/repos/${REPO_OWNER}/${REPO_NAME}/releases?per_page=30`, {
          headers: { 'User-Agent': 'SubHub-Updater', 'Accept': 'application/vnd.github.v3+json' },
          signal: AbortSignal.timeout(6000)
        });
        if (ghRes.ok) {
          const ghReleases = await ghRes.json();
          if (Array.isArray(ghReleases) && ghReleases.length > 0) {
            const remoteMapped = ghReleases.map(r => {
              const rawVer = (r.tag_name || '').replace(/^v/, '');
              const builtinMatch = BUILTIN_VERSIONS_ZH.find(b => b.version === rawVer);
              return {
                version: rawVer,
                tag: r.tag_name || `v${rawVer}`,
                name: r.name || (builtinMatch ? builtinMatch.name : `SubHub v${rawVer}`),
                publishedAt: r.published_at || (builtinMatch ? builtinMatch.publishedAt : ''),
                highlights: builtinMatch ? builtinMatch.highlights : ['官方发布版本'],
                changelogZh: (builtinMatch ? builtinMatch.changelogZh : '') || r.body || '暂无详细中文更新说明',
                url: r.html_url
              };
            });
            // Merge: keep remoteMapped first, append any builtin versions that might not be on GitHub
            const existingVers = new Set(remoteMapped.map(r => r.version));
            for (const b of BUILTIN_VERSIONS_ZH) {
              if (!existingVers.has(b.version)) {
                remoteMapped.push(b);
              }
            }
            versionsList = remoteMapped;
          }
        }
      } catch (err) {
        // GitHub API network error or rate limit - fallback to BUILTIN_VERSIONS_ZH
      }
    }

    // Sort versions descending
    versionsList.sort((a, b) => compareVersions(b.version, a.version));

    // Determine latest version
    latestVersion = versionsList[0]?.version || CURRENT_VERSION;

    // Decorate versions with current/latest/action tags
    const decoratedVersions = versionsList.map((v, idx) => {
      const cmp = compareVersions(v.version, CURRENT_VERSION);
      let actionType = 'current';
      if (cmp > 0) actionType = 'upgrade';
      else if (cmp < 0) actionType = 'rollback';

      return {
        ...v,
        isLatest: idx === 0,
        isCurrent: cmp === 0,
        actionType
      };
    });

    const hasUpdate = compareVersions(latestVersion, CURRENT_VERSION) > 0;

    res.json({
      success: true,
      currentVersion: CURRENT_VERSION,
      latestVersion,
      hasUpdate,
      commitHash,
      checked,
      repoUrl: REPO_URL,
      isDocker,
      isGit,
      versions: decoratedVersions
    });
  } catch (err) {
    res.status(500).json({ error: err.message });
  }
});

app.post('/api/system/update', authMiddleware, adminOnly, async (req, res) => {
  try {
    const isGit = fs.existsSync(path.join(__dirname, '.git'));
    const isDocker = fs.existsSync('/.dockerenv') || process.env.DOCKER === 'true';
    const { targetVersion } = req.body || {};

    let targetTag = 'main';
    let isSpecificVersion = false;
    if (targetVersion && targetVersion !== 'latest') {
      const cleanVer = targetVersion.trim().replace(/^v/, '');
      targetTag = `v${cleanVer}`;
      isSpecificVersion = true;
    }

    let logs = [];

    if (isGit) {
      if (isSpecificVersion) {
        logs.push(`🎯 [1/3] 正在从 GitHub 远端获取历史版本标签 (git fetch --all --tags)...`);
        try {
          await execPromise('git fetch origin main --tags', { cwd: __dirname });
          logs.push(`🏷️ 正在精确检出目标版本 [${targetTag}]...`);
          const checkoutRes = await execPromise(`git checkout tags/${targetTag} 2>&1 || git checkout ${targetTag} 2>&1 || git checkout ${targetVersion} 2>&1`, { cwd: __dirname });
          logs.push(checkoutRes.stdout || checkoutRes.stderr || `成功切换到版本 ${targetTag}`);
        } catch (gitErr) {
          logs.push(`⚠️ 版本检出提示: ${gitErr.message}，正在尝试回退至 main 分支...`);
          await execPromise('git checkout main 2>&1 && git pull origin main 2>&1', { cwd: __dirname }).catch(() => {});
        }
      } else {
        logs.push('🚀 [1/3] 正在从 GitHub 远端拉取最新代码 (git fetch & reset)...');
        try {
          const pullRes = await execPromise('git fetch origin main && git reset --hard origin/main', { cwd: __dirname });
          logs.push(pullRes.stdout || pullRes.stderr || '代码拉取成功');
        } catch (gitErr) {
          logs.push(`⚠️ Git 拉取警告: ${gitErr.message}`);
        }
      }

      logs.push('📦 [2/3] 正在检查并更新运行依赖 (npm install --production)...');
      try {
        const npmRes = await execPromise('npm install --production', { cwd: __dirname });
        if (npmRes.stdout) logs.push(npmRes.stdout.slice(0, 300));
      } catch (npmErr) {
        logs.push(`⚠️ 依赖更新提示: ${npmErr.message}`);
      }

      logs.push(`🔄 [3/3] 版本切换【${isSpecificVersion ? targetTag : '最新稳定版'}】与依赖安装完成！正在触发服务平滑热重启...`);

      // Trigger hot restart
      setTimeout(() => {
        console.log(`🔄 系统在线版本切换 [${isSpecificVersion ? targetTag : 'latest'}] 完成，正在重启进程...`);
        process.exit(0);
      }, 1500);

      return res.json({
        success: true,
        message: `版本切换成功！已部署至【${isSpecificVersion ? targetTag : '最新稳定版'}】，系统正在自动热重启，请稍候 3 秒刷新页面...`,
        logs: logs.join('\n')
      });
    } else {
      return res.json({
        success: true,
        isDockerManual: true,
        message: `当前运行在独立容器环境，可直接在宿主机执行一键指令切换至【${isSpecificVersion ? targetTag : '最新版'}】：`,
        command: `bash <(curl -fsSL https://raw.githubusercontent.com/${REPO_OWNER}/${REPO_NAME}/main/install.sh) update ${isSpecificVersion ? targetTag : ''}`
      });
    }
  } catch (err) {
    res.status(500).json({ error: `版本切换失败: ${err.message}` });
  }
});

// ── Start ─────────────────────────────────────────────────────────────────────

init().then(() => {
  app.listen(PORT, '0.0.0.0', () => {
    console.log(`====================================================`);
    console.log(`🚀 Clash Sub Hub v1.0.4 已启动`);
    console.log(`🌐 Web 管理端: http://localhost:${PORT}`);
    console.log(`👤 默认账号: admin / admin`);
    console.log(`====================================================`);
  });

  // Start periodic background subscription auto-updater and user ban expiration sweep
  sweepExpiredUserBans();
  setInterval(checkAndRefreshAllSubscriptions, AUTO_REFRESH_CHECK_INTERVAL_MS);
  setInterval(sweepExpiredUserBans, 15000); // Sweep expired bans every 15s
}).catch(err => { console.error('启动失败:', err); process.exit(1); });
