use crate::models::CustomRenameRule;
use lazy_static::lazy_static;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct RegionInfo {
    pub code: &'static str,
    pub name: &'static str,
    pub flag: &'static str,
    pub pattern: &'static str,
}

pub const REGION_FLAGS: &[RegionInfo] = &[
    RegionInfo { code: "HK", name: "香港", flag: "🇭🇰", pattern: r"(?i)(香港|HongKong|Hong Kong|HK|HKG|HKT|HKBN)" },
    RegionInfo { code: "TW", name: "台湾", flag: "🇹🇼", pattern: r"(?i)(台湾|臺灣|Taiwan|TW|TaiWan|TWN|Hinet)" },
    RegionInfo { code: "JP", name: "日本", flag: "🇯🇵", pattern: r"(?i)(日本|东京|大阪|埼玉|Japan|JP|Tokyo|Osaka|NRT|HND|KIX)" },
    RegionInfo { code: "SG", name: "新加坡", flag: "🇸🇬", pattern: r"(?i)(新加坡|狮城|Singapore|SG|SIN|SGP)" },
    RegionInfo { code: "US", name: "美国", flag: "🇺🇸", pattern: r"(?i)(美国|美國|洛杉矶|硅谷|西雅图|达拉斯|芝加哥|纽约|波特兰|United States|USA|US|LA|LAX|SJC|SEA|DFW|ORD|JFK|EWR|SFO|PHX)" },
    RegionInfo { code: "KR", name: "韩国", flag: "🇰🇷", pattern: r"(?i)(韩国|首尔|Korea|KR|KOR|Seoul|ICN)" },
    RegionInfo { code: "GB", name: "英国", flag: "🇬🇧", pattern: r"(?i)(英国|伦敦|United Kingdom|\bUK\b|London|LHR|\bGB\b)" },
    RegionInfo { code: "DE", name: "德国", flag: "🇩🇪", pattern: r"(?i)(德国|法兰克福|Germany|DE|Frankfurt|FRA)" },
    RegionInfo { code: "FR", name: "法国", flag: "🇫🇷", pattern: r"(?i)(法国|巴黎|France|FR|Paris|CDG)" },
    RegionInfo { code: "CA", name: "加拿大", flag: "🇨🇦", pattern: r"(?i)(加拿大|温哥华|多伦多|Canada|CA|Vancouver|Toronto|YVR|YYZ)" },
    RegionInfo { code: "AU", name: "澳大利亚", flag: "🇦🇺", pattern: r"(?i)(澳大利亚|澳洲|悉尼|墨尔本|Australia|AU|Sydney|Melbourne|SYD|MEL)" },
    RegionInfo { code: "RU", name: "俄罗斯", flag: "🇷🇺", pattern: r"(?i)(俄罗斯|莫斯科|Russia|RU|Moscow|SVO|DME)" },
    RegionInfo { code: "IN", name: "印度", flag: "🇮🇳", pattern: r"(?i)(印度|孟买|India|IN|Mumbai|BOM|DEL)" },
    RegionInfo { code: "TH", name: "泰国", flag: "🇹🇭", pattern: r"(?i)(泰国|曼谷|Thailand|TH|Bangkok|BKK)" },
    RegionInfo { code: "VN", name: "越南", flag: "🇻🇳", pattern: r"(?i)(越南|河内|胡志明|Vietnam|VN|Hanoi|SGN)" },
    RegionInfo { code: "MY", name: "马来西亚", flag: "🇲🇾", pattern: r"(?i)(马来西亚|大马|吉隆坡|Malaysia|MY|Kuala Lumpur|KUL)" },
    RegionInfo { code: "PH", name: "菲律宾", flag: "🇵🇭", pattern: r"(?i)(菲律宾|马尼拉|Philippines|PH|Manila|MNL)" },
    RegionInfo { code: "TR", name: "土耳其", flag: "🇹🇷", pattern: r"(?i)(土耳其|伊斯坦布尔|Turkey|TR|Istanbul|IST)" },
    RegionInfo { code: "AR", name: "阿根廷", flag: "🇦🇷", pattern: r"(?i)(阿根廷|布宜诺斯艾利斯|Argentina|AR|Buenos Aires|EZE)" },
    RegionInfo { code: "BR", name: "巴西", flag: "🇧🇷", pattern: r"(?i)(巴西|圣保罗|Brazil|BR|Sao Paulo|GRU)" },
    RegionInfo { code: "NL", name: "荷兰", flag: "🇳🇱", pattern: r"(?i)(荷兰|阿姆斯特丹|Netherlands|NL|Amsterdam|AMS)" },
    RegionInfo { code: "CH", name: "瑞士", flag: "🇨🇭", pattern: r"(?i)(瑞士|苏黎世|Switzerland|CH|Zurich|ZRH)" },
    RegionInfo { code: "SE", name: "瑞典", flag: "🇸🇪", pattern: r"(?i)(瑞典|斯德哥尔摩|Sweden|SE|Stockholm|ARN)" },
    RegionInfo { code: "IT", name: "意大利", flag: "🇮🇹", pattern: r"(?i)(意大利|米兰|罗马|Italy|IT|Milan|Rome|MXP|FCO)" },
    RegionInfo { code: "ES", name: "西班牙", flag: "🇪🇸", pattern: r"(?i)(西班牙|马德里|Spain|ES|Madrid|MAD)" },
    RegionInfo { code: "AE", name: "阿联酋", flag: "🇦🇪", pattern: r"(?i)(阿联酋|迪拜|UAE|Dubai|DXB)" },
];

