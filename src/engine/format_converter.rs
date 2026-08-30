use crate::models::ProxyNode;
use base64::Engine;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use url::Url;

pub fn build_clash_yaml_from_nodes(nodes: &[ProxyNode], enable_udp: bool, _template: &str) -> String {
    let mut cleaned_proxies = Vec::new();
    let mut node_names = Vec::new();
    let mut auto_test_names = Vec::new();

    for p in nodes {
        node_names.push(p.name.clone());
        if !p.name.starts_with("📊") && !p.name.starts_with("⏰") && p.server != "127.0.0.1" {
            auto_test_names.push(p.name.clone());
        }
        if let Ok(val) = serde_json::to_value(p) {
            if let serde_json::Value::Object(mut map) = val {
                map.remove("extra");
                if enable_udp {
                    map.insert("udp".into(), serde_json::json!(true));
                }
                cleaned_proxies.push(serde_json::Value::Object(map));
            }
        }
    }

    let mut auto_test_proxies = auto_test_names.clone();
    if auto_test_proxies.is_empty() {
        auto_test_proxies.push("DIRECT".into());
    }

    let mut main_select_proxies = vec!["⚡ 自动优选".to_string(), "DIRECT".to_string()];
    main_select_proxies.extend(node_names.clone());
    main_select_proxies.dedup();

    let proxy_groups = serde_json::json!([
        {
            "name": "🚀 节点选择",
            "type": "select",
            "proxies": main_select_proxies
        },
        {
            "name": "⚡ 自动优选",
            "type": "url-test",
            "url": "http://www.gstatic.com/generate_204",
            "interval": 300,
            "tolerance": 50,
            "proxies": auto_test_proxies
        },
        {
            "name": "🤖 AI 专线",
            "type": "select",
            "proxies": ["🚀 节点选择", "⚡ 自动优选", "DIRECT"]
        },
        {
            "name": "🎬 国际流媒体",
            "type": "select",
            "proxies": ["🚀 节点选择", "⚡ 自动优选", "DIRECT"]
        },
        {
            "name": "🐟 漏网之鱼",
            "type": "select",
            "proxies": ["🚀 节点选择", "DIRECT"]
        }
    ]);

    let rules = serde_json::json!([
        "AND,((NETWORK,udp),(DST-PORT,443)),REJECT",
        "DOMAIN-SUFFIX,openai.com,🤖 AI 专线",
        "DOMAIN-SUFFIX,chatgpt.com,🤖 AI 专线",
        "DOMAIN-SUFFIX,claude.ai,🤖 AI 专线",
        "DOMAIN-SUFFIX,anthropic.com,🤖 AI 专线",
        "DOMAIN-SUFFIX,gemini.google.com,🤖 AI 专线",
        "DOMAIN-SUFFIX,youtube.com,🎬 国际流媒体",
        "DOMAIN-SUFFIX,googlevideo.com,🎬 国际流媒体",
        "DOMAIN-SUFFIX,netflix.com,🎬 国际流媒体",
        "DOMAIN-SUFFIX,disneyplus.com,🎬 国际流媒体",
        "DOMAIN-SUFFIX,spotify.com,🎬 国际流媒体",
        "GEOSITE,private,DIRECT",
        "GEOSITE,cn,DIRECT",
        "GEOIP,LAN,DIRECT,no-resolve",
        "GEOIP,CN,DIRECT,no-resolve",
        "GEOSITE,geolocation-!cn,🚀 节点选择",
        "MATCH,🐟 漏网之鱼"
    ]);

    let clash_obj = serde_json::json!({
        "mixed-port": 7890,
        "allow-lan": true,
        "mode": "rule",
        "log-level": "info",
        "ipv6": false,
        "tun": {
            "enable": true,
            "stack": "mixed",
            "dns-hijack": ["any:53", "tcp://any:53"],
            "auto-route": true,
            "auto-detect-interface": true,
            "strict-route": true
        },
        "dns": {
            "enable": true,
            "ipv6": false,
            "enhanced-mode": "fake-ip",
            "fake-ip-range": "198.18.0.1/16",
            "listen": "127.0.0.1:1053",
            "direct-nameserver": ["223.5.5.5", "119.29.29.29", "180.76.76.76"],
            "default-nameserver": ["223.5.5.5", "119.29.29.29", "180.76.76.76"],
            "nameserver": ["223.5.5.5", "119.29.29.29", "180.76.76.76", "https://223.5.5.5/dns-query", "https://1.12.12.12/dns-query"],
            "fallback": ["https://1.1.1.1/dns-query", "https://8.8.8.8/dns-query", "https://dns.google/dns-query"],
            "fallback-filter": { "geoip": true, "geoip-code": "CN", "ipcidr": ["240.0.0.0/4"] }
        },
        "proxies": cleaned_proxies,
        "proxy-groups": proxy_groups,
        "rules": rules
    });

    serde_yaml::to_string(&clash_obj).unwrap_or_else(|_| "proxies: []".into())
}

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

