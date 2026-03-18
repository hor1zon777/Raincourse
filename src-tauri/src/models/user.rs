use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub user_id: i64,
    pub name: String,
    pub school: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResult {
    pub user_id: i64,
    pub name: String,
    pub school: String,
    pub last_login_ip: String,
    pub auth: String,
}
