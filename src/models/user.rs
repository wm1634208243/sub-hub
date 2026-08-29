use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub password_hash: String,
    pub role: String, // "admin" or "user"
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub disabled: Option<bool>,
    #[serde(default)]
    pub disabled_until: Option<String>,
    #[serde(default)]
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
