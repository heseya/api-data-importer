#![warn(
    clippy::pedantic,
    clippy::nursery,
    clippy::style,
    clippy::unwrap_used,
    clippy::expect_used
)]

mod heseya;
mod importer;

use anyhow::Context;
use heseya::{LoginDto, Sdk};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().context("Failed to load .env file")?;
    let email = dotenvy::var("API_EMAIL").context("Failed to get api email")?;
    let password = dotenvy::var("API_PASSWORD").context("Failed to get api password")?;
    let api_url = dotenvy::var("API_URL").context("Failed to get api url")?;

    let auth = LoginDto {
        email,
        password,
        code: None,
    };

    let mut heseya_sdk = Sdk::new(&api_url, "HeseyaImporter/0.1");

    let tokens = heseya::api_login(&auth, &heseya_sdk).await?;

    heseya_sdk.set_tokens(tokens);

    let file_list = importer::get_request_files()
        .await
        .context("Failed to get request files")?;
    importer::import_request_files(file_list, &mut heseya_sdk).await;

    Ok(())
}
