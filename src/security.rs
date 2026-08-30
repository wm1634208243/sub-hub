use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue},
    middleware::Next,
    response::Response,
};
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

lazy_static! {
    // Pre-computed bcrypt hash of "dummy_timing_protection_secret_salt" to equalize execution time on invalid usernames
    pub static ref DUMMY_BCRYPT_HASH: &'static str = "$2a$10$w8.mCkWpM9oDqjV3bH1tSuO8oW1/lFkR3G1eK5rY7uI9oP2aScD5e";
}

// ── Rate Limiter Engine ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct RateEntry {
    attempts: u32,
    window_start: Instant,
    locked_until: Option<Instant>,
}

#[derive(Clone, Default)]
pub struct RateLimiter {
    entries: Arc<RwLock<HashMap<String, RateEntry>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if the given key is currently locked. Returns Err(remaining_duration) if locked.
    pub async fn check(&self, key: &str) -> Result<(), Duration> {
        let map = self.entries.read().await;
        if let Some(entry) = map.get(key) {
            if let Some(locked_until) = entry.locked_until {
                let now = Instant::now();
                if now < locked_until {
                    return Err(locked_until - now);
                }
            }
        }
        Ok(())
    }

    /// Record a failed attempt. If attempts exceed max_attempts within window, locks for lock_duration.
    pub async fn record_failure(
        &self,
        key: &str,
        max_attempts: u32,
        window: Duration,
        lock_duration: Duration,
    ) -> Option<Duration> {
        let mut map = self.entries.write().await;
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert(RateEntry {
            attempts: 0,
            window_start: now,
            locked_until: None,
        });

        // Reset window if expired and not locked
        if entry.locked_until.is_none() && now.duration_since(entry.window_start) > window {
            entry.attempts = 0;
            entry.window_start = now;
        }

        entry.attempts += 1;

        if entry.attempts >= max_attempts {
            let lock_until = now + lock_duration;
            entry.locked_until = Some(lock_until);
            return Some(lock_duration);
        }

        None
    }

    /// Clear failed attempts upon successful authentication
    pub async fn record_success(&self, key: &str) {
        let mut map = self.entries.write().await;
        map.remove(key);
    }

    /// Periodic cleanup of expired entries to prevent memory leak
    pub async fn cleanup(&self) {
        let mut map = self.entries.write().await;
        let now = Instant::now();
        map.retain(|_, entry| {
            if let Some(locked) = entry.locked_until {
                now < locked
            } else {
                now.duration_since(entry.window_start) < Duration::from_secs(3600)
            }
        });
    }
}

// ── Timing Attack Mitigation ──────────────────────────────────────────────────

/// Execute a dummy bcrypt verification to equalize response timing when a username does not exist
pub fn dummy_bcrypt_verify(password: &str) {
    let _ = bcrypt::verify(password, *DUMMY_BCRYPT_HASH);
}

// ── Input & Path Validation ───────────────────────────────────────────────────

/// Validate username against path traversal, control characters, and injection
pub fn validate_username(username: &str) -> Result<(), &'static str> {
    let uname = username.trim();
    if uname.is_empty() {
        return Err("用户名不能为空");
    }
    if uname.len() > 64 {
        return Err("用户名长度不能超过 64 个字符");
    }
    if uname.contains('/') || uname.contains('\\') || uname.contains("..") || uname.contains('\0') {
        return Err("用户名包含非法路径字符");
    }
    if !uname.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '@' || c == '.') {
        return Err("用户名仅支持字母、数字、下划线与中划线");
    }
    Ok(())
}

/// Validate subscription URL or raw node links
pub fn validate_subscription_url(url: &str) -> Result<(), &'static str> {
    let u = url.trim();
    if u.is_empty() {
        return Err("订阅链接或节点内容不能为空");
    }
    if u.starts_with("http://") || u.starts_with("https://")
        || u.starts_with("vless://") || u.starts_with("vmess://")
        || u.starts_with("trojan://") || u.starts_with("ss://")
        || u.starts_with("hysteria2://") || u.starts_with("hy2://")
        || u.starts_with("tuic://") || u.starts_with("socks5://") {
        if u.len() > 65536 {
            return Err("订阅链接长度超出安全限制");
        }
        return Ok(());
    }
    // Also allow multi-line node links or raw base64 content
    if u.lines().any(|l| l.trim().contains("://")) || u.len() > 20 {
        if u.len() > 65536 {
            return Err("节点内容长度超出安全限制");
        }
        return Ok(());
    }
    Err("请输入有效的 HTTP/HTTPS 订阅链接或单节点链接 (vless://, vmess://, trojan://, ss:// 等)")
}

// ── Client IP & UA Extractor ──────────────────────────────────────────────────

pub fn extract_client_ip(headers: &HeaderMap) -> String {
    let candidates = [
        "cf-connecting-ip",
        "true-client-ip",
        "x-real-ip",
        "x-forwarded-for",
    ];

    for name in &candidates {
        if let Some(val) = headers.get(*name).and_then(|v| v.to_str().ok()) {
            let first_ip = val.split(',').next().unwrap_or(val).trim();
            if !first_ip.is_empty() && first_ip.len() <= 64 {
                return first_ip.to_string();
            }
        }
    }

    "127.0.0.1".to_string()
}

// ── Security Headers Middleware ───────────────────────────────────────────────

pub async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();

    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        "x-frame-options",
        HeaderValue::from_static("SAMEORIGIN"),
    );
    headers.insert(
        "x-xss-protection",
        HeaderValue::from_static("1; mode=block"),
    );
    headers.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("geolocation=(), camera=(), microphone=()"),
    );

    resp
}
