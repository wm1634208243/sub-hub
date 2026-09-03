use crate::api::auth_handlers::AppState;
use crate::engine::format_converter::{
    build_clash_yaml_from_nodes, convert_to_base64, convert_to_clash_proxies_only,
    convert_to_loon_conf, convert_to_quanx_conf, convert_to_raw_links,
    convert_to_singbox_json, convert_to_surge_conf, detect_client_target,
};
use crate::engine::renamer::format_node_name;
use crate::models::ProxyNode;
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct ConvertQuery {
    pub url: Option<String>,
    pub target: Option<String>,
    pub emoji: Option<bool>,
    pub udp: Option<bool>,
    pub skip_cert_verify: Option<bool>,
    pub include: Option<String>,
    pub exclude: Option<String>,
    pub template: Option<String>,
    pub filename: Option<String>,
    pub token: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ConvertPreviewPayload {
    pub url: String,
    pub target: Option<String>,
    pub emoji: Option<bool>,
    pub udp: Option<bool>,
    pub skip_cert_verify: Option<bool>,
    pub include: Option<String>,
    pub exclude: Option<String>,
    pub template: Option<String>,
}

#[derive(Serialize)]
pub struct ConvertPreviewResponse {
    pub success: bool,
    pub content: String,
    pub target: String,
    #[serde(rename = "nodesCount")]
    pub nodes_count: usize,
    #[serde(rename = "userInfo")]
    pub user_info: Option<serde_json::Value>,
    #[serde(rename = "detectedCountries")]
    pub detected_countries: Vec<serde_json::Value>,
    #[serde(rename = "sampleNodes")]
    pub sample_nodes: Vec<serde_json::Value>,
}

/// Fetch, merge, and filter nodes from multiple raw links / sub URLs
async fn process_conversion_nodes(
    state: &AppState,
    raw_url_input: &str,
    emoji: bool,
    udp: bool,
    skip_cert_verify: bool,
    include_regex: Option<&str>,
    exclude_regex: Option<&str>,
) -> Result<(Vec<ProxyNode>, Option<serde_json::Value>, Option<String>), String> {
    let input = raw_url_input.trim();
    if input.is_empty() {
        return Err("源订阅链接或节点内容不能为空".into());
    }

    // Split input into individual sub URLs or raw node blocks
    let mut urls_to_fetch = Vec::new();
    for part in input.split(|c| c == '|' || c == '\n' || c == '\r') {
        let p = part.trim();
        if !p.is_empty() {
            urls_to_fetch.push(p.to_string());
        }
    }

    if urls_to_fetch.is_empty() {
        return Err("未提取到有效的订阅地址或节点内容".into());
    }

    let mut merged_nodes: Vec<ProxyNode> = Vec::new();
    let mut agg_upload: u64 = 0;
    let mut agg_download: u64 = 0;
    let mut agg_total: u64 = 0;
    let mut min_expire: Option<u64> = None;
    let mut has_user_info = false;

    for u in urls_to_fetch {
        match state.fetcher.fetch(&u, "", false).await {
            Ok(res) => {
                if let Some(ui) = res.user_info {
                    has_user_info = true;
                    if let Some(up) = ui.get("upload").and_then(|v| v.as_u64()) {
                        agg_upload += up;
                    }
                    if let Some(down) = ui.get("download").and_then(|v| v.as_u64()) {
                        agg_download += down;
                    }
                    if let Some(tot) = ui.get("total").and_then(|v| v.as_u64()) {
                        agg_total += tot;
                    }
                    if let Some(exp) = ui.get("expire").and_then(|v| v.as_u64()) {
                        min_expire = Some(min_expire.map_or(exp, |curr| curr.min(exp)));
                    }
                }
                merged_nodes.extend(res.nodes);
            }
            Err(e) => {
                tracing::warn!("SubConverter fetch error for [{}]: {}", u, e);
            }
        }
    }

    if merged_nodes.is_empty() {
        return Err("未能从提供的订阅源或节点中解析出任何有效代理节点".into());
    }

    // Filter by include / exclude regex
    let inc_re = include_regex
        .filter(|s| !s.trim().is_empty())
        .and_then(|pat| Regex::new(pat.trim()).ok());
    let exc_re = exclude_regex
        .filter(|s| !s.trim().is_empty())
        .and_then(|pat| Regex::new(pat.trim()).ok());

    let mut filtered_nodes = Vec::new();
    for mut node in merged_nodes {
        if let Some(re) = &inc_re {
            if !re.is_match(&node.name) {
                continue;
            }
        }
        if let Some(re) = &exc_re {
            if re.is_match(&node.name) {
                continue;
            }
        }

        // Apply emoji and renaming
        if emoji {
            node.name = format_node_name(&node.name, &node.server, true, true, &[], None);
        }

        // Apply UDP and Skip Cert Verify overrides
        if udp {
            node.udp = Some(true);
        }
        if skip_cert_verify {
            node.skip_cert_verify = Some(true);
        }

        filtered_nodes.push(node);
    }

    if filtered_nodes.is_empty() {
        return Err("所有节点均被正则规则过滤排除，无可用节点".into());
    }

    let user_info_header = if has_user_info || agg_total > 0 || agg_upload > 0 || agg_download > 0 || min_expire.is_some() {
        let mut s = format!("upload={}; download={}; total={}", agg_upload, agg_download, agg_total);
        if let Some(exp) = min_expire {
            let exp_sec = if exp > 100_000_000_000 { exp / 1000 } else { exp };
            s.push_str(&format!("; expire={}", exp_sec));
        }
        Some(s)
    } else {
        None
    };

    let user_info_json = if has_user_info {
        let used = agg_upload + agg_download;
        let percent_used = if agg_total > 0 { ((used as f64 / agg_total as f64) * 100.0).round() as u32 } else { 0 };
        Some(serde_json::json!({
            "upload": agg_upload,
            "download": agg_download,
            "total": agg_total,
            "used": used,
            "percentUsed": percent_used,
            "expire": min_expire
        }))
    } else {
        None
    };

    Ok((filtered_nodes, user_info_json, user_info_header))
}

/// GET /api/convert - Universal Conversion Subscription Endpoint for All Clients
pub async fn universal_convert_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ConvertQuery>,
) -> Response {
    let ip = crate::security::extract_client_ip(&headers);
    let ip_key = format!("convert_rate_ip:{}", ip);
    if let Err(remaining) = state.rate_limiter.check(&ip_key).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            format!("转换接口请求过于频繁，请等待 {} 秒后再试", remaining.as_secs().max(1)),
        ).into_response();
    }

    let ua = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default();
    let raw_url = match query.url {
        Some(u) if !u.trim().is_empty() => u,
        _ => return (StatusCode::BAD_REQUEST, "缺少 url 参数").into_response(),
    };

    let emoji = query.emoji.unwrap_or(true);
    let udp = query.udp.unwrap_or(true);
    let skip_cert = query.skip_cert_verify.unwrap_or(false);
    let tmpl = query.template.as_deref().unwrap_or("default");

    let (nodes, _ui_json, ui_header) = match process_conversion_nodes(
        &state,
        &raw_url,
        emoji,
        udp,
        skip_cert,
        query.include.as_deref(),
        query.exclude.as_deref(),
    ).await {
        Ok(res) => res,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("订阅转换失败: {}", e)).into_response(),
    };

    let target = query.target.unwrap_or_else(|| detect_client_target(ua).to_string());
    let target_lower = target.to_lowercase();

    let (content, content_type, filename) = match target_lower.as_str() {
        "clash" | "mihomo" | "clash.meta" => {
            let y = build_clash_yaml_from_nodes(&nodes, udp, tmpl);
            (y, "text/yaml; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-Clash.yaml".into()))
        }
        "clash_proxies" | "clash-proxies" | "proxies" => {
            let y = convert_to_clash_proxies_only(&nodes);
            (y, "text/yaml; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-Proxies.yaml".into()))
        }
        "singbox" | "sing-box" | "sfa" | "sfi" | "sfm" => {
            let j = convert_to_singbox_json(&nodes);
            (j, "application/json; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-SingBox.json".into()))
        }
        "surge" => {
            let c = convert_to_surge_conf(&nodes);
            (c, "text/plain; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-Surge.conf".into()))
        }
        "loon" => {
            let c = convert_to_loon_conf(&nodes);
            (c, "text/plain; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-Loon.conf".into()))
        }
        "quanx" | "quantumultx" | "quantumult_x" => {
            let c = convert_to_quanx_conf(&nodes);
            (c, "text/plain; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-QuantumultX.conf".into()))
        }
        "raw" | "nodes" | "links" => {
            let r = convert_to_raw_links(&nodes);
            (r, "text/plain; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-Nodes.txt".into()))
        }
        _ => {
            // Default Base64 (Shadowrocket / universal)
            let b = convert_to_base64(&nodes);
            (b, "text/plain; charset=utf-8", query.filename.unwrap_or_else(|| "SubHub-Base64.txt".into()))
        }
    };

    let mut resp = (StatusCode::OK, content).into_response();
    let resp_headers = resp.headers_mut();

    if let Ok(v) = HeaderValue::from_str(content_type) {
        resp_headers.insert(header::CONTENT_TYPE, v);
    }
    let disp = format!("attachment; filename=\"{}\"", filename);
    if let Ok(v) = HeaderValue::from_str(&disp) {
        resp_headers.insert(header::CONTENT_DISPOSITION, v);
    }
    if let Some(ui_h) = ui_header {
        if let Ok(v) = HeaderValue::from_str(&ui_h) {
            resp_headers.insert(HeaderName::from_static("subscription-userinfo"), v);
        }
    }

    resp
}

