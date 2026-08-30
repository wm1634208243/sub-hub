use crate::models::ProxyNode;
use base64::Engine;
use percent_encoding::percent_decode_str;
use std::collections::HashMap;
use url::Url;

pub fn parse_node_link(link: &str, prefix: &str) -> Option<ProxyNode> {
    let raw = link.trim();
    if raw.is_empty() {
        return None;
    }

    if raw.starts_with("vless://") {
        parse_vless(raw, prefix)
    } else if raw.starts_with("vmess://") {
        parse_vmess(raw, prefix)
    } else if raw.starts_with("trojan://") {
        parse_trojan(raw, prefix)
    } else if raw.starts_with("ss://") {
        parse_shadowsocks(raw, prefix)
    } else if raw.starts_with("hysteria2://") || raw.starts_with("hy2://") {
        parse_hysteria2(raw, prefix)
    } else if raw.starts_with("tuic://") {
        parse_tuic(raw, prefix)
    } else {
        None
    }
}

fn build_name(raw_name: &str, prefix: &str) -> String {
    let clean: String = raw_name
        .replace('\u{FFFD}', "")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    let name = clean.trim();
    if name.is_empty() {
        if prefix.is_empty() { "Node".to_string() } else { format!("[{}] Node", prefix) }
    } else if prefix.is_empty() {
        name.to_string()
    } else {
        format!("[{}] {}", prefix, name)
    }
}

fn parse_vless(link: &str, prefix: &str) -> Option<ProxyNode> {
    let url = Url::parse(link).ok()?;
    let uuid = url.username().to_string();
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);
    let tag = url.fragment().map(|f| percent_decode_str(f).decode_utf8_lossy().to_string()).unwrap_or_default();
    let name = build_name(&tag, prefix);

    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let network = query.get("type").cloned().unwrap_or_else(|| "tcp".into());
    let security = query.get("security").cloned().unwrap_or_default();
    let is_tls = security == "tls" || security == "reality";
    let sni = query.get("sni").cloned();
    let flow = query.get("flow").cloned();
    let fp = query.get("fp").cloned();

    let mut reality_opts = None;
    if security == "reality" {
        let mut map = serde_json::Map::new();
        if let Some(pbk) = query.get("pbk") {
            map.insert("public-key".into(), serde_json::Value::String(pbk.clone()));
        }
        if let Some(sid) = query.get("sid") {
            map.insert("short-id".into(), serde_json::Value::String(sid.clone()));
        }
        reality_opts = Some(serde_json::Value::Object(map));
    }

    let mut ws_opts = None;
    if network == "ws" {
        let mut map = serde_json::Map::new();
        if let Some(path) = query.get("path") {
            map.insert("path".into(), serde_json::Value::String(path.clone()));
        }
        if let Some(host) = query.get("host") {
            let mut headers = serde_json::Map::new();
            headers.insert("Host".into(), serde_json::Value::String(host.clone()));
            map.insert("headers".into(), serde_json::Value::Object(headers));
        }
        ws_opts = Some(serde_json::Value::Object(map));
    }

    let mut grpc_opts = None;
    if network == "grpc" {
        let mut map = serde_json::Map::new();
        if let Some(service_name) = query.get("serviceName") {
            map.insert("grpc-service-name".into(), serde_json::Value::String(service_name.clone()));
        }
        grpc_opts = Some(serde_json::Value::Object(map));
    }

    let mut extra = HashMap::new();
    if let Some(fl) = flow {
        extra.insert("flow".into(), serde_json::Value::String(fl));
    }

    Some(ProxyNode {
        name,
        node_type: "vless".into(),
        server,
        port,
        uuid: Some(uuid),
        cipher: Some("none".into()),
        network: Some(network),
        tls: Some(is_tls),
        udp: Some(true),
        servername: sni.clone(),
        sni,
        alpn: query.get("alpn").map(|a| a.split(',').map(|s| s.trim().to_string()).collect()),
        skip_cert_verify: query.get("allowInsecure").map(|v| v == "1"),
        client_fingerprint: fp,
        ws_opts,
        grpc_opts,
        reality_opts,
        raw_name: Some(tag),
        extra,
        ..Default::default()
    })
}

