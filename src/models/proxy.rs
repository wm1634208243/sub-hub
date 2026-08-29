use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyNode {
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub server: String,
    pub port: u16,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub cipher: Option<String>,
    #[serde(default)]
    pub alter_id: Option<u32>,
    #[serde(default)]
    pub network: Option<String>,
    #[serde(default)]
    pub tls: Option<bool>,
    #[serde(default)]
    pub udp: Option<bool>,
    #[serde(default)]
    pub servername: Option<String>,
    #[serde(default)]
    pub sni: Option<String>,
    #[serde(default)]
    pub alpn: Option<Vec<String>>,
    #[serde(default)]
    pub skip_cert_verify: Option<bool>,
    #[serde(default)]
    pub client_fingerprint: Option<String>,
    #[serde(default)]
    pub ws_opts: Option<serde_json::Value>,
    #[serde(default)]
    pub grpc_opts: Option<serde_json::Value>,
    #[serde(default)]
    pub reality_opts: Option<serde_json::Value>,
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default)]
    pub insecure: Option<bool>,
    // Internal metadata
    #[serde(skip)]
    pub default_region: Option<String>,
    #[serde(skip)]
    pub raw_name: Option<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}