pub fn convert_to_raw_links(proxies: &[ProxyNode]) -> String {
    let mut uris = Vec::new();
    for p in proxies {
        if let Some(uri) = proxy_to_uri(p) {
            uris.push(uri);
        }
    }
    uris.join("\n")
}

pub fn convert_to_clash_proxies_only(proxies: &[ProxyNode]) -> String {
    let mut cleaned = Vec::new();
    for p in proxies {
        if let Ok(val) = serde_json::to_value(p) {
            if let serde_json::Value::Object(mut map) = val {
                map.remove("extra");
                cleaned.push(serde_json::Value::Object(map));
            }
        }
    }
    let obj = serde_json::json!({ "proxies": cleaned });
    serde_yaml::to_string(&obj).unwrap_or_else(|_| "proxies: []".into())
}

pub fn convert_to_surge_list(proxies: &[ProxyNode]) -> String {
    let mut lines = Vec::new();
    for p in proxies {
        match p.node_type.to_lowercase().as_str() {
            "ss" => {
                let cipher = p.cipher.as_deref().unwrap_or("aes-256-gcm");
                let pass = p.password.as_deref().unwrap_or("0");
                lines.push(format!("{} = ss, {}, {}, encrypt-method={}, password={}, udp-relay=true", p.name, p.server, p.port, cipher, pass));
            }
            "trojan" => {
                let pass = p.password.as_deref().unwrap_or_default();
                let sni = p.servername.as_deref().unwrap_or(&p.server);
                let mut params = vec![format!("password={}", pass), format!("sni={}", sni), "udp-relay=true".into()];
                if p.skip_cert_verify.unwrap_or(false) {
                    params.push("skip-cert-verify=true".into());
                }
                lines.push(format!("{} = trojan, {}, {}, {}", p.name, p.server, p.port, params.join(", ")));
            }
            "vmess" => {
                let uuid = p.uuid.as_deref().unwrap_or_default();
                let mut params = vec![format!("username={}", uuid), "udp-relay=true".into()];
                if p.network.as_deref().unwrap_or("tcp") == "ws" {
                    params.push("ws=true".into());
                    if let Some(wo) = &p.ws_opts {
                        if let Some(path) = wo.get("path").and_then(|v| v.as_str()) {
                            params.push(format!("ws-path={}", path));
                        }
                        if let Some(host) = wo.get("headers").and_then(|h| h.get("Host")).and_then(|v| v.as_str()) {
                            params.push(format!("ws-headers=Host:{}", host));
                        }
                    }
                }
                if p.tls.unwrap_or(false) {
                    params.push("tls=true".into());
                    if let Some(sni) = &p.servername {
                        params.push(format!("sni={}", sni));
                    }
                }
                lines.push(format!("{} = vmess, {}, {}, {}", p.name, p.server, p.port, params.join(", ")));
            }
            "vless" => {
                let uuid = p.uuid.as_deref().unwrap_or_default();
                let mut params = vec![format!("username={}", uuid), "udp-relay=true".into()];
                if p.tls.unwrap_or(false) {
                    params.push("tls=true".into());
                    if let Some(sni) = &p.servername {
                        params.push(format!("sni={}", sni));
                    }
                }
                lines.push(format!("{} = vless, {}, {}, {}", p.name, p.server, p.port, params.join(", ")));
            }
            "hysteria2" | "hy2" => {
                let pass = p.auth.as_deref().or(p.password.as_deref()).unwrap_or_default();
                let sni = p.servername.as_deref().unwrap_or(&p.server);
                lines.push(format!("{} = hysteria2, {}, {}, password={}, sni={}", p.name, p.server, p.port, pass, sni));
            }
            _ => {}
        }
    }
    lines.join("\n")
}

