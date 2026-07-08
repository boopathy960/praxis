use std::time::Duration;

use reqwest::blocking::Client;
use url::Url;

use crate::common::run_blocking_io;

pub fn safe_fetch_html(url_str: &str) -> Result<String, String> {
    let parsed_url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;

    // SSRF Check BEFORE fetching
    let host = parsed_url.host_str().unwrap_or_default();
    if crate::semantic_render::is_private_host(host) {
        return Err("Private or loopback URLs are blocked".into());
    }

    // The blocking reqwest client owns a private tokio runtime, so the whole
    // exchange runs on a dedicated thread (see run_blocking_io).
    run_blocking_io(move || {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                crate::common::AppError::Internal(format!("Failed to build client: {e}"))
            })?;

        let response = client
            .get(parsed_url)
            .send()
            .map_err(|e| crate::common::AppError::Internal(format!("Request failed: {e}")))?;

        // Enforce 2MB limit during read
        let mut body = String::new();
        let mut reader = std::io::Read::take(response, 2 * 1024 * 1024); // max 2MB
        std::io::Read::read_to_string(&mut reader, &mut body)
            .map_err(|e| crate::common::AppError::Internal(format!("Failed to read body: {e}")))?;

        Ok(body)
    })
    .map_err(|error| error.to_string())
}
