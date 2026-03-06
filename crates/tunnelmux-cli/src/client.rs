use super::*;

pub(super) async fn get_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    base_url: &str,
    path: &str,
    token: Option<&str>,
) -> anyhow::Result<T> {
    let url = format!("{}{}", base_url, path);
    let response = request_with_token(client.get(&url), token)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    decode_response(response).await
}

pub(super) async fn post_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    base_url: &str,
    path: &str,
    payload: &impl serde::Serialize,
    token: Option<&str>,
) -> anyhow::Result<T> {
    let url = format!("{}{}", base_url, path);
    let response = request_with_token(client.post(&url), token)
        .json(payload)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    decode_response(response).await
}

pub(super) async fn delete_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    base_url: &str,
    path: &str,
    token: Option<&str>,
) -> anyhow::Result<T> {
    let url = format!("{}{}", base_url, path);
    let response = request_with_token(client.delete(&url), token)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    decode_response(response).await
}

pub(super) async fn delete_route_by_id(
    client: &Client,
    base_url: &str,
    id: &str,
    token: Option<&str>,
    ignore_missing: bool,
) -> anyhow::Result<DeleteRouteResponse> {
    let path = format!("/v1/routes/{id}");
    let url = format!("{}{}", base_url, path);
    let response = request_with_token(client.delete(&url), token)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    if status == ReqwestStatusCode::NOT_FOUND && ignore_missing {
        return Ok(DeleteRouteResponse { removed: false });
    }
    if !status.is_success() {
        return Err(anyhow!("HTTP {}: {}", status, extract_error_message(&body)));
    }
    serde_json::from_str::<DeleteRouteResponse>(&body)
        .with_context(|| format!("failed to parse delete route response: {}", body))
}

pub(super) async fn put_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    base_url: &str,
    path: &str,
    payload: &impl serde::Serialize,
    token: Option<&str>,
) -> anyhow::Result<T> {
    let url = format!("{}{}", base_url, path);
    let response = request_with_token(client.put(&url), token)
        .json(payload)
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    decode_response(response).await
}

pub(super) fn request_with_token(
    builder: reqwest::RequestBuilder,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    match token {
        Some(token) => builder.bearer_auth(token),
        None => builder,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StreamAttemptOutcome {
    Stopped,
    Disconnected,
}

#[derive(Debug)]
pub(super) enum StreamAttemptError {
    Retryable(anyhow::Error),
    Fatal(anyhow::Error),
}

pub(super) async fn stream_sse_with_reconnect<F>(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    path: &str,
    interval_ms: u64,
    retry_policy: StreamRetryPolicy,
    stream_name: &str,
    mut render_frame: F,
) -> anyhow::Result<()>
where
    F: FnMut(&SseFrame) -> anyhow::Result<()>,
{
    let url = format!("{}{}", base_url, path);
    let mut retry_delay_ms = retry_policy.initial_ms;

    loop {
        let backoff_after_wait = match stream_sse_once(
            client,
            &url,
            token,
            interval_ms,
            stream_name,
            &mut render_frame,
        )
        .await
        {
            Ok(StreamAttemptOutcome::Stopped) => return Ok(()),
            Ok(StreamAttemptOutcome::Disconnected) => {
                retry_delay_ms = retry_policy.initial_ms;
                eprintln!(
                    "{} stream disconnected; reconnecting in {}ms",
                    stream_name, retry_delay_ms
                );
                false
            }
            Err(StreamAttemptError::Retryable(error)) => {
                eprintln!(
                    "{} stream interrupted; reconnecting in {}ms: {:#}",
                    stream_name, retry_delay_ms, error
                );
                true
            }
            Err(StreamAttemptError::Fatal(error)) => return Err(error),
        };

        if wait_before_stream_retry(retry_delay_ms).await? {
            return Ok(());
        }
        if backoff_after_wait {
            retry_delay_ms = next_stream_retry_delay_ms(retry_delay_ms, retry_policy);
        }
    }
}

pub(super) async fn stream_sse_once<F>(
    client: &Client,
    url: &str,
    token: Option<&str>,
    interval_ms: u64,
    stream_name: &str,
    render_frame: &mut F,
) -> Result<StreamAttemptOutcome, StreamAttemptError>
where
    F: FnMut(&SseFrame) -> anyhow::Result<()>,
{
    let mut response = request_with_token(client.get(url), token)
        .query(&[("interval_ms", interval_ms)])
        .send()
        .await
        .map_err(|error| {
            StreamAttemptError::Retryable(
                anyhow!(error).context(format!("request failed for stream endpoint: {url}")),
            )
        })?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.map_err(|error| {
            StreamAttemptError::Fatal(anyhow!(error).context("failed to read stream error body"))
        })?;
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|err| err.error)
            .unwrap_or(body);
        return Err(StreamAttemptError::Fatal(anyhow!(
            "HTTP {} while opening {} stream: {}",
            status,
            stream_name,
            message
        )));
    }

    let mut pending = String::new();
    let mut builder = SseFrameBuilder::default();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                return Ok(StreamAttemptOutcome::Stopped);
            }
            chunk = response.chunk() => {
                let chunk = chunk.map_err(|error| {
                    StreamAttemptError::Retryable(anyhow!(error).context(format!(
                        "failed to read {} stream chunk",
                        stream_name
                    )))
                })?;
                let Some(chunk) = chunk else {
                    return Ok(StreamAttemptOutcome::Disconnected);
                };
                pending.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(line) = take_next_sse_line(&mut pending) {
                    if let Some(frame) = builder.push_line(&line) {
                        render_frame(&frame).map_err(StreamAttemptError::Fatal)?;
                    }
                }
            }
        }
    }
}

pub(super) async fn wait_before_stream_retry(delay_ms: u64) -> anyhow::Result<bool> {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Ok(true),
        _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => Ok(false),
    }
}

pub(super) fn next_stream_retry_delay_ms(
    current_delay_ms: u64,
    retry_policy: StreamRetryPolicy,
) -> u64 {
    current_delay_ms
        .saturating_mul(2)
        .clamp(retry_policy.initial_ms, retry_policy.max_ms)
}

pub(super) async fn decode_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> anyhow::Result<T> {
    let status = response.status();
    let body = response
        .text()
        .await
        .context("failed to read response body")?;
    if !status.is_success() {
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|err| err.error)
            .unwrap_or_else(|_| body.clone());
        return Err(anyhow!("HTTP {}: {}", status, message));
    }

    serde_json::from_str::<T>(&body).with_context(|| {
        format!(
            "failed to parse success response (status {}): {}",
            status, body
        )
    })
}

pub(super) fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<ErrorResponse>(body)
        .map(|err| err.error)
        .unwrap_or_else(|_| body.to_string())
}
