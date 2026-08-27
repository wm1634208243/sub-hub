/**
 * SubHub Latency Checker & Dead Node Filter
 * Fast parallel TCP/TLS connectivity & handshake latency tester
 */

import net from 'net';
import tls from 'tls';

// In-memory cache for node latency (TTL: 3 minutes)
const latencyCache = new Map();
const CACHE_TTL_MS = 3 * 60 * 1000;

/**
 * Probe a single proxy node for real TCP/TLS connection latency
 * @param {Object} proxy { server, port, type, tls, sni }
 * @param {number} timeoutMs
 * @param {boolean} forceRefresh
 * @returns {Promise<{alive: boolean, latency: number, error?: string}>}
 */
export async function probeNodeLatency(proxy, timeoutMs = 2000, forceRefresh = false) {
  if (!proxy || !proxy.server || !proxy.port) {
    return { alive: false, latency: -1, error: 'INVALID_CONFIG' };
  }

  const host = String(proxy.server).trim();
  const port = parseInt(proxy.port, 10);
  const cacheKey = `${host}:${port}`;

  if (!forceRefresh && latencyCache.has(cacheKey)) {
    const cached = latencyCache.get(cacheKey);
    if (Date.now() - cached.testedAt < CACHE_TTL_MS) {
      return { alive: cached.alive, latency: cached.latency, error: cached.error };
    }
  }

  const isTls = (
    proxy.tls === true ||
    proxy.type === 'trojan' ||
    proxy.type === 'hysteria2' ||
    proxy.type === 'vless' ||
    port === 443 ||
    port === 8443 ||
    port === 2053 ||
    port === 2083 ||
    port === 2087 ||
    port === 2096
  );

  const start = Date.now();
  let result = null;

  try {
    if (isTls) {
      result = await new Promise((resolve) => {
        let isResolved = false;
        const socket = tls.connect({
          host,
          port,
          servername: proxy.sni || (/^[0-9\.]+$/.test(host) ? undefined : host),
          rejectUnauthorized: false,
          timeout: timeoutMs
        });

        const cleanup = () => {
          try {
            socket.removeAllListeners();
            socket.destroy();
          } catch {}
        };

        socket.on('secureConnect', () => {
          if (!isResolved) {
            isResolved = true;
            const latency = Date.now() - start;
            cleanup();
            resolve({ alive: true, latency });
          }
        });

        socket.on('timeout', () => {
          if (!isResolved) {
            isResolved = true;
            cleanup();
            resolve({ alive: false, error: 'TIMEOUT', latency: -1 });
          }
        });

        socket.on('error', (err) => {
          if (!isResolved) {
            isResolved = true;
            cleanup();
            // SSL handshake failure on non-TLS server still proves port is listening and responsive!
            if (err.message && (err.message.includes('wrong version') || err.message.includes('packet length') || err.code === 'ECONNRESET')) {
              resolve({ alive: true, latency: Date.now() - start });
            } else {
              resolve({ alive: false, error: err.code || err.message, latency: -1 });
            }
          }
        });
      });
    } else {
      result = await new Promise((resolve) => {
        let isResolved = false;
        const socket = new net.Socket();
        socket.setTimeout(timeoutMs);

        const cleanup = () => {
          try {
            socket.removeAllListeners();
            socket.destroy();
          } catch {}
        };

        socket.on('connect', () => {
          if (!isResolved) {
            isResolved = true;
            const latency = Date.now() - start;
            cleanup();
            resolve({ alive: true, latency });
          }
        });

        socket.on('timeout', () => {
          if (!isResolved) {
            isResolved = true;
            cleanup();
            resolve({ alive: false, error: 'TIMEOUT', latency: -1 });
          }
        });

        socket.on('error', (err) => {
          if (!isResolved) {
            isResolved = true;
            cleanup();
            resolve({ alive: false, error: err.code || err.message, latency: -1 });
          }
        });

        socket.connect(port, host);
      });
    }
  } catch (e) {
    result = { alive: false, error: e.message, latency: -1 };
  }

  // Cache test result
  latencyCache.set(cacheKey, {
    alive: result.alive,
    latency: result.latency,
    error: result.error,
    testedAt: Date.now()
  });

  return result;
}

/**
 * Batch probe an array of proxies with a concurrency pool
 * @param {Array} proxies 
 * @param {Object} options { concurrency, timeoutMs, forceRefresh }
 */
export async function batchProbeProxies(proxies = [], options = {}) {
  const {
    concurrency = 20,
    timeoutMs = 2000,
    forceRefresh = false
  } = options;

  if (!Array.isArray(proxies) || proxies.length === 0) {
    return { proxies: [], aliveCount: 0, deadCount: 0, avgLatency: 0 };
  }

  const results = new Array(proxies.length);
  let cursor = 0;

  async function worker() {
    while (cursor < proxies.length) {
      const idx = cursor++;
      const proxy = proxies[idx];
      try {
        const probe = await probeNodeLatency(proxy, timeoutMs, forceRefresh);
        results[idx] = {
          ...proxy,
          alive: probe.alive,
          latency: probe.latency,
          latencyError: probe.error
        };
      } catch (err) {
        results[idx] = {
          ...proxy,
          alive: false,
          latency: -1,
          latencyError: err.message
        };
      }
    }
  }

  const poolSize = Math.min(concurrency, proxies.length);
  const workers = Array.from({ length: poolSize }, () => worker());
  await Promise.all(workers);

  let aliveCount = 0;
  let totalLatency = 0;

  for (const p of results) {
    if (p && p.alive) {
      aliveCount++;
      totalLatency += (p.latency || 0);
    }
  }

  const deadCount = results.length - aliveCount;
  const avgLatency = aliveCount > 0 ? Math.round(totalLatency / aliveCount) : 0;

  return {
    proxies: results,
    aliveCount,
    deadCount,
    avgLatency
  };
}

/**
 * Filter dead nodes and sort proxies by latency
 * @param {Array} proxies 
 * @param {Object} options { enableDeadNodeFilter, enableLatencySort }
 */
export function applyLatencyFilterAndSort(proxies = [], options = {}) {
  const { enableDeadNodeFilter = false, enableLatencySort = false } = options;
  let output = [...proxies];

  if (enableDeadNodeFilter) {
    output = output.filter(p => p.alive !== false);
  }

  if (enableLatencySort) {
    output.sort((a, b) => {
      const latA = (a.alive !== false && typeof a.latency === 'number' && a.latency >= 0) ? a.latency : 999999;
      const latB = (b.alive !== false && typeof b.latency === 'number' && b.latency >= 0) ? b.latency : 999999;
      return latA - latB;
    });
  }

  return output;
}
