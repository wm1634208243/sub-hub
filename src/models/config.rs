use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionItem {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub default_region: Option<String>,
    #[serde(default)]
    pub custom_expire: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_60")]
    pub auto_refresh_interval: u32,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub nodes_count: Option<usize>,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub user_info: Option<serde_json::Value>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

fn default_true() -> bool { true }
fn default_60() -> u32 { 60 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRenameRule {
    pub search: String,
    pub replace: String,
    #[serde(default)]
    pub is_regex: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserConfig {
    #[serde(default = "default_token")]
    pub subscription_token: String,
    #[serde(default)]
    pub token_expires_at: Option<String>,
    #[serde(default = "default_gui")]
    pub mode: String,
    #[serde(default = "default_direct")]
    pub fallback_rule: String,
    #[serde(default = "default_true")]
    pub enable_geo_site_cn: bool,
    #[serde(default = "default_true")]
    pub enable_geo_ip_cn: bool,
    #[serde(default = "default_true")]
    pub enable_sniffer: bool,
    #[serde(default = "default_true")]
    pub enable_tcp_concurrent: bool,
    #[serde(default = "default_true")]
    pub enable_no_resolve: bool,
    #[serde(default = "default_true")]
    pub enable_unified_delay: bool,
    #[serde(default = "default_true")]
    pub enable_process_strict: bool,
    #[serde(default)]
    pub custom_proxy_group_name: Option<String>,
    #[serde(default = "default_true")]
    pub enable_ai_group: bool,
    #[serde(default = "default_true")]
    pub enable_media_group: bool,
    #[serde(default = "default_true")]
    pub enable_telegram_group: bool,
    #[serde(default = "default_true")]
    pub enable_game_group: bool,
    #[serde(default = "default_true")]
    pub enable_apple_group: bool,
    #[serde(default = "default_true")]
    pub enable_ad_block: bool,
    #[serde(default = "default_true")]
    pub enable_final_group: bool,
    #[serde(default = "default_true")]
    pub enable_loyalsoldier: bool,
    #[serde(default = "default_platforms")]
    pub target_platforms: Vec<String>,
    #[serde(default = "default_true")]
    pub enable_auto_platform_detect: bool,
    #[serde(default)]
    pub subscriptions: Vec<SubscriptionItem>,
    #[serde(default = "default_true")]
    pub enable_auto_flags: bool,
    #[serde(default = "default_true")]
    pub enable_clean_ad_and_rate: bool,
    #[serde(default = "default_true")]
    pub enable_geo_ip_lookup: bool,
    #[serde(default)]
    pub enable_dead_node_filter: bool,
    #[serde(default)]
    pub enable_latency_sort: bool,
    #[serde(default = "default_2000")]
    pub latency_timeout_ms: u32,
    #[serde(default)]
    pub custom_rename_rules: Vec<CustomRenameRule>,
    // URLTest Candidate Pool Settings
    #[serde(default = "default_all")]
    pub auto_test_scope: String,
    #[serde(default = "default_regions")]
    pub auto_test_regions: Vec<String>,
    #[serde(default)]
    pub auto_test_include_keywords: Option<String>,
    #[serde(default)]
    pub auto_test_exclude_keywords: Option<String>,
    #[serde(default)]
    pub excluded_auto_test_nodes: Vec<String>,
    #[serde(default = "default_nameservers")]
    pub nameservers: Vec<String>,
    #[serde(default = "default_fallback_dns")]
    pub fallback_dns: Vec<String>,
    #[serde(default)]
    pub proxy_ips: Vec<String>,
    #[serde(default)]
    pub proxy_processes: Vec<String>,
    #[serde(default)]
    pub proxy_keywords: Vec<String>,
    #[serde(default)]
    pub proxy_domains: Vec<String>,
    #[serde(default)]
    pub direct_ips: Vec<String>,
    #[serde(default)]
    pub direct_processes: Vec<String>,
    #[serde(default)]
    pub direct_keywords: Vec<String>,
    #[serde(default)]
    pub direct_domains: Vec<String>,
    #[serde(default)]
    pub fake_ip_filter: Vec<String>,
    #[serde(default)]
    pub custom_script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettings {
    #[serde(default = "default_server_port")]
    pub server_port: u16,
    #[serde(default = "default_allow_registration")]
    pub allow_registration: bool,
    #[serde(default)]
    pub custom_domain: String,
    #[serde(default)]
    pub enable_https_redirect: bool,
    #[serde(default = "default_runtime")]
    pub runtime: String,
}

fn default_server_port() -> u16 { 3000 }
fn default_allow_registration() -> bool { true }
fn default_runtime() -> String { "Rust (Tokio + Axum) High Performance Single Binary Engine".into() }

impl Default for SystemSettings {
    fn default() -> Self {
        Self {
            server_port: 3000,
            allow_registration: true,
            custom_domain: "".into(),
            enable_https_redirect: false,
            runtime: default_runtime(),
        }
    }
}

pub fn default_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
fn default_gui() -> String { "gui".to_string() }
fn default_direct() -> String { "DIRECT".to_string() }
fn default_all() -> String { "all".to_string() }
fn default_platforms() -> Vec<String> {
    vec!["macos".into(), "windows".into(), "ios".into(), "android".into()]
}
fn default_2000() -> u32 { 2000 }
fn default_regions() -> Vec<String> {
    vec!["HK".into(), "JP".into(), "SG".into(), "US".into()]
}
fn default_nameservers() -> Vec<String> {
    vec!["223.5.5.5".into(), "119.29.29.29".into()]
}
fn default_fallback_dns() -> Vec<String> {
    vec!["https://1.1.1.1/dns-query".into(), "https://8.8.8.8/dns-query".into()]
}

impl Default for UserConfig {
    fn default() -> Self {
        UserConfig {
            subscription_token: default_token(),
            token_expires_at: None,
            mode: default_gui(),
            fallback_rule: default_direct(),
            enable_geo_site_cn: true,
            enable_geo_ip_cn: true,
            enable_sniffer: true,
            enable_tcp_concurrent: true,
            enable_no_resolve: true,
            enable_unified_delay: true,
            enable_process_strict: true,
            custom_proxy_group_name: None,
            enable_ai_group: true,
            enable_media_group: true,
            enable_telegram_group: true,
            enable_game_group: true,
            enable_apple_group: true,
            enable_ad_block: true,
            enable_final_group: true,
            enable_loyalsoldier: true,
            target_platforms: default_platforms(),
            enable_auto_platform_detect: true,
            subscriptions: vec![],
            enable_auto_flags: true,
            enable_clean_ad_and_rate: true,
            enable_geo_ip_lookup: true,
            enable_dead_node_filter: false,
            enable_latency_sort: false,
            latency_timeout_ms: 2000,
            custom_rename_rules: vec![],
            auto_test_scope: default_all(),
            auto_test_regions: default_regions(),
            auto_test_include_keywords: None,
            auto_test_exclude_keywords: None,
            excluded_auto_test_nodes: vec![],
            nameservers: default_nameservers(),
            fallback_dns: default_fallback_dns(),
            proxy_ips: vec![],
            proxy_processes: vec![],
            proxy_keywords: vec![],
            proxy_domains: vec![],
            direct_ips: vec![],
            direct_processes: vec![],
            direct_keywords: vec![],
            direct_domains: vec![],
            fake_ip_filter: vec![],
            custom_script: None,
        }
    }
}
