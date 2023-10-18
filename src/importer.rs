use std::sync::Arc;

use anyhow::Context;
use reqwest::{Client, Response};
use tokio::{
    fs::DirEntry,
    sync::{Mutex, Semaphore},
};

use crate::heseya::{self, ApiTokens, Request};

pub async fn get_request_files() -> anyhow::Result<Vec<DirEntry>> {
    let mut directory = tokio::fs::read_dir("./requests")
        .await
        .context("Failed to read requests directory")?;

    let mut file_list: Vec<DirEntry> = Vec::new();

    while let Some(file) = directory
        .next_entry()
        .await
        .context("Failed to read next entry")?
    {
        let is_file = file.file_type().await?.is_file();
        let is_json = file.path().extension().map_or(false, |ext| ext == "json");

        if is_file && is_json {
            file_list.push(file);
        }
    }

    file_list.sort_by_key(tokio::fs::DirEntry::path);

    Ok(file_list)
}

type FileIdentifier = (usize, String);
type RequestIdentifier = (usize, Request);
type IncompleteFile = (FileIdentifier, Vec<RequestIdentifier>);

pub async fn import_request_files(
    file_list: Vec<DirEntry>,
    api_url: &str,
    client: &Client,
    auth: Arc<Mutex<ApiTokens>>,
    semaphore_permits: usize,
) {
    let api_url: Arc<str> = Arc::from(api_url);
    let mut failed_files: Vec<FileIdentifier> = Vec::new();
    let mut incomplete_files: Vec<IncompleteFile> = Vec::new();

    for (index, file_info) in file_list.iter().enumerate() {
        let file_name = file_info.file_name().to_string_lossy().to_string();

        println!(
            "[File] [{}/{}] [IMPORTING] Name: {}",
            index + 1,
            file_list.len(),
            file_name,
        );
        let result = import_request_file(file_info, api_url.clone(), client, auth.clone(), semaphore_permits).await;

        match result {
            Ok(result) => {
                if !result.is_empty() {
                    incomplete_files.push(((index + 1, file_name), result));
                }

                println!(
                    "[File] [{}/{}] [OK] Imported file: {}",
                    index + 1,
                    file_list.len(),
                    file_info.file_name().to_string_lossy()
                );
            }
            Err(err) => {
                failed_files.push((index + 1, file_name));

                let err: anyhow::Error = err.context(format!(
                    "Failed to import file: {}",
                    file_info.file_name().to_string_lossy()
                ));

                println!("[File] [{}/{}] [FAIL] {err:?}", index + 1, file_list.len());
            }
        }
    }

    if !failed_files.is_empty() {
        println!("Failed to import the following files:");

        for (index, file) in failed_files {
            println!("  - [File] [{index}] {file}");
        }
    }

    if !incomplete_files.is_empty() {
        println!("The following files have failed requests:");

        for ((index, file), requests) in incomplete_files {
            println!("  - [File] [{index}] {file}");

            for (index, request) in requests {
                println!(
                    "    - [Request] [{index}] [{method:?}] {url}",
                    index = index + 1,
                    method = request.method,
                    url = request.url
                );
            }
        }
    }
}

async fn import_request_file(
    file_info: &DirEntry,
    api_url: Arc<str>,
    client: &Client,
    auth: Arc<Mutex<ApiTokens>>,
    mut semaphore_permits: usize,
) -> anyhow::Result<Vec<(usize, Request)>> {
    let path = file_info.path();

    let file = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let requests = serde_json::from_str::<Vec<heseya::Request>>(&file)
        .with_context(|| format!("Failed to parse file: {}", path.display()))?;

    if path.display().to_string().ends_with("synchronic.json") {
       semaphore_permits = 1;
    }

    let mut handles = Vec::new();
    let semaphore = Arc::new(Semaphore::new(semaphore_permits));
    let failed_requests: Arc<Mutex<Vec<RequestIdentifier>>> = Arc::new(Mutex::new(Vec::new()));

    for (index, request) in requests.iter().enumerate() {
        let api_url = api_url.clone();
        let client = client.clone();
        let request = request.clone();
        let auth = auth.clone();
        let length = requests.len();
        let failed_requests = failed_requests.clone();

        let permit = semaphore.clone().acquire_owned().await;

        let handle = tokio::spawn(async move {
            let result =
                heseya::make_request_retry_with_auth(&api_url, &request, &client, auth.clone())
                    .await;

            match result {
                Ok(response) => {
                    if !response.status().is_success() {
                        failed_requests.lock().await.push((index, request.clone()));
                    }

                    print_response(response, &request, index, length).await;
                }
                Err(err) => {
                    failed_requests.lock().await.push((index, request.clone()));

                    println!(
                        "[Request] [{}/{}] [FAIL] [{method:?}] {url}: {err:?}",
                        index + 1,
                        length,
                        method = request.method,
                        url = request.url,
                    );
                }
            }

            drop(permit);
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await?;
    }

    let mut failed_requests = failed_requests.lock().await.clone();
    failed_requests.sort_by_key(|(index, _)| *index);

    if !failed_requests.is_empty() {
        println!("Failed to import the following requests:");

        for (index, request) in &failed_requests {
            println!(
                "  - [Request] [{}] [{method:?}] {url}",
                index + 1,
                method = request.method,
                url = request.url
            );
        }
    }

    Ok(failed_requests)
}

async fn print_response(response: Response, request: &Request, index: usize, length: usize) {
    let is_success = response.status().is_success();

    println!(
        "[Request] [{}/{}] [{result_string}] [{method:?}] {url}: {status}",
        index + 1,
        length,
        result_string = if is_success { "OK" } else { "FAIL" },
        method = request.method,
        url = request.url,
        status = response.status()
    );

    if !is_success {
        let response_body = response.text().await;

        match response_body {
            Ok(response_body) => {
                let json = serde_json::from_str::<serde_json::Value>(&response_body);

                json.map_or_else(
                    |_| {
                        println!("{response_body:}");
                    },
                    |json| {
                        println!("{json}");
                    },
                );
            }
            Err(err) => println!("Failed to read response body: {err:?}"),
        }
    }
}
