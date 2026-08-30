use crate::engine::protocol_parser::parse_node_link;
use crate::engine::renamer::is_announcement_node;
use crate::models::ProxyNode;
use base64::Engine;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct FetchResult {
    pub url: String,
    pub prefix: String,
    pub nodes: Vec<ProxyNode>,
    pub user_info: Option<serde_json::Value>,
    pub source_type: String,
    pub updated_at: String,
}

#[derive(Clone)]
struct CacheEntry {
    result: FetchResult,
    cached_at: Instant,
}

pub struct SubscriptionFetcher {
    client: Client,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
}

impl SubscriptionFetcher {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(12))
            .danger_accept_invalid_certs(true)
            .user_agent("ClashMeta/v1.18.0 (Clash.Meta; Mihomo; SubHub)")
            .build()
            .unwrap_or_default();

        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn clear_cache(&self, url: Option<&str>) {
        let mut map = self.cache.write().await;
        if let Some(u) = url {
            map.remove(u);
        } else {
            map.clear();
        }
    }

    pub async fn fetch(&self, sub_url: &str, prefix: &str, force_refresh: bool) -> Result<FetchResult, String> {
        let url = sub_url.trim();
        if let Err(msg) = crate::security::validate_subscription_url(url) {
            return Err(msg.to_string());
        }

        // Direct single/multiple node links (vless://, vmess://, trojan://, ss://, hy2://, tuic://, Base64)
        if !url.starts_with("http://") && !url.starts_with("https://") {
            let nodes = parse_subscription_content(url, prefix);
            if nodes.is_empty() {
                return Err("无法从输入的链接中解析出有效的代理节点，请检查节点链接格式是否正确".into());
            }
            let source_type = if nodes.len() == 1 {
                format!("自建单节点 ({})", nodes[0].node_type.to_uppercase())
            } else {
                format!("自定义节点池 ({} 个节点)", nodes.len())
            };
            return Ok(FetchResult {
                url: url.to_string(),
                prefix: prefix.to_string(),
                nodes,
                user_info: None,
                source_type,
                updated_at: chrono::Utc::now().to_rfc3339(),
            });
        }

        if !force_refresh {
            let map = self.cache.read().await;
            if let Some(entry) = map.get(url) {
                if entry.cached_at.elapsed() < Duration::from_secs(600) {
                    let mut res = entry.result.clone();
                    res.prefix = prefix.to_string();
                    return Ok(res);
                }
            }
        }

        let resp = self.client.get(url).send().await.map_err(|e| format!("连接上游订阅超时或网络错误: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("上游订阅响应错误 (HTTP {})", resp.status()));
        }

        let headers = resp.headers().clone();
        let userinfo_header = headers
            .get("subscription-userinfo")
            .or_else(|| headers.get("Subscription-Userinfo"))
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();

        let user_info = parse_user_info(userinfo_header);
        let body_text = resp.text().await.map_err(|e| format!("读取订阅响应正文失败: {}", e))?;
        let nodes = parse_subscription_content(&body_text, prefix);
        let source_type = detect_source_type(url, &body_text, &nodes);

        let result = FetchResult {
            url: url.to_string(),
            prefix: prefix.to_string(),
            nodes,
            user_info,
            source_type,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };

        {
            let mut map = self.cache.write().await;
            map.insert(url.to_string(), CacheEntry {
                result: result.clone(),
                cached_at: Instant::now(),
            });
        }

        Ok(result)
    }
}

pub fn parse_user_info(header: &str) -> Option<serde_json::Value> {
    if header.trim().is_empty() {
        return None;
    }

    let mut upload: u64 = 0;
    let mut download: u64 = 0;
    let mut total: u64 = 0;
    let mut expire: Option<u64> = None;

    for part in header.split(';') {
        if let Some((k, v)) = part.split_once('=') {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            if let Ok(num) = val.parse::<u64>() {
                match key.as_str() {
                    "upload" => upload = num,
                    "download" => download = num,
                    "total" => total = num,
                    "expire" => expire = Some(num * 1000),
                    _ => {}
                }
            }
        }
    }

    let used = upload + download;
    let remaining = if total > used { total - used } else { 0 };
    let percent_used = if total > 0 { ((used as f64 / total as f64) * 100.0).round() as u32 } else { 0 };

    Some(serde_json::json!({
        "upload": upload,
        "download": download,
        "total": total,
        "used": used,
        "remaining": remaining,
        "percentUsed": percent_used,
        "expire": expire
    }))
}

