use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use reqwest::{Body, Client};
use tokio::sync::Mutex;

pub use self::interfaces::{ApiTokens, Request, RequestMethod, Response};

mod interfaces;

pub fn make_client(user_agent: &str) -> Client {
    #[allow(clippy::expect_used)]
    reqwest::Client::builder()
        .user_agent(user_agent)
        .build()
        .expect("Failed to create Heysya SDK client ")
}

pub async fn make_request_retry_with_auth(
    api_url: &str,
    request: &Request,
    client: &Client,
    auth: Arc<Mutex<ApiTokens>>,
) -> anyhow::Result<reqwest::Response> {
    let (tokens, should_update_auth) = match &request.auth {
        Some(login_dto) => {
            let tokens = get_tokens(api_url, client, &login_dto.email, &login_dto.password).await?;
            (tokens, false)
        }
        None => {
            let tokens = auth.lock().await.clone();
            (tokens, true)
        }
    };

    let response = make_request_retry(api_url, request, client, Some(&tokens.token)).await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let new_token = refresh_tokens(api_url, client, tokens).await?;

        if should_update_auth {
            let mut auth = auth.lock().await;
            *auth = new_token.clone();
        }

        let response = make_request_retry(api_url, request, client, Some(&new_token.token)).await?;

        return Ok(response);
    }

    Ok(response)
}

pub async fn make_request_retry(
    api_url: &str,
    request: &Request,
    client: &Client,
    token: Option<&str>,
) -> anyhow::Result<reqwest::Response> {
    let retry_delay_seconds = 3;
    let mut retry_count = 0;

    loop {
        let response = make_request(api_url, request, client, token).await;

        match response {
            Ok(response) => return Ok(response),
            Err(err) => {
                if retry_count >= 3 {
                    return Err(err);
                }

                retry_count += 1;
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay_seconds)).await;
            }
        }
    }
}

pub async fn make_request(
    api_url: &str,
    request: &Request,
    client: &Client,
    token: Option<&str>,
) -> anyhow::Result<reqwest::Response> {
    let full_url = format!("{}{}", api_url, request.url);
    let method = request.method.clone();

    let mut builder = client.request(method.into(), full_url);

    if let Some(files) = &request.files {
        let json = serde_json::to_value(&request.body).context("Failed to serialize json")?;
        let form = make_request_body_files(files, &json, client).await?;
        builder = builder.multipart(form);
    } else {
        builder = builder.json(&request.body);
    }

    if let Some(token) = token {
        builder = builder.bearer_auth(token);
    }

    let response = builder.send().await.context("Failed to make request")?;

    Ok(response)
}

async fn make_request_body_files(
    files: &HashMap<String, String>,
    json: &serde_json::Value,
    client: &Client,
) -> anyhow::Result<reqwest::multipart::Form> {
    let mut form = reqwest::multipart::Form::new();

    for (name, url) in files {
        let bytes = download_file(url, client).await?;
        let body = Body::from(bytes);

        // naive implementation
        let file_name = url.split('/').last().context("Failed to get file name")?;

        let file = reqwest::multipart::Part::stream(body).file_name(file_name.to_string());
        form = form.part(name.to_string(), file);
    }

    let json = serde_json::to_value(json).context("Failed to serialize json")?;

    for (name, value) in json.as_object().context("Failed to get json object")? {
        match value {
            serde_json::Value::String(value) => {
                form = form.text(name.to_string(), value.to_string());
            }
            serde_json::Value::Number(value) => {
                form = form.text(name.to_string(), value.to_string());
            }
            serde_json::Value::Bool(value) => {
                form = form.text(name.to_string(), value.to_string());
            }
            serde_json::Value::Null => (),
            _ => {
                return Err(anyhow::anyhow!("Unsupported json value type"));
            }
        }
    }

    Ok(form)
}

async fn download_file(url: &str, client: &Client) -> anyhow::Result<Vec<u8>> {
    let bytes = client
        .get(url)
        .send()
        .await
        .context("Failed to download file")?
        .bytes()
        .await
        .context("Failed to read file bytes")?;

    Ok(bytes.to_vec())
}

async fn refresh_tokens(
    api_url: &str,
    client: &Client,
    auth: ApiTokens,
) -> anyhow::Result<ApiTokens> {
    let request = Request {
        url: "/auth/refresh".to_string(),
        method: RequestMethod::Post,
        body: Some(serde_json::json!({ "refresh_token": auth.refresh_token })),
        files: None,
        auth: None,
    };

    let response = make_request_retry(api_url, &request, client, None).await?;

    let response = response
        .json::<Response<ApiTokens>>()
        .await
        .context("Failed to parse refresh token response")?;

    Ok(response.data)
}

pub async fn get_tokens(
    api_url: &str,
    client: &Client,
    email: &str,
    password: &str,
) -> anyhow::Result<ApiTokens> {
    let request = Request {
        url: "/login".to_string(),
        method: RequestMethod::Post,
        body: Some(serde_json::json!({ "email": email, "password": password })),
        files: None,
        auth: None,
    };

    let response = make_request_retry(api_url, &request, client, None).await?;

    let response = response
        .json::<Response<ApiTokens>>()
        .await
        .context("Failed to parse login response")?;

    Ok(response.data)
}