/// POST /api/convert/preview - Instant Web Preview and Node Inspection
pub async fn universal_convert_preview_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<ConvertPreviewPayload>,
) -> Result<Json<ConvertPreviewResponse>, (StatusCode, Json<serde_json::Value>)> {
    let ip = crate::security::extract_client_ip(&headers);
    let ip_key = format!("convert_rate_ip:{}", ip);
    if let Err(remaining) = state.rate_limiter.check(&ip_key).await {
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "error": format!("预览接口请求过于频繁，请等待 {} 秒后再试", remaining.as_secs().max(1))
            })),
        ));
    }

    let emoji = payload.emoji.unwrap_or(true);
    let udp = payload.udp.unwrap_or(true);
    let skip_cert = payload.skip_cert_verify.unwrap_or(false);
    let tmpl = payload.template.as_deref().unwrap_or("default");

    let (nodes, user_info, _ui_header) = process_conversion_nodes(
        &state,
        &payload.url,
        emoji,
        udp,
        skip_cert,
        payload.include.as_deref(),
        payload.exclude.as_deref(),
    ).await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e })),
        )
    })?;

    let target = payload.target.unwrap_or_else(|| "clash".into());
    let target_lower = target.to_lowercase();

    let content = match target_lower.as_str() {
        "clash" | "mihomo" => build_clash_yaml_from_nodes(&nodes, udp, tmpl),
        "clash_proxies" | "clash-proxies" | "proxies" => convert_to_clash_proxies_only(&nodes),
        "singbox" | "sing-box" => convert_to_singbox_json(&nodes),
        "surge" => convert_to_surge_conf(&nodes),
        "loon" => convert_to_loon_conf(&nodes),
        "quanx" | "quantumultx" => convert_to_quanx_conf(&nodes),
        "raw" | "nodes" => convert_to_raw_links(&nodes),
        _ => convert_to_base64(&nodes),
    };

    // Calculate country distribution and sample nodes
    let mut country_counts: HashMap<String, usize> = HashMap::new();
    let mut sample_nodes = Vec::new();

    for (idx, n) in nodes.iter().enumerate() {
        let mut region = "其他".to_string();
        if n.name.contains("🇭🇰") || n.name.contains("香港") || n.name.contains("HK") {
            region = "🇭🇰 香港".into();
        } else if n.name.contains("🇯🇵") || n.name.contains("日本") || n.name.contains("JP") || n.name.contains("东京") {
            region = "🇯🇵 日本".into();
        } else if n.name.contains("🇺🇸") || n.name.contains("美国") || n.name.contains("US") {
            region = "🇺🇸 美国".into();
        } else if n.name.contains("🇸🇬") || n.name.contains("新加坡") || n.name.contains("SG") {
            region = "🇸🇬 新加坡".into();
        } else if n.name.contains("🇹🇼") || n.name.contains("台湾") || n.name.contains("TW") {
            region = "🇹🇼 台湾".into();
        } else if n.name.contains("🇰🇷") || n.name.contains("韩国") || n.name.contains("KR") {
            region = "🇰🇷 韩国".into();
        } else if n.name.contains("🇬🇧") || n.name.contains("英国") || n.name.contains("UK") {
            region = "🇬🇧 英国".into();
        } else if n.name.contains("🇩🇪") || n.name.contains("德国") || n.name.contains("DE") {
            region = "🇩🇪 德国".into();
        }
        *country_counts.entry(region.clone()).or_insert(0) += 1;

        if idx < 20 {
            sample_nodes.push(serde_json::json!({
                "name": n.name,
                "type": n.node_type.to_uppercase(),
                "server": n.server,
                "port": n.port,
                "country": region
            }));
        }
    }

    let mut detected_countries: Vec<serde_json::Value> = country_counts
        .into_iter()
        .map(|(k, v)| serde_json::json!({ "label": k, "count": v }))
        .collect();
    detected_countries.sort_by(|a, b| {
        b.get("count").and_then(|v| v.as_u64()).unwrap_or(0)
            .cmp(&a.get("count").and_then(|v| v.as_u64()).unwrap_or(0))
    });

    Ok(Json(ConvertPreviewResponse {
        success: true,
        content,
        target,
        nodes_count: nodes.len(),
        user_info,
        detected_countries,
        sample_nodes,
    }))
}