pub fn convert_to_surge_conf(proxies: &[ProxyNode]) -> String {
    let mut lines = Vec::new();
    lines.push("# Generated by SubHub Universal SubConverter".into());
    lines.push("[General]".into());
    lines.push("loglevel = notify".into());
    lines.push("dns-server = 223.5.5.5, 119.29.29.29".into());
    lines.push("skip-proxy = 127.0.0.1, 192.168.0.0/16, 10.0.0.0/8, 172.16.0.0/12, localhost, *.local".into());
    lines.push("".into());
    lines.push("[Proxy]".into());
    lines.push(convert_to_surge_list(proxies));
    lines.push("".into());
    lines.push("[Proxy Group]".into());
    let proxy_names: Vec<String> = proxies.iter().map(|p| p.name.clone()).collect();
    if !proxy_names.is_empty() {
        lines.push(format!("🚀 节点选择 = select, ⚡ 自动优选, DIRECT, {}", proxy_names.join(", ")));
        lines.push(format!("⚡ 自动优选 = url-test, {}, url=http://www.gstatic.com/generate_204, interval=300, tolerance=50", proxy_names.join(", ")));
    } else {
        lines.push("🚀 节点选择 = select, DIRECT".into());
    }
    lines.push("".into());
    lines.push("[Rule]".into());
    lines.push("DOMAIN-SUFFIX,cn,DIRECT".into());
    lines.push("GEOIP,CN,DIRECT".into());
    lines.push("FINAL,🚀 节点选择".into());
    lines.join("\n")
}

pub fn convert_to_loon_conf(proxies: &[ProxyNode]) -> String {
    let mut lines = Vec::new();
    lines.push("# Generated by SubHub Universal SubConverter".into());
    lines.push("[General]".into());
    lines.push("ipv6 = false".into());
    lines.push("dns-server = 223.5.5.5, 119.29.29.29".into());
    lines.push("".into());
    lines.push("[Proxy]".into());
    
    let mut proxy_names = Vec::new();
    for p in proxies {
        proxy_names.push(p.name.clone());
        match p.node_type.to_lowercase().as_str() {
            "ss" => {
                let cipher = p.cipher.as_deref().unwrap_or("aes-256-gcm");
                let pass = p.password.as_deref().unwrap_or("0");
                lines.push(format!("{} = Shadowsocks,{},{},{},\"{}\",fast-open=false,udp=true", p.name, p.server, p.port, cipher, pass));
            }
            "vmess" => {
                let uuid = p.uuid.as_deref().unwrap_or_default();
                let mut parts = vec![format!("{} = vmess,{},{},auto,\"{}\"", p.name, p.server, p.port, uuid)];
                if p.network.as_deref().unwrap_or("tcp") == "ws" {
                    parts.push("transport=ws".into());
                    if let Some(wo) = &p.ws_opts {
                        if let Some(path) = wo.get("path").and_then(|v| v.as_str()) {
                            parts.push(format!("path={}", path));
                        }
                        if let Some(host) = wo.get("headers").and_then(|h| h.get("Host")).and_then(|v| v.as_str()) {
                            parts.push(format!("host={}", host));
                        }
                    }
                }
                if p.tls.unwrap_or(false) {
                    parts.push("tls=true".into());
                    if let Some(sni) = &p.servername {
                        parts.push(format!("sni={}", sni));
                    }
                }
                lines.push(parts.join(","));
            }
            "vless" => {
                let uuid = p.uuid.as_deref().unwrap_or_default();
                let mut parts = vec![format!("{} = vless,{},{},\"{}\"", p.name, p.server, p.port, uuid)];
                if p.tls.unwrap_or(false) {
                    parts.push("tls=true".into());
                    if let Some(sni) = &p.servername {
                        parts.push(format!("sni={}", sni));
                    }
                }
                lines.push(parts.join(","));
            }
            "trojan" => {
                let pass = p.password.as_deref().unwrap_or_default();
                let sni = p.servername.as_deref().unwrap_or(&p.server);
                lines.push(format!("{} = trojan,{},{},\"{}\",tls=true,sni={}", p.name, p.server, p.port, pass, sni));
            }
            "hysteria2" | "hy2" => {
                let pass = p.auth.as_deref().or(p.password.as_deref()).unwrap_or_default();
                let sni = p.servername.as_deref().unwrap_or(&p.server);
                lines.push(format!("{} = Hysteria2,{},{},password=\"{}\",sni={}", p.name, p.server, p.port, pass, sni));
            }
            _ => {}
        }
    }

    lines.push("".into());
    lines.push("[Proxy Group]".into());
    if !proxy_names.is_empty() {
        lines.push(format!("🚀 节点选择 = select, ⚡ 自动优选, DIRECT, {}", proxy_names.join(", ")));
        lines.push(format!("⚡ 自动优选 = url-test, {}, url=http://www.gstatic.com/generate_204, interval=300, tolerance=50", proxy_names.join(", ")));
    } else {
        lines.push("🚀 节点选择 = select, DIRECT".into());
    }

    lines.push("".into());
    lines.push("[Rule]".into());
    lines.push("DOMAIN-SUFFIX,cn,DIRECT".into());
    lines.push("GEOIP,CN,DIRECT".into());
    lines.push("FINAL,🚀 节点选择".into());

    lines.join("\n")
}

