use crate::models::UserConfig;

pub fn compile_config_to_js(cfg: &UserConfig) -> String {
    let direct_domains = serde_json::to_string(&cfg.direct_domains).unwrap_or_else(|_| "[]".into());
    let proxy_domains = serde_json::to_string(&cfg.proxy_domains).unwrap_or_else(|_| "[]".into());
    let direct_ips = serde_json::to_string(&cfg.direct_ips).unwrap_or_else(|_| "[]".into());
    let proxy_ips = serde_json::to_string(&cfg.proxy_ips).unwrap_or_else(|_| "[]".into());
    let direct_keywords = serde_json::to_string(&cfg.direct_keywords).unwrap_or_else(|_| "[]".into());
    let proxy_keywords = serde_json::to_string(&cfg.proxy_keywords).unwrap_or_else(|_| "[]".into());
    let direct_processes = serde_json::to_string(&cfg.direct_processes).unwrap_or_else(|_| "[]".into());
    let proxy_processes = serde_json::to_string(&cfg.proxy_processes).unwrap_or_else(|_| "[]".into());

    format!(r#"// =========================================================================
// SubHub (Clash Sub Hub) - JavaScript Override Script (Rust Engine Generated)
// =========================================================================

function main(config, profileName) {{
  config = config || {{}};
  config.dns = config.dns || {{}};
  config.dns.enable = true;
  config.dns['enhanced-mode'] = 'fake-ip';

  const directDomains = {direct_domains};
  const proxyDomains = {proxy_domains};
  const directIps = {direct_ips};
  const proxyIps = {proxy_ips};
  const directKeywords = {direct_keywords};
  const proxyKeywords = {proxy_keywords};
  const directProcesses = {direct_processes};
  const proxyProcesses = {proxy_processes};

  const customRules = [];

  for (const kw of directKeywords) customRules.push(`DOMAIN-KEYWORD,${{kw}},DIRECT`);
  for (const kw of proxyKeywords) customRules.push(`DOMAIN-KEYWORD,${{kw}},🚀 节点选择`);
  for (const d of directDomains) customRules.push(`DOMAIN-SUFFIX,${{d}},DIRECT`);
  for (const d of proxyDomains) customRules.push(`DOMAIN-SUFFIX,${{d}},🚀 节点选择`);
  for (const ip of directIps) customRules.push(`IP-CIDR,${{ip}},DIRECT,no-resolve`);
  for (const ip of proxyIps) customRules.push(`IP-CIDR,${{ip}},🚀 节点选择,no-resolve`);
  for (const p of directProcesses) customRules.push(`PROCESS-NAME,${{p}},DIRECT`);
  for (const p of proxyProcesses) customRules.push(`PROCESS-NAME,${{p}},🚀 节点选择`);

  config.rules = [...customRules, ...(config.rules || [])];
  return config;
}}
"#)
}
