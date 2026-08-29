use crate::models::ProxyNode;
use base64::Engine;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use url::Url;

pub fn proxy_to_uri(proxy: &ProxyNode) -> Option<String> {
    let tag = utf8_percent_encode(&proxy.name, NON_ALPHANUMERIC).to_string();

    match proxy.node_type.to_lowercase().as_str() {
        "vless" => {
            let uuid = proxy.uuid.as_deref().unwrap_or_default();
            let mut params = Vec::new();
            params.push(format!("encryption={}", proxy.cipher.as_deref().unwrap_or("none")));

            let net = proxy.network.as_deref().unwrap_or("tcp");
            params.push(format!("type={}", net));

            if let Some(ro) = &proxy.reality_opts {
                params.push("security=reality".into());
                if let Some(pbk) = ro.get("public-key").and_then(|v| v.as_str()) {
                    params.push(format!("pbk={}", pbk));
                }
                if let Some(sid) = ro.get("short-id").and_then(|v| v.as_str()) {
                    params.push(format!("sid={}", sid));
                }
                if let Some(sni) = &proxy.servername {
                    params.push(format!("sni={}", sni));
                }
                if let Some(fp) = &proxy.client_fingerprint {
                    params.push(format!("fp={}", fp));
                }
            } else if proxy.tls.unwrap_or(false) {
                params.push("security=tls".into());
                if let Some(sni) = &proxy.servername {
                    params.push(format!("sni={}", sni));
                }
                if let Some(fp) = &proxy.client_fingerprint {
                    params.push(format!("fp={}", fp));
                }
                if proxy.skip_cert_verify.unwrap_or(false) {
                    params.push("allowInsecure=1".into());
                }
            }

            if net == "ws" {
                if let Some(wo) = &proxy.ws_opts {
                    if let Some(path) = wo.get("path").and_then(|v| v.as_str()) {
                        params.push(format!("path={}", utf8_percent_encode(path, NON_ALPHANUMERIC)));
                    }
                    if let Some(host) = wo.get("headers").and_then(|h| h.get("Host")).and_then(|v| v.as_str()) {
                        params.push(format!("host={}", host));
                    }
                }
            } else if net == "grpc" {
                if let Some(go) = &proxy.grpc_opts {
                    if let Some(service_name) = go.get("grpc-service-name").and_then(|v| v.as_str()) {
                        params.push(format!("serviceName={}", service_name));
                        params.push("mode=gun".into());
                    }
                }
            }

            Some(format!(
                "vless://{}@{}:{}?{}#{}",
                uuid,
                proxy.server,
                proxy.port,
                params.join("&"),
                tag
            ))
        }

        "vmess" => {
            let mut map = serde_json::Map::new();
            map.insert("v".into(), serde_json::Value::String("2".into()));
            map.insert("ps".into(), serde_json::Value::String(proxy.name.clone()));
            map.insert("add".into(), serde_json::Value::String(proxy.server.clone()));
            map.insert("port".into(), serde_json::json!(proxy.port));
            map.insert("id".into(), serde_json::Value::String(proxy.uuid.clone().unwrap_or_default()));
            map.insert("aid".into(), serde_json::json!(proxy.alter_id.unwrap_or(0)));
            map.insert("scy".into(), serde_json::Value::String(proxy.cipher.clone().unwrap_or_else(|| "auto".into())));
            map.insert("net".into(), serde_json::Value::String(proxy.network.clone().unwrap_or_else(|| "tcp".into())));
            map.insert("type".into(), serde_json::Value::String("none".into()));

            let host = proxy.ws_opts.as_ref()
                .and_then(|w| w.get("headers"))
                .and_then(|h| h.get("Host"))
                .and_then(|v| v.as_str())
                .or(proxy.servername.as_deref())
                .unwrap_or_default();
            map.insert("host".into(), serde_json::Value::String(host.into()));

            let path = proxy.ws_opts.as_ref()
                .and_then(|w| w.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            map.insert("path".into(), serde_json::Value::String(path.into()));
            map.insert("tls".into(), serde_json::Value::String(if proxy.tls.unwrap_or(false) { "tls".into() } else { "".into() }));
            map.insert("sni".into(), serde_json::Value::String(proxy.servername.clone().unwrap_or_default()));

            let b64 = base64::engine::general_purpose::STANDARD.encode(serde_json::to_string(&map).unwrap_or_default());
            Some(format!("vmess://{}", b64))
        }

        "trojan" => {
            let pass = proxy.password.as_deref().unwrap_or_default();
            let mut params = Vec::new();
            if let Some(sni) = &proxy.servername {
                params.push(format!("sni={}", sni));
            }
            if proxy.skip_cert_verify.unwrap_or(false) {
                params.push("allowInsecure=1".into());
            }
            Some(format!(
                "trojan://{}@{}:{}?{}#{}",
                pass,
                proxy.server,
                proxy.port,
                params.join("&"),
                tag
            ))
        }

        "ss" => {
            let cipher = proxy.cipher.as_deref().unwrap_or("aes-256-gcm");
            let pass = proxy.password.as_deref().unwrap_or_default();
            let userinfo = format!("{}:{}", cipher, pass);
            let b64_user = base64::engine::general_purpose::STANDARD.encode(userinfo);
            Some(format!("ss://{}@{}:{}#{}", b64_user, proxy.server, proxy.port, tag))
        }

        "hysteria2" => {
            let auth = proxy.auth.as_deref().or(proxy.password.as_deref()).unwrap_or_default();
            let mut params = Vec::new();
            if let Some(sni) = &proxy.servername {
                params.push(format!("sni={}", sni));
            }
            if proxy.skip_cert_verify.unwrap_or(false) || proxy.insecure.unwrap_or(false) {
                params.push("insecure=1".into());
            }
            Some(format!(
                "hysteria2://{}@{}:{}?{}#{}",
                auth,
                proxy.server,
                proxy.port,
                params.join("&"),
                tag
            ))
        }

        "tuic" => {
            let uuid = proxy.uuid.as_deref().unwrap_or_default();
            let pass = proxy.password.as_deref().unwrap_or_default();
            let mut params = Vec::new();
            if let Some(sni) = &proxy.servername {
                params.push(format!("sni={}", sni));
            }
            Some(format!(
                "tuic://{}:{}@{}:{}?{}#{}",
                uuid,
                pass,
                proxy.server,
                proxy.port,
                params.join("&"),
                tag
            ))
        }

        _ => None,
    }
}

pub fn convert_to_base64(proxies: &[ProxyNode]) -> String {
    let mut uris = Vec::new();
    for p in proxies {
        if let Some(uri) = proxy_to_uri(p) {
            uris.push(uri);
        }
    }
    let joined = uris.join("\n");
    base64::engine::general_purpose::STANDARD.encode(joined)
}

pub fn convert_to_surge_list(proxies: &[ProxyNode]) -> String {
    let mut lines = Vec::new();
    for p in proxies {
        match p.node_type.to_lowercase().as_str() {
            "ss" => {
                let cipher = p.cipher.as_deref().unwrap_or("aes-256-gcm");
                let pass = p.password.as_deref().unwrap_or_default();
                lines.push(format!("{} = ss, {}, {}, encrypt-method={}, password={}", p.name, p.server, p.port, cipher, pass));
            }
            "trojan" => {
                let pass = p.password.as_deref().unwrap_or_default();
                let sni = p.servername.as_deref().unwrap_or(&p.server);
                lines.push(format!("{} = trojan, {}, {}, password={}, sni={}", p.name, p.server, p.port, pass, sni));
            }
            "vmess" => {
                let uuid = p.uuid.as_deref().unwrap_or_default();
                let ws = p.network.as_deref().unwrap_or("tcp") == "ws";
                lines.push(format!("{} = vmess, {}, {}, username={}, ws={}", p.name, p.server, p.port, uuid, ws));
            }
            _ => {}
        }
    }
    lines.join("\n")
}

pub fn convert_to_singbox_json(proxies: &[ProxyNode]) -> String {
    let mut outbounds = Vec::new();

    for p in proxies {
        let mut map = serde_json::Map::new();
        map.insert("tag".into(), serde_json::Value::String(p.name.clone()));
        map.insert("type".into(), serde_json::Value::String(p.node_type.to_lowercase()));
        map.insert("server".into(), serde_json::Value::String(p.server.clone()));
        map.insert("server_port".into(), serde_json::json!(p.port));

        if let Some(uuid) = &p.uuid {
            map.insert("uuid".into(), serde_json::Value::String(uuid.clone()));
        }
        if let Some(pass) = &p.password {
            map.insert("password".into(), serde_json::Value::String(pass.clone()));
        }
        if let Some(cipher) = &p.cipher {
            map.insert("method".into(), serde_json::Value::String(cipher.clone()));
        }

        if p.tls.unwrap_or(false) {
            let mut tls_map = serde_json::Map::new();
            tls_map.insert("enabled".into(), serde_json::Value::Bool(true));
            if let Some(sni) = &p.servername {
                tls_map.insert("server_name".into(), serde_json::Value::String(sni.clone()));
            }
            if p.skip_cert_verify.unwrap_or(false) {
                tls_map.insert("insecure".into(), serde_json::Value::Bool(true));
            }
            map.insert("tls".into(), serde_json::Value::Object(tls_map));
        }

        outbounds.push(serde_json::Value::Object(map));
    }

    // Default direct & block outbounds
    outbounds.push(serde_json::json!({ "type": "direct", "tag": "direct" }));
    outbounds.push(serde_json::json!({ "type": "block", "tag": "block" }));

    let config = serde_json::json!({
        "version": 1,
        "outbounds": outbounds
    });

    serde_json::to_string_pretty(&config).unwrap_or_default()
}

pub fn detect_client_target(ua: &str) -> &'static str {
    let u = ua.to_lowercase();
    if u.contains("sing-box") || u.contains("singbox") || u.contains("sfa") || u.contains("sfi") || u.contains("sfm") {
        "singbox"
    } else if u.contains("surge") {
        "surge"
    } else if u.contains("quantumult") || u.contains("quanx") {
        "quanx"
    } else if u.contains("shadowrocket") || u.contains("v2rayn") || u.contains("v2rayng") || u.contains("nekobox") || u.contains("karing") {
        "base64"
    } else {
        "clash"
    }
}
