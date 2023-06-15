use anyhow::Context;
use tokio::fs::DirEntry;

use crate::heseya::{self, Request};

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
        if file.file_type().await?.is_file() {
            file_list.push(file);
        }
    }

    file_list.sort_by_key(tokio::fs::DirEntry::path);

    Ok(file_list)
}

type FileIdentifier = (usize, String);
type RequestIdentifier = (usize, Request);
type IncompleteFile = (FileIdentifier, Vec<RequestIdentifier>);

pub async fn import_request_files(file_list: Vec<DirEntry>, heseya_sdk: &mut heseya::Sdk) {
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
        let result = import_request_file(file_info, heseya_sdk).await;

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
    heseya_sdk: &mut heseya::Sdk,
) -> anyhow::Result<Vec<(usize, Request)>> {
    let path = file_info.path();

    let file = tokio::fs::read_to_string(&path)
        .await
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let requests = serde_json::from_str::<Vec<heseya::Request>>(&file)
        .with_context(|| format!("Failed to parse file: {}", path.display()))?;

    let mut failed_requests: Vec<RequestIdentifier> = Vec::new();

    for (index, request) in requests.iter().enumerate() {
        let method = request.method.clone();
        let url = request.url.clone();
        let result = heseya::make_api_request(request, heseya_sdk).await;

        match result {
            Ok(response) => {
                let is_success = response.status().is_success();

                println!(
                    "[Request] [{}/{}] [{result_string}] [{method:?}] {url}: {status}",
                    index + 1,
                    requests.len(),
                    result_string = if is_success { "OK" } else { "FAIL" },
                    status = response.status()
                );

                if !is_success {
                    failed_requests.push((index, request.clone()));

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
            Err(err) => {
                failed_requests.push((index, request.clone()));

                println!(
                    "[Request] [{}/{}] [FAIL] [{method:?}] {url}: {err:?}",
                    index + 1,
                    requests.len(),
                );
            }
        }
    }

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
