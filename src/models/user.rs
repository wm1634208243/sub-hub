use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub username: String,
    #[serde(alias = "password_hash", alias = "passwordHash")]
    pub password_hash: String,
    pub role: String, // "admin" or "user"
    #[serde(default, alias = "created_at", alias = "createdAt")]
    pub created_at: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default, alias = "disabled_until", alias = "disabledUntil")]
    pub disabled_until: Option<String>,
    #[serde(default, alias = "disabled_reason", alias = "disabledReason")]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicUserView {
    pub username: String,
    pub role: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub disabled_until: Option<String>,
    #[serde(default)]
    pub disabled_reason: Option<String>,
}