fn parse_vmess(link: &str, prefix: &str) -> Option<ProxyNode> {
    let b64_str = link.strip_prefix("vmess://")?;
    let decoded = base64::engine::general_purpose::STANDARD.decode(b64_str.trim()).ok()?;
    let json_val: serde_json::Value = serde_json::from_slice(&decoded).ok()?;

    let server = json_val.get("add")?.as_str()?.to_string();
    let port = json_val.get("port")?.as_u64()? as u16;
    let uuid = json_val.get("id")?.as_str()?.to_string();
    let alter_id = json_val.get("aid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let raw_name = json_val.get("ps").and_then(|v| v.as_str()).unwrap_or("VMess");
    let name = build_name(raw_name, prefix);

    let net = json_val.get("net").and_then(|v| v.as_str()).unwrap_or("tcp").to_string();
    let tls = json_val.get("tls").and_then(|v| v.as_str()).map(|t| t == "tls").unwrap_or(false);
    let cipher = json_val.get("scy").and_then(|v| v.as_str()).unwrap_or("auto").to_string();
    let sni = json_val.get("sni").and_then(|v| v.as_str()).map(|s| s.to_string());

    let mut ws_opts = None;
    if net == "ws" {
        let mut map = serde_json::Map::new();
        if let Some(path) = json_val.get("path").and_then(|v| v.as_str()) {
            map.insert("path".into(), serde_json::Value::String(path.to_string()));
        }
        if let Some(host) = json_val.get("host").and_then(|v| v.as_str()) {
            let mut headers = serde_json::Map::new();
            headers.insert("Host".into(), serde_json::Value::String(host.to_string()));
            map.insert("headers".into(), serde_json::Value::Object(headers));
        }
        ws_opts = Some(serde_json::Value::Object(map));
    }

    Some(ProxyNode {
        name,
        node_type: "vmess".into(),
        server,
        port,
        uuid: Some(uuid),
        cipher: Some(cipher),
        alter_id: Some(alter_id),
        network: Some(net),
        tls: Some(tls),
        udp: Some(true),
        servername: sni.clone(),
        sni,
        ws_opts,
        raw_name: Some(raw_name.to_string()),
        ..Default::default()
    })
}

fn parse_trojan(link: &str, prefix: &str) -> Option<ProxyNode> {
    let url = Url::parse(link).ok()?;
    let password = url.password().unwrap_or(url.username()).to_string();
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);
    let tag = url.fragment().map(|f| percent_decode_str(f).decode_utf8_lossy().to_string()).unwrap_or_default();
    let name = build_name(&tag, prefix);

    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let sni = query.get("sni").or_else(|| query.get("peer")).cloned();

    Some(ProxyNode {
        name,
        node_type: "trojan".into(),
        server,
        port,
        password: Some(password),
        network: query.get("type").cloned(),
        tls: Some(true),
        udp: Some(true),
        servername: sni.clone(),
        sni,
        alpn: query.get("alpn").map(|a| a.split(',').map(|s| s.trim().to_string()).collect()),
        skip_cert_verify: query.get("allowInsecure").map(|v| v == "1"),
        raw_name: Some(tag),
        ..Default::default()
    })
}

fn parse_shadowsocks(link: &str, prefix: &str) -> Option<ProxyNode> {
    let body = link.strip_prefix("ss://")?;
    let (main_part, tag) = match body.split_once('#') {
        Some((m, t)) => (m, percent_decode_str(t).decode_utf8_lossy().to_string()),
        None => (body, "SS".to_string()),
    };
    let name = build_name(&tag, prefix);

    let (cipher, password, server, port) = if main_part.contains('@') {
        let (user_info, host_port) = main_part.split_once('@')?;
        let decoded_user = base64::engine::general_purpose::STANDARD
            .decode(user_info)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_else(|| user_info.to_string());
        let (c, p) = decoded_user.split_once(':')?;
        let (s, port_str) = host_port.split_once(':')?;
        (c.to_string(), p.to_string(), s.to_string(), port_str.parse::<u16>().ok()?)
    } else {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(main_part)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())?;
        let (user_info, host_port) = decoded.split_once('@')?;
        let (c, p) = user_info.split_once(':')?;
        let (s, port_str) = host_port.split_once(':')?;
        (c.to_string(), p.to_string(), s.to_string(), port_str.parse::<u16>().ok()?)
    };

    Some(ProxyNode {
        name,
        node_type: "ss".into(),
        server,
        port,
        password: Some(password),
        cipher: Some(cipher),
        udp: Some(true),
        raw_name: Some(tag),
        ..Default::default()
    })
}

fn parse_hysteria2(link: &str, prefix: &str) -> Option<ProxyNode> {
    let clean = link.strip_prefix("hysteria2://").or_else(|| link.strip_prefix("hy2://"))?;
    let url = Url::parse(&format!("hysteria2://{}", clean)).ok()?;
    let auth = url.username().to_string();
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);
    let tag = url.fragment().map(|f| percent_decode_str(f).decode_utf8_lossy().to_string()).unwrap_or_default();
    let name = build_name(&tag, prefix);

    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let sni = query.get("sni").cloned();
    let insecure = query.get("insecure").map(|v| v == "1");

    Some(ProxyNode {
        name,
        node_type: "hysteria2".into(),
        server,
        port,
        password: Some(auth.clone()),
        tls: Some(true),
        udp: Some(true),
        servername: sni.clone(),
        sni,
        alpn: query.get("alpn").map(|a| a.split(',').map(|s| s.trim().to_string()).collect()),
        skip_cert_verify: insecure,
        auth: Some(auth),
        insecure,
        raw_name: Some(tag),
        ..Default::default()
    })
}

fn parse_tuic(link: &str, prefix: &str) -> Option<ProxyNode> {
    let url = Url::parse(link).ok()?;
    let uuid = url.username().to_string();
    let password = url.password().unwrap_or_default().to_string();
    let server = url.host_str()?.to_string();
    let port = url.port().unwrap_or(443);
    let tag = url.fragment().map(|f| percent_decode_str(f).decode_utf8_lossy().to_string()).unwrap_or_default();
    let name = build_name(&tag, prefix);

    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let sni = query.get("sni").cloned();

    Some(ProxyNode {
        name,
        node_type: "tuic".into(),
        server,
        port,
        uuid: Some(uuid),
        password: Some(password),
        tls: Some(true),
        udp: Some(true),
        servername: sni.clone(),
        sni,
        alpn: query.get("alpn").map(|a| a.split(',').map(|s| s.trim().to_string()).collect()),
        skip_cert_verify: query.get("allow_insecure").map(|v| v == "1"),
        raw_name: Some(tag),
        ..Default::default()
    })
}
