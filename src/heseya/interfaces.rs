#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ApiTokens {
    pub token: String,
    pub identity_token: String,
    pub refresh_token: String,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct LoginDto {
    pub email: String,
    pub password: String,
    pub code: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct HeseyaLoginResponse {
    pub data: ApiTokens,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub enum HeseyaRequestMethod {
    #[serde(alias = "GET", alias = "get")]
    Get,
    #[serde(alias = "POST", alias = "post")]
    Post,
    #[serde(alias = "PUT", alias = "put")]
    Put,
    #[serde(alias = "PATCH", alias = "patch")]
    Patch,
    #[serde(alias = "DELETE", alias = "delete")]
    Delete,
}

impl From<HeseyaRequestMethod> for reqwest::Method {
    fn from(method: HeseyaRequestMethod) -> Self {
        match method {
            HeseyaRequestMethod::Get => Self::GET,
            HeseyaRequestMethod::Post => Self::POST,
            HeseyaRequestMethod::Put => Self::PUT,
            HeseyaRequestMethod::Patch => Self::PATCH,
            HeseyaRequestMethod::Delete => Self::DELETE,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct Request {
    pub method: HeseyaRequestMethod,
    pub url: String,
    pub body: Option<serde_json::Value>,
    pub auth: Option<LoginDto>,
}