pub fn convert_to_quanx_conf(proxies: &[ProxyNode]) -> String {
    let mut lines = Vec::new();
    lines.push("# Generated by SubHub Universal SubConverter".into());
    lines.push("[general]".into());
    lines.push("server_check_url=http://www.gstatic.com/generate_204".into());
    lines.push("".into());
    lines.push("[server_local]".into());

    let mut proxy_names = Vec::new();
    for p in proxies {
        proxy_names.push(p.name.clone());
        match p.node_type.to_lowercase().as_str() {
            "ss" => {
                let cipher = p.cipher.as_deref().unwrap_or("aes-256-gcm");
                let pass = p.password.as_deref().unwrap_or("0");
                lines.push(format!("shadowsocks={}:{}, method={}, password={}, tag={}", p.server, p.port, cipher, pass, p.name));
            }
            "vmess" => {
                let uuid = p.uuid.as_deref().unwrap_or_default();
                let mut parts = vec![
                    format!("vmess={}:{}", p.server, p.port),
                    "method=none".into(),
                    format!("password={}", uuid),
                ];
                if p.network.as_deref().unwrap_or("tcp") == "ws" {
                    parts.push("obfs=ws".into());
                    if let Some(wo) = &p.ws_opts {
                        if let Some(path) = wo.get("path").and_then(|v| v.as_str()) {
                            parts.push(format!("obfs-uri={}", path));
                        }
                        if let Some(host) = wo.get("headers").and_then(|h| h.get("Host")).and_then(|v| v.as_str()) {
                            parts.push(format!("obfs-host={}", host));
                        }
                    }
                }
                if p.tls.unwrap_or(false) {
                    parts.push("tls13=true".into());
                    if let Some(sni) = &p.servername {
                        parts.push(format!("tls-host={}", sni));
                    }
                }
                parts.push(format!("tag={}", p.name));
                lines.push(parts.join(", "));
            }
            "vless" => {
                let uuid = p.uuid.as_deref().unwrap_or_default();
                let mut parts = vec![
                    format!("vless={}:{}", p.server, p.port),
                    "method=none".into(),
                    format!("password={}", uuid),
                ];
                if p.tls.unwrap_or(false) {
                    parts.push("tls13=true".into());
                    if let Some(sni) = &p.servername {
                        parts.push(format!("tls-host={}", sni));
                    }
                }
                parts.push(format!("tag={}", p.name));
                lines.push(parts.join(", "));
            }
            "trojan" => {
                let pass = p.password.as_deref().unwrap_or_default();
                let sni = p.servername.as_deref().unwrap_or(&p.server);
                lines.push(format!("trojan={}:{}, password={}, over-tls=true, tls-host={}, tag={}", p.server, p.port, pass, sni, p.name));
            }
            "hysteria2" | "hy2" => {
                let pass = p.auth.as_deref().or(p.password.as_deref()).unwrap_or_default();
                let sni = p.servername.as_deref().unwrap_or(&p.server);
                lines.push(format!("hysteria2={}:{}, password={}, tls-host={}, tag={}", p.server, p.port, pass, sni, p.name));
            }
            _ => {}
        }
    }

    lines.push("".into());
    lines.push("[policy]".into());
    if !proxy_names.is_empty() {
        lines.push(format!("static=🚀 节点选择, direct, ⚡ 自动优选, {}, img-url=https://raw.githubusercontent.com/Koolson/Qure/master/IconSet/Color/Rocket.png", proxy_names.join(", ")));
        lines.push(format!("url-latency-benchmark=⚡ 自动优选, {}, check-interval=300, tolerance=50, img-url=https://raw.githubusercontent.com/Koolson/Qure/master/IconSet/Color/Auto.png", proxy_names.join(", ")));
    } else {
        lines.push("static=🚀 节点选择, direct".into());
    }

    lines.push("".into());
    lines.push("[filter_local]".into());
    lines.push("geoip, cn, direct".into());
    lines.push("final, 🚀 节点选择".into());

    lines.join("\n")
}