pub fn parse_subscription_content(content: &str, prefix: &str) -> Vec<ProxyNode> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return vec![];
    }

    // 1. Try parse as Clash YAML
    if trimmed.contains("proxies:") || trimmed.starts_with("port:") || trimmed.starts_with("mixed-port:") {
        if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(trimmed) {
            if let Some(proxies_arr) = val.get("proxies").and_then(|p| p.as_sequence()) {
                let mut nodes = Vec::new();
                for item in proxies_arr {
                    if let Ok(mut node) = serde_yaml::from_value::<ProxyNode>(item.clone()) {
                        if !is_announcement_node(&node.name, &node.server, node.port) {
                            if !prefix.is_empty() {
                                node.name = format!("[{}] {}", prefix, node.name);
                            }
                            nodes.push(node);
                        }
                    }
                }
                for n in &mut nodes {
                    n.extra.remove("name");
                }
                if !nodes.is_empty() {
                    return nodes;
                }
            }
        }
    }

    // 2. Try Base64 decoding
    let mut target_text = trimmed.to_string();
    let clean_b64 = trimmed.replace(&['\r', '\n', ' ', '\t'][..], "");
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&clean_b64) {
        if let Ok(decoded_str) = String::from_utf8(decoded) {
            if decoded_str.contains("://") || decoded_str.contains("vmess://") {
                target_text = decoded_str;
            }
        }
    }

    let mut nodes = Vec::new();
    for line in target_text.lines() {
        let l = line.trim();
        if !l.is_empty() {
            if let Some(node) = parse_node_link(l, prefix) {
                if !is_announcement_node(&node.name, &node.server, node.port) {
                    nodes.push(node);
                }
            }
        }
    }

    for n in &mut nodes {
        n.extra.remove("name");
    }

    nodes
}

pub fn detect_source_type(sub_url: &str, body_text: &str, nodes: &[ProxyNode]) -> String {
    let url = sub_url.to_lowercase();
    if url.contains(":2096") || url.contains(":2053") || url.contains(":54321") || url.contains("/sub/") || url.contains("/clash/") || url.contains("/xui") || url.contains("3x-ui") {
        return "3X-UI / X-UI".into();
    }
    if url.contains("v2board") || url.contains("/api/v1/client/subscribe") || url.contains("sspanel") || url.contains("mod_sub") {
        return "商业机场 (V2board/SSPanel)".into();
    }
    if url.contains("github.com") || url.contains("raw.githubusercontent.com") || url.contains("gitlab.com") {
        return "GitHub 托管源".into();
    }
    if url.contains("sub?target=") || url.contains("subconverter") {
        return "Subconverter 转换".into();
    }

    let trimmed = body_text.trim();
    if trimmed.starts_with("proxies:") || trimmed.contains("proxy-groups:") || trimmed.starts_with("port:") {
        return "Clash YAML".into();
    }

    if let Some(first) = nodes.first() {
        return format!("{} 节点池", first.node_type.to_uppercase());
    }

    "标准代理订阅".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fetch_direct_vless_node_link() {
        let fetcher = SubscriptionFetcher::new();
        let raw_vless = "vless://5935302c-3d1c-4063-9b90-8b66fbb6d71d@154.36.179.51:443?type=tcp&security=reality&pbk=JNB0fv4NYiJr4etyNEu7S7b_hGdhxbPJz2yo0ALhaTw&fp=chrome&sni=www.cloudflare.com&sid=ee1a1954#%E9%80%90%E7%BB%B4%E4%BA%91JP";
        let res = fetcher.fetch(raw_vless, "", false).await.expect("Should parse vless node directly");
        assert_eq!(res.nodes.len(), 1);
        assert_eq!(res.nodes[0].name, "逐维云JP");
        assert_eq!(res.nodes[0].server, "154.36.179.51");
        assert_eq!(res.nodes[0].port, 443);
        assert_eq!(res.nodes[0].node_type, "vless");
    }
}

