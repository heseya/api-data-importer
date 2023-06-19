use std::collections::HashMap;

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct LoginDto {
    pub email: String,
    pub password: String,
    pub code: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct ApiTokens {
    pub token: String,
    pub identity_token: String,
    pub refresh_token: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub enum RequestMethod {
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

impl From<RequestMethod> for reqwest::Method {
    fn from(method: RequestMethod) -> Self {
        match method {
            RequestMethod::Get => Self::GET,
            RequestMethod::Post => Self::POST,
            RequestMethod::Put => Self::PUT,
            RequestMethod::Patch => Self::PATCH,
            RequestMethod::Delete => Self::DELETE,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct Request {
    pub method: RequestMethod,
    pub url: String,
    pub body: Option<serde_json::Value>,
    pub auth: Option<LoginDto>,
    pub files: Option<HashMap<String, String>>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize, Clone)]
pub struct Response<T> {
    pub data: T,
}