pub fn convert_to_singbox_json(proxies: &[ProxyNode]) -> String {
    let mut outbounds = Vec::new();
    let mut node_tags = Vec::new();
    let mut auto_test_tags = Vec::new();

    for p in proxies {
        let ntype = p.node_type.to_lowercase();
        let mut map = serde_json::Map::new();
        map.insert("tag".into(), serde_json::Value::String(p.name.clone()));
        map.insert("server".into(), serde_json::Value::String(p.server.clone()));
        map.insert("server_port".into(), serde_json::json!(p.port));

        match ntype.as_str() {
            "vless" => {
                map.insert("type".into(), serde_json::Value::String("vless".into()));
                if let Some(uuid) = &p.uuid {
                    map.insert("uuid".into(), serde_json::Value::String(uuid.clone()));
                }
                if let Some(flow) = &p.flow {
                    map.insert("flow".into(), serde_json::Value::String(flow.clone()));
                }
                let mut tls_map = serde_json::Map::new();
                tls_map.insert("enabled".into(), serde_json::Value::Bool(true));
                if let Some(sni) = &p.servername {
                    tls_map.insert("server_name".into(), serde_json::Value::String(sni.clone()));
                }
                if let Some(fp) = &p.client_fingerprint {
                    let mut utls_map = serde_json::Map::new();
                    utls_map.insert("enabled".into(), serde_json::Value::Bool(true));
                    utls_map.insert("fingerprint".into(), serde_json::Value::String(fp.clone()));
                    tls_map.insert("utls".into(), serde_json::Value::Object(utls_map));
                }
                if let Some(ro) = &p.reality_opts {
                    let mut reality_map = serde_json::Map::new();
                    reality_map.insert("enabled".into(), serde_json::Value::Bool(true));
                    if let Some(pbk) = ro.get("public-key").or_else(|| ro.get("public_key")).and_then(|v| v.as_str()) {
                        reality_map.insert("public_key".into(), serde_json::Value::String(pbk.to_string()));
                    }
                    if let Some(sid) = ro.get("short-id").or_else(|| ro.get("short_id")).and_then(|v| v.as_str()) {
                        reality_map.insert("short_id".into(), serde_json::Value::String(sid.to_string()));
                    }
                    tls_map.insert("reality".into(), serde_json::Value::Object(reality_map));
                }
                map.insert("tls".into(), serde_json::Value::Object(tls_map));
            }
            "trojan" => {
                map.insert("type".into(), serde_json::Value::String("trojan".into()));
                if let Some(pass) = &p.password {
                    map.insert("password".into(), serde_json::Value::String(pass.clone()));
                }
                let mut tls_map = serde_json::Map::new();
                tls_map.insert("enabled".into(), serde_json::Value::Bool(true));
                if let Some(sni) = &p.servername {
                    tls_map.insert("server_name".into(), serde_json::Value::String(sni.clone()));
                }
                map.insert("tls".into(), serde_json::Value::Object(tls_map));
            }
            "vmess" => {
                map.insert("type".into(), serde_json::Value::String("vmess".into()));
                if let Some(uuid) = &p.uuid {
                    map.insert("uuid".into(), serde_json::Value::String(uuid.clone()));
                }
                map.insert("security".into(), serde_json::Value::String(p.cipher.clone().unwrap_or("auto".into())));
                if p.tls.unwrap_or(false) {
                    let mut tls_map = serde_json::Map::new();
                    tls_map.insert("enabled".into(), serde_json::Value::Bool(true));
                    if let Some(sni) = &p.servername {
                        tls_map.insert("server_name".into(), serde_json::Value::String(sni.clone()));
                    }
                    map.insert("tls".into(), serde_json::Value::Object(tls_map));
                }
            }
            "ss" | "shadowsocks" => {
                map.insert("type".into(), serde_json::Value::String("shadowsocks".into()));
                map.insert("method".into(), serde_json::Value::String(p.cipher.clone().unwrap_or("aes-128-gcm".into())));
                map.insert("password".into(), serde_json::Value::String(p.password.clone().unwrap_or("0".into())));
            }
            "hysteria2" | "hy2" => {
                map.insert("type".into(), serde_json::Value::String("hysteria2".into()));
                if let Some(pass) = &p.password {
                    map.insert("password".into(), serde_json::Value::String(pass.clone()));
                }
                let mut tls_map = serde_json::Map::new();
                tls_map.insert("enabled".into(), serde_json::Value::Bool(true));
                if let Some(sni) = &p.servername {
                    tls_map.insert("server_name".into(), serde_json::Value::String(sni.clone()));
                }
                map.insert("tls".into(), serde_json::Value::Object(tls_map));
            }
            _ => {
                map.insert("type".into(), serde_json::Value::String(ntype.clone()));
                if let Some(pass) = &p.password { map.insert("password".into(), serde_json::Value::String(pass.clone())); }
            }
        }

        // WS transport
        if p.network.as_deref().unwrap_or("tcp") == "ws" {
            let mut trans_map = serde_json::Map::new();
            trans_map.insert("type".into(), serde_json::Value::String("ws".into()));
            if let Some(wo) = &p.ws_opts {
                if let Some(path) = wo.get("path").and_then(|v| v.as_str()) {
                    trans_map.insert("path".into(), serde_json::Value::String(path.to_string()));
                }
                if let Some(host) = wo.get("headers").and_then(|h| h.get("Host")).and_then(|v| v.as_str()) {
                    let mut h_map = serde_json::Map::new();
                    h_map.insert("Host".into(), serde_json::Value::String(host.to_string()));
                    trans_map.insert("headers".into(), serde_json::Value::Object(h_map));
                }
            }
            map.insert("transport".into(), serde_json::Value::Object(trans_map));
        }

        node_tags.push(p.name.clone());
        if !p.name.starts_with("📊") && !p.name.starts_with("⏰") && p.server != "127.0.0.1" {
            auto_test_tags.push(p.name.clone());
        }
        outbounds.push(serde_json::Value::Object(map));
    }

    // Selector outbound
    let mut selector_outbounds = vec!["⚡ 自动优选".to_string()];
    selector_outbounds.extend(node_tags);
    selector_outbounds.push("direct".to_string());
    selector_outbounds.dedup();

    let mut final_outbounds = vec![
        serde_json::json!({
            "type": "selector",
            "tag": "🚀 节点选择",
            "outbounds": selector_outbounds,
            "default": "⚡ 自动优选"
        }),
        serde_json::json!({
            "type": "urltest",
            "tag": "⚡ 自动优选",
            "outbounds": if auto_test_tags.is_empty() { vec!["direct".to_string()] } else { auto_test_tags },
            "url": "http://www.gstatic.com/generate_204",
            "interval": "300s",
            "tolerance": 50
        }),
    ];

    final_outbounds.extend(outbounds);
    final_outbounds.push(serde_json::json!({ "type": "direct", "tag": "direct" }));
    final_outbounds.push(serde_json::json!({ "type": "block", "tag": "block" }));
    final_outbounds.push(serde_json::json!({ "type": "dns", "tag": "dns-out" }));

    let config = serde_json::json!({
        "version": 1,
        "dns": {
            "servers": [
                { "tag": "dns-remote", "address": "https://1.1.1.1/dns-query", "detour": "🚀 节点选择" },
                { "tag": "dns-direct", "address": "223.5.5.5", "detour": "direct" },
                { "tag": "dns-block", "address": "rcode://success" }
            ],
            "rules": [
                { "outbound": "any", "server": "dns-direct" },
                { "geosite": "cn", "server": "dns-direct" }
            ],
            "strategy": "prefer_ipv4"
        },
        "outbounds": final_outbounds,
        "route": {
            "rules": [
                { "protocol": "dns", "outbound": "dns-out" },
                { "domain_suffix": ["wmxhub.com"], "outbound": "direct" },
                { "geosite": "category-ads-all", "outbound": "block" },
                { "geosite": "cn", "outbound": "direct" },
                { "geoip": "cn", "outbound": "direct" },
                { "geoip": "private", "outbound": "direct" }
            ],
            "auto_detect_interface": true,
            "final": "🚀 节点选择"
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_converters_multi_platform() {
        let node = ProxyNode {
            name: "US-Node".into(),
            node_type: "vless".into(),
            server: "1.2.3.4".into(),
            port: 443,
            uuid: Some("uuid-1234".into()),
            tls: Some(true),
            servername: Some("example.com".into()),
            client_fingerprint: Some("chrome".into()),
            reality_opts: Some(serde_json::json!({
                "public-key": "pbk123",
                "short-id": "sid123"
            })),
            ..Default::default()
        };

        let list = vec![node];

        // 1. Sing-box JSON test
        let sb_json = convert_to_singbox_json(&list);
        assert!(sb_json.contains("\"type\": \"vless\""));
        assert!(sb_json.contains("\"server_name\": \"example.com\""));
        assert!(sb_json.contains("\"public_key\": \"pbk123\""));
        assert!(sb_json.contains("\"tag\": \"🚀 节点选择\""));
        assert!(sb_json.contains("\"tag\": \"⚡ 自动优选\""));

        // 2. Base64 test
        let b64 = convert_to_base64(&list);
        assert!(!b64.is_empty());
        let decoded = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b64).unwrap()).unwrap();
        assert!(decoded.starts_with("vless://"));
        assert!(decoded.contains("security=reality"));

        // 3. User-Agent detection test
        assert_eq!(detect_client_target("Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) ClashMi/1.0"), "clash");
        assert_eq!(detect_client_target("ClashVerge/1.5.0 Windows NT 10.0"), "clash");
        assert_eq!(detect_client_target("SFA/1.8.0 (Android 14)"), "singbox");
        assert_eq!(detect_client_target("Surge/5.0 (macOS 14.0)"), "surge");
        assert_eq!(detect_client_target("Shadowrocket/2.2.0 (iOS 17.0)"), "base64");

        // 4. Clash full YAML test
        let clash_yaml = build_clash_yaml_from_nodes(&list, true, "default");
        assert!(clash_yaml.contains("mixed-port: 7890"));
        assert!(clash_yaml.contains("🚀 节点选择"));
        assert!(clash_yaml.contains("US-Node"));

        // 5. Clash Proxies only test
        let proxies_yaml = convert_to_clash_proxies_only(&list);
        assert!(proxies_yaml.starts_with("proxies:"));
        assert!(proxies_yaml.contains("US-Node"));

        // 6. Surge conf test
        let surge_conf = convert_to_surge_conf(&list);
        assert!(surge_conf.contains("[Proxy]"));
        assert!(surge_conf.contains("US-Node = vless, 1.2.3.4, 443"));

        // 7. Loon conf test
        let loon_conf = convert_to_loon_conf(&list);
        assert!(loon_conf.contains("[Proxy]"));
        assert!(loon_conf.contains("US-Node = vless,1.2.3.4,443"));

        // 8. Quantumult X test
        let qx_conf = convert_to_quanx_conf(&list);
        assert!(qx_conf.contains("[server_local]"));
        assert!(qx_conf.contains("vless=1.2.3.4:443"));

        // 9. Raw links test
        let raw = convert_to_raw_links(&list);
        assert!(raw.starts_with("vless://"));
    }
}

