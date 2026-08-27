/**
 * Protocol Parser for Clash / Mihomo
 * Parses single protocol URI links (vless, vmess, trojan, ss, hysteria2, tuic)
 * into standard Clash proxy objects.
 */

export function parseNodeLink(link, prefix = '') {
  if (!link || typeof link !== 'string') return null;
  link = link.trim();

  try {
    if (link.startsWith('vless://')) {
      return parseVless(link, prefix);
    }
    if (link.startsWith('vmess://')) {
      return parseVmess(link, prefix);
    }
    if (link.startsWith('trojan://')) {
      return parseTrojan(link, prefix);
    }
    if (link.startsWith('ss://')) {
      return parseShadowsocks(link, prefix);
    }
    if (link.startsWith('hysteria2://') || link.startsWith('hy2://')) {
      return parseHysteria2(link, prefix);
    }
    if (link.startsWith('tuic://')) {
      return parseTuic(link, prefix);
    }
  } catch (err) {
    // ignore malformed single node
  }

  return null;
}

function formatName(name, prefix) {
  const cleanName = (name || 'Node').trim();
  return prefix ? `[${prefix}] ${cleanName}` : cleanName;
}

// 1. VLESS (3X-UI standard)
function parseVless(uri, prefix) {
  const url = new URL(uri);
  const uuid = url.username;
  const server = url.hostname;
  const port = parseInt(url.port || '443', 10);
  const name = formatName(decodeURIComponent(url.hash.slice(1)) || `${server}:${port}`, prefix);
  const params = url.searchParams;

  const type = params.get('type') || 'tcp';
  const security = params.get('security') || 'none';
  const flow = params.get('flow');
  const sni = params.get('sni') || params.get('peer');
  const fp = params.get('fp') || 'chrome';
  const pbk = params.get('pbk');
  const sid = params.get('sid');
  const path = params.get('path') || '/';
  const host = params.get('host');
  const serviceName = params.get('serviceName');

  const proxy = {
    name,
    type: 'vless',
    server,
    port,
    uuid,
    udp: true
  };

  if (flow) proxy.flow = flow;

  if (security === 'tls') {
    proxy.tls = true;
    if (sni) proxy.servername = sni;
    if (fp) proxy['client-fingerprint'] = fp;
  } else if (security === 'reality') {
    proxy.tls = true;
    if (sni) proxy.servername = sni;
    if (fp) proxy['client-fingerprint'] = fp;
    proxy['reality-opts'] = {};
    if (pbk) proxy['reality-opts']['public-key'] = pbk;
    if (sid) proxy['reality-opts']['short-id'] = sid;
  }

  if (type === 'ws') {
    proxy.network = 'ws';
    proxy['ws-opts'] = { path };
    if (host) proxy['ws-opts'].headers = { Host: host };
  } else if (type === 'grpc') {
    proxy.network = 'grpc';
    proxy['grpc-opts'] = {
      'grpc-service-name': serviceName || path.replace(/^\//, '')
    };
  } else if (type === 'http' || type === 'h2') {
    proxy.network = 'http';
    proxy['http-opts'] = {
      path: [path]
    };
    if (host) proxy['http-opts'].headers = { Host: [host] };
  }

  return proxy;
}

// 2. VMess
function parseVmess(uri, prefix) {
  const b64 = uri.slice(8).trim();
  const raw = Buffer.from(b64, 'base64').toString('utf-8');
  const json = JSON.parse(raw);

  const name = formatName(json.ps || `${json.add}:${json.port}`, prefix);
  const proxy = {
    name,
    type: 'vmess',
    server: json.add,
    port: parseInt(json.port, 10),
    uuid: json.id,
    alterId: parseInt(json.aid || '0', 10),
    cipher: json.scy || 'auto',
    udp: true
  };

  if (json.tls === 'tls' || json.tls === '1') {
    proxy.tls = true;
    if (json.sni) proxy.servername = json.sni;
    if (json.fp) proxy['client-fingerprint'] = json.fp;
  }

  const net = json.net || 'tcp';
  if (net === 'ws') {
    proxy.network = 'ws';
    proxy['ws-opts'] = {
      path: json.path || '/'
    };
    if (json.host) proxy['ws-opts'].headers = { Host: json.host };
  } else if (net === 'grpc') {
    proxy.network = 'grpc';
    proxy['grpc-opts'] = {
      'grpc-service-name': json.path || ''
    };
  }

  return proxy;
}

// 3. Trojan
function parseTrojan(uri, prefix) {
  const url = new URL(uri);
  const password = url.username || url.password;
  const server = url.hostname;
  const port = parseInt(url.port || '443', 10);
  const name = formatName(decodeURIComponent(url.hash.slice(1)) || `${server}:${port}`, prefix);
  const params = url.searchParams;

  const sni = params.get('sni') || params.get('peer');
  const type = params.get('type') || 'tcp';

  const proxy = {
    name,
    type: 'trojan',
    server,
    port,
    password,
    udp: true,
    sni: sni || server
  };

  const fp = params.get('fp');
  if (fp) proxy['client-fingerprint'] = fp;

  if (type === 'ws') {
    proxy.network = 'ws';
    proxy['ws-opts'] = {
      path: params.get('path') || '/'
    };
    const host = params.get('host');
    if (host) proxy['ws-opts'].headers = { Host: host };
  } else if (type === 'grpc') {
    proxy.network = 'grpc';
    proxy['grpc-opts'] = {
      'grpc-service-name': params.get('serviceName') || ''
    };
  }

  return proxy;
}

// 4. Shadowsocks
function parseShadowsocks(uri, prefix) {
  let raw = uri.slice(5);
  let name = '';
  if (raw.includes('#')) {
    const parts = raw.split('#');
    raw = parts[0];
    name = decodeURIComponent(parts[1]);
  }

  let userinfo = '', server = '', port = 0;
  if (raw.includes('@')) {
    const atParts = raw.split('@');
    let encodedUserInfo = atParts[0];
    try {
      userinfo = Buffer.from(encodedUserInfo, 'base64').toString('utf-8');
    } catch {
      userinfo = encodedUserInfo;
    }
    const hostParts = atParts[1].split(':');
    server = hostParts[0];
    port = parseInt(hostParts[1], 10);
  } else {
    // entire string base64 encoded
    const decoded = Buffer.from(raw, 'base64').toString('utf-8');
    const match = decoded.match(/^(.*?):(.*?)@(.*?):(\d+)$/);
    if (match) {
      userinfo = `${match[1]}:${match[2]}`;
      server = match[3];
      port = parseInt(match[4], 10);
    }
  }

  const [cipher, password] = userinfo.split(':');
  if (!server || !port || !cipher || !password) return null;

  return {
    name: formatName(name || `${server}:${port}`, prefix),
    type: 'ss',
    server,
    port,
    cipher,
    password,
    udp: true
  };
}

// 5. Hysteria 2
function parseHysteria2(uri, prefix) {
  const url = new URL(uri.replace(/^hy2:\/\//, 'hysteria2://'));
  const auth = url.username || url.password;
  const server = url.hostname;
  const port = parseInt(url.port || '443', 10);
  const name = formatName(decodeURIComponent(url.hash.slice(1)) || `${server}:${port}`, prefix);
  const params = url.searchParams;

  const proxy = {
    name,
    type: 'hysteria2',
    server,
    port,
    password: auth,
    sni: params.get('sni') || server
  };

  const obfs = params.get('obfs');
  const obfsPassword = params.get('obfs-password');
  if (obfs) {
    proxy.obfs = obfs;
    if (obfsPassword) proxy['obfs-password'] = obfsPassword;
  }

  return proxy;
}

// 6. TUIC
function parseTuic(uri, prefix) {
  const url = new URL(uri);
  const uuid = url.username;
  const password = url.password;
  const server = url.hostname;
  const port = parseInt(url.port || '443', 10);
  const name = formatName(decodeURIComponent(url.hash.slice(1)) || `${server}:${port}`, prefix);
  const params = url.searchParams;

  return {
    name,
    type: 'tuic',
    server,
    port,
    uuid,
    password,
    sni: params.get('sni') || server,
    'congestion-controller': params.get('congestion_control') || 'bbr',
    'udp-relay-mode': params.get('udp_relay_mode') || 'native',
    'reduce-rtt': true
  };
}
