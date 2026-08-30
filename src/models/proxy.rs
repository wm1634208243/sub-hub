use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyNode {
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub server: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cipher: Option<String>,
    #[serde(default, rename = "alterId", alias = "alter_id", alias = "alterId", skip_serializing_if = "Option::is_none")]
    pub alter_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udp: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub servername: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sni: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpn: Option<Vec<String>>,
    #[serde(default, rename = "skip-cert-verify", alias = "skip_cert_verify", alias = "skipCertVerify", skip_serializing_if = "Option::is_none")]
    pub skip_cert_verify: Option<bool>,
    #[serde(default, rename = "client-fingerprint", alias = "client_fingerprint", alias = "clientFingerprint", skip_serializing_if = "Option::is_none")]
    pub client_fingerprint: Option<String>,
    #[serde(default, rename = "ws-opts", alias = "ws_opts", alias = "wsOpts", skip_serializing_if = "Option::is_none")]
    pub ws_opts: Option<serde_json::Value>,
    #[serde(default, rename = "grpc-opts", alias = "grpc_opts", alias = "grpcOpts", skip_serializing_if = "Option::is_none")]
    pub grpc_opts: Option<serde_json::Value>,
    #[serde(default, rename = "reality-opts", alias = "reality_opts", alias = "realityOpts", skip_serializing_if = "Option::is_none")]
    pub reality_opts: Option<serde_json::Value>,
    #[serde(default, rename = "h2-opts", alias = "h2_opts", alias = "h2Opts", skip_serializing_if = "Option::is_none")]
    pub h2_opts: Option<serde_json::Value>,
    #[serde(default, rename = "http-opts", alias = "http_opts", alias = "httpOpts", skip_serializing_if = "Option::is_none")]
    pub http_opts: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insecure: Option<bool>,
    // Internal metadata
    #[serde(skip)]
    pub default_region: Option<String>,
    #[serde(skip)]
    pub raw_name: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Default for ProxyNode {
    fn default() -> Self {
        Self {
            name: "".into(),
            node_type: "ss".into(),
            server: "".into(),
            port: 443,
            uuid: None,
            password: None,
            cipher: None,
            alter_id: None,
            network: None,
            tls: None,
            udp: Some(true),
            servername: None,
            sni: None,
            alpn: None,
            skip_cert_verify: None,
            client_fingerprint: None,
            ws_opts: None,
            grpc_opts: None,
            reality_opts: None,
            h2_opts: None,
            http_opts: None,
            auth: None,
            flow: None,
            insecure: None,
            default_region: None,
            raw_name: None,
            extra: HashMap::new(),
        }
    }
}