lazy_static! {
    static ref REGION_REGEXES: Vec<(RegionInfo, Regex)> = {
        REGION_FLAGS
            .iter()
            .map(|r| (r.clone(), Regex::new(r.pattern).unwrap()))
            .collect()
    };
    static ref TRAFFIC_SUFFIX_REGEX: Regex = Regex::new(r"(?i)-\d+(?:\.\d+)?\s*(?:KB|MB|GB|TB|PB|K|M|G|T)(?:-∞)?").unwrap();
    static ref AD_MULTIPLIER_REGEX: Regex = Regex::new(r"(?i)(?:\([0-9.]+x\)|\[[0-9.]+倍率?\]|-[0-9.]+x|\b[0-9.]+x\b)").unwrap();
    static ref DOMAIN_PROMO_REGEX: Regex = Regex::new(r"(?i)(?:https?://)?(?:www\.)?[a-zA-Z0-9-]+\.(?:com|xyz|net|org|top|vip|club|me|cc|io|cn|site|info|link|icu)(?:/[^\s]*)?").unwrap();
    static ref DUMMY_ANNOUNCEMENT_REGEX: Regex = Regex::new(r"(?i)^(?:剩余流量|已用流量|距离重置|套餐到期|到期时间|官网地址|官方网站|最新地址|通知公告|客服群组|使用说明|重要提示|套餐|TB|GB|MB|重置|剩余|到期|通知|公告|说明|提示)[\s:：0-9a-zA-Z._\-–—∞%]*$").unwrap();
    static ref ALL_FLAGS_REGEX: Regex = Regex::new(r"[\u{1F1E6}-\u{1F1FF}]{2}").unwrap();
}

pub fn detect_node_primary_region(name: &str, server: &str, default_region: Option<&str>) -> Option<RegionInfo> {
    if let Some(def) = default_region {
        let code = def.trim().to_uppercase();
        if !code.is_empty() {
            if let Some(r) = REGION_FLAGS.iter().find(|r| r.code == code) {
                return Some(r.clone());
            }
        }
    }

    // 1. Clean name from traffic suffixes
    let clean_name = TRAFFIC_SUFFIX_REGEX.replace_all(name, "");

    // 2. Pattern matching in node name
    for (info, reg) in REGION_REGEXES.iter() {
        if reg.is_match(&clean_name) {
            return Some(info.clone());
        }
    }

    // 3. Pattern matching in server hostname
    let srv_lower = server.to_lowercase();
    for (info, reg) in REGION_REGEXES.iter() {
        if reg.is_match(&srv_lower) {
            return Some(info.clone());
        }
    }

    None
}

pub fn format_node_name(
    raw_name: &str,
    server: &str,
    enable_auto_flags: bool,
    enable_clean_ad_and_rate: bool,
    custom_rename_rules: &[CustomRenameRule],
    default_region: Option<&str>,
) -> String {
    let mut name = raw_name.trim().to_string();

    // 1. Clean ads and multipliers
    if enable_clean_ad_and_rate {
        name = TRAFFIC_SUFFIX_REGEX.replace_all(&name, "").to_string();
        name = AD_MULTIPLIER_REGEX.replace_all(&name, "").to_string();
        name = DOMAIN_PROMO_REGEX.replace_all(&name, "").to_string();
        name = name.trim().to_string();
    }

    // 2. Apply custom rename rules
    for rule in custom_rename_rules {
        if !rule.enabled || rule.search.is_empty() {
            continue;
        }
        if rule.is_regex {
            if let Ok(re) = Regex::new(&rule.search) {
                name = re.replace_all(&name, &rule.replace).to_string();
            }
        } else {
            name = name.replace(&rule.search, &rule.replace);
        }
    }

    // 3. Inject Auto Flags
    if enable_auto_flags {
        let region = detect_node_primary_region(&name, server, default_region);
        if let Some(reg) = region {
            // Remove existing flags first to prevent duplicate flags
            name = ALL_FLAGS_REGEX.replace_all(&name, "").trim().to_string();
            name = format!("{} {}", reg.flag, name);
        }
    }

    name.trim().to_string()
}

pub fn is_announcement_node(name: &str, server: &str, port: u16) -> bool {
    let srv = server.trim().to_lowercase();
    if ["127.0.0.1", "0.0.0.0", "localhost", "::1", "null", "example.com"].contains(&srv.as_str()) || port == 0 {
        return true;
    }

    let clean = name.trim();
    // Strip leading [prefix]
    let stripped = if let Some(idx) = clean.find(']') {
        if clean.starts_with('[') { clean[idx + 1..].trim() } else { clean }
    } else {
        clean
    };

    let keywords = [
        "剩余流量", "已用流量", "距离重置", "套餐到期", "到期时间", "官方网站", "官网地址", "最新地址",
        "通知公告", "客服群组", "使用说明", "重要提示", "套餐重置", "账号到期", "流量剩余", "官网",
        "发布页", "重置时间", "重置日", "公告", "通知", "维护中", "说明", "套餐", "客服", "群组",
        "剩余", "到期", "重置", "提示",
    ];

    for kw in keywords {
        if stripped.contains(kw) {
            if stripped.contains("剩余流量")
                || stripped.contains("已用流量")
                || stripped.contains("到期时间")
                || stripped.contains("距离重置")
                || stripped.contains("官方网站")
                || stripped.contains("官网")
                || stripped.contains("通知")
                || stripped.contains("公告")
                || stripped.contains("套餐")
                || stripped == "TB"
                || stripped == "GB"
                || stripped == "MB"
            {
                return true;
            }
        }
    }

    let stripped_symbols = stripped.replace(&['-', '_', '–', '—', ' ', ':', '：', '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', '.', '∞', '%'][..], "").to_lowercase();
    if ["tb", "gb", "mb", "kb", "套餐", "重置", "剩余", "到期", "通知", "公告", "官网", "提示", "说明", "群组", "客服"].contains(&stripped_symbols.as_str()) {
        return true;
    }

    false
}
