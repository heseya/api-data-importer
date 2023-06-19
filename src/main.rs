#![warn(
    clippy::pedantic,
    clippy::nursery,
    clippy::style,
    clippy::unwrap_used,
    clippy::expect_used
)]

mod heseya;
mod importer;

use std::sync::Arc;

use anyhow::Context;
use heseya::{get_tokens, make_client};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().context("Failed to load .env file")?;
    let email = dotenvy::var("API_EMAIL").context("Failed to get api email")?;
    let password = dotenvy::var("API_PASSWORD").context("Failed to get api password")?;
    let api_url = dotenvy::var("API_URL").context("Failed to get api url")?;

    let client = make_client("HeseyaImporter/0.1");
    let tokens = get_tokens(&api_url, &client, &email, &password).await?;
    let auth = Arc::new(Mutex::new(tokens));

    let file_list = importer::get_request_files()
        .await
        .context("Failed to get request files")?;
    importer::import_request_files(file_list, &api_url, &client, auth).await;

    Ok(())
}
