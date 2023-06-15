mod interfaces;

use std::str::FromStr;

use anyhow::Context;

use crate::heseya::interfaces::HeseyaLoginResponse;

pub use self::interfaces::{ApiTokens, LoginDto, Request};

pub struct Sdk {
    api_url: String,
    max_retries: usize,
    client: reqwest::Client,
    tokens: Option<ApiTokens>,
}

impl Sdk {
    pub fn new(api_url: &str, user_agent: &str) -> Self {
        #[allow(clippy::expect_used)]
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .build()
            .expect("Failed to create Heysya SDK client ");

        Self {
            api_url: api_url.to_string(),
            client,
            tokens: None,
            max_retries: 3,
        }
    }

    pub fn set_tokens(&mut self, tokens: ApiTokens) {
        self.tokens = Some(tokens);
    }
}

pub async fn api_login(auth: &LoginDto, heseya_sdk: &Sdk) -> anyhow::Result<ApiTokens> {
    let login_url = format!("{}/login", heseya_sdk.api_url);

    let response = heseya_sdk
        .client
        .post(login_url)
        .json(auth)
        .send()
        .await
        .context("Failed to make login request")?;

    // let HeseyaLoginResponse { data: tokens } = response
    //     .json()
    //     .await
    //     .with_context(|| format!("Failed to parse login response\n{}", ))?;

    let body = response
        .text()
        .await
        .context("Failed to read login response body")?;

    let json = serde_json::Value::from_str(&body)
        .with_context(|| format!("Failed to parse login response\n{body:}"))?;

    let tokens = serde_json::from_value::<HeseyaLoginResponse>(json.clone())
        .with_context(|| format!("Failed to parse login response\n{json:#}"))?
        .data;

    Ok(tokens)
}

pub async fn make_api_request(
    api_request: &Request,
    heseya_sdk: &mut Sdk,
) -> anyhow::Result<reqwest::Response> {
    let method = &api_request.method;
    let url = format!("{}{}", heseya_sdk.api_url, api_request.url);

    let mut tokens = heseya_sdk.tokens.clone();

    if let Some(auth) = &api_request.auth {
        let result = api_login(auth, heseya_sdk)
            .await
            .context("Failed to login")?;

        tokens = Some(result);
    }

    let mut request_builder = heseya_sdk
        .client
        .request(method.clone().into(), url.as_str())
        .json(&api_request.body);

    if let Some(tokens) = tokens.clone() {
        request_builder = request_builder.bearer_auth(tokens.token);
    }

    let request = request_builder.build().context("Failed to build request")?;

    let mut response = retry_request(&heseya_sdk.client, &request, heseya_sdk.max_retries)
        .await
        .context("Failed to make request")?;

    if let Some(tokens) = tokens {
        if response.status() != reqwest::StatusCode::UNAUTHORIZED {
            return Ok(response);
        }

        println!("Token expired, refreshing token");
        let tokens = refresh_token(heseya_sdk, tokens)
            .await
            .context("Failed to refresh token")?;

        let request = heseya_sdk
            .client
            .request(method.clone().into(), url.as_str())
            .json(&api_request.body)
            .bearer_auth(&tokens.token)
            .build()
            .context("Failed to build request after refreshing token")?;

        response = retry_request(&heseya_sdk.client, &request, heseya_sdk.max_retries)
            .await
            .context("Failed to make request after refreshing token")?;
    }

    Ok(response)
}

async fn refresh_token(heseya_sdk: &mut Sdk, tokens: ApiTokens) -> anyhow::Result<ApiTokens> {
    let mut does_sdk_use_same_token = false;

    if let Some(sdk_tokens) = &heseya_sdk.tokens {
        does_sdk_use_same_token = sdk_tokens.refresh_token == tokens.refresh_token;
    }

    let body = serde_json::json!({
        "refresh_token": tokens.refresh_token,
    });

    let request = heseya_sdk
        .client
        .post(format!(
            "{api_url}/auth/refresh",
            api_url = heseya_sdk.api_url
        ))
        .json(&body)
        .build()
        .context("Failed to build refresh token request")?;

    let response = retry_request(&heseya_sdk.client, &request, heseya_sdk.max_retries)
        .await
        .context("Failed to execute refresh token request")?;

    let tokens = response
        .json::<ApiTokens>()
        .await
        .context("Failed to parse token response")?;

    if does_sdk_use_same_token {
        heseya_sdk.set_tokens(tokens.clone());
    }

    Ok(tokens)
}

async fn retry_request(
    client: &reqwest::Client,
    request: &reqwest::Request,
    max_retries: usize,
) -> anyhow::Result<reqwest::Response> {
    let retry_delay_seconds = 3;
    let mut retry_count = 0;

    loop {
        let request_attempt = request
            .try_clone()
            .ok_or_else(|| anyhow::anyhow!("Failed to clone request for retrying request"))?;

        let response = client.execute(request_attempt).await;

        let is_response_ok = response
            .as_ref()
            .map(|response| !response.status().is_server_error())
            .unwrap_or(false);

        if is_response_ok || retry_count >= max_retries {
            return response.context("Failed to execute request");
        }

        tokio::time::sleep(std::time::Duration::from_secs(retry_delay_seconds)).await;
        retry_count += 1;
        println!("Retrying request: {retry_count}");
    }
}
