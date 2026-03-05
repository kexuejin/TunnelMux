use std::time::Duration;

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::Client;
use serde_json::json;
use tunnelmux_core::{
    CreateRouteRequest, DEFAULT_CONTROL_ADDR, DEFAULT_GATEWAY_TARGET_URL, DeleteRouteResponse,
    ErrorResponse, HealthCheckSettingsResponse, HealthResponse, MetricsResponse, RoutesResponse,
    TunnelLogsResponse, TunnelProvider, TunnelStartRequest, TunnelStatusResponse,
    UpdateHealthCheckSettingsRequest, UpstreamsHealthResponse,
};

#[derive(Debug, Parser)]
#[command(name = "tunnelmux", version, about = "TunnelMux CLI")]
struct Cli {
    #[arg(long, default_value = DEFAULT_CONTROL_ADDR)]
    server: String,

    #[arg(long)]
    token: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Read daemon health and tunnel status
    Status,
    /// Read runtime metrics snapshot
    Metrics {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,
    },
    /// Tunnel lifecycle controls
    Tunnel {
        #[command(subcommand)]
        command: TunnelCommand,
    },
    /// Route rules controls
    Routes {
        #[command(subcommand)]
        command: RoutesCommand,
    },
    /// Upstream health observability
    Upstreams {
        #[command(subcommand)]
        command: UpstreamsCommand,
    },
    /// Runtime settings controls
    Settings {
        #[command(subcommand)]
        command: SettingsCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TunnelCommand {
    /// Start tunnel process
    Start {
        #[arg(long, value_enum, default_value_t = CliTunnelProvider::Cloudflared)]
        provider: CliTunnelProvider,

        #[arg(long, default_value = DEFAULT_GATEWAY_TARGET_URL)]
        target_url: String,

        #[arg(long, default_value_t = true)]
        auto_restart: bool,
    },
    /// Tail provider logs
    Logs {
        #[arg(long, default_value_t = 200)]
        lines: usize,

        #[arg(long, default_value_t = false)]
        follow: bool,
    },
    /// Stop tunnel process
    Stop,
}

#[derive(Debug, Clone, ValueEnum)]
enum CliTunnelProvider {
    Cloudflared,
    Ngrok,
}

impl From<CliTunnelProvider> for TunnelProvider {
    fn from(value: CliTunnelProvider) -> Self {
        match value {
            CliTunnelProvider::Cloudflared => TunnelProvider::Cloudflared,
            CliTunnelProvider::Ngrok => TunnelProvider::Ngrok,
        }
    }
}

#[derive(Debug, Subcommand)]
enum RoutesCommand {
    /// List all routes
    List,
    /// Add a new route
    Add {
        #[arg(long)]
        id: String,

        #[arg(long)]
        upstream_url: String,

        #[arg(long)]
        fallback_upstream_url: Option<String>,

        #[arg(long)]
        health_check_path: Option<String>,

        #[arg(long)]
        host: Option<String>,

        #[arg(long)]
        path_prefix: Option<String>,

        #[arg(long)]
        strip_path_prefix: Option<String>,

        #[arg(long, default_value_t = false)]
        disabled: bool,
    },
    /// Update an existing route by id
    Update {
        #[arg(long)]
        id: String,

        #[arg(long)]
        upstream_url: String,

        #[arg(long)]
        fallback_upstream_url: Option<String>,

        #[arg(long)]
        health_check_path: Option<String>,

        #[arg(long)]
        host: Option<String>,

        #[arg(long)]
        path_prefix: Option<String>,

        #[arg(long)]
        strip_path_prefix: Option<String>,

        #[arg(long, default_value_t = false)]
        disabled: bool,
    },
    /// Remove route by id
    Remove {
        #[arg(long)]
        id: String,
    },
}

#[derive(Debug, Subcommand)]
enum UpstreamsCommand {
    /// List discovered upstream health snapshots
    Health {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,

        #[arg(long, default_value_t = false, conflicts_with = "table")]
        json: bool,

        #[arg(long, default_value_t = false)]
        table: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SettingsCommand {
    /// Read or update health-check settings
    HealthCheck {
        #[arg(long)]
        interval_ms: Option<u64>,

        #[arg(long)]
        timeout_ms: Option<u64>,

        #[arg(long)]
        path: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpstreamsOutputFormat {
    Table,
    Json,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let base_url = normalize_base_url(&cli.server);
    let token = resolve_api_token(cli.token);
    let client = Client::new();

    match cli.command {
        Command::Status => {
            let health: HealthResponse = get_json(&client, &base_url, "/v1/health", None).await?;
            let tunnel: TunnelStatusResponse =
                get_json(&client, &base_url, "/v1/tunnel/status", token.as_deref()).await?;

            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "health": health,
                    "tunnel": tunnel.tunnel,
                }))?
            );
        }
        Command::Metrics { watch, interval_ms } => {
            if watch {
                watch_metrics(
                    &client,
                    &base_url,
                    token.as_deref(),
                    normalize_watch_interval_ms(interval_ms)?,
                )
                .await?;
            } else {
                let metrics: MetricsResponse =
                    get_json(&client, &base_url, "/v1/metrics", token.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&metrics)?);
            }
        }
        Command::Tunnel { command } => match command {
            TunnelCommand::Start {
                provider,
                target_url,
                auto_restart,
            } => {
                let payload = TunnelStartRequest {
                    provider: provider.into(),
                    target_url,
                    auto_restart: Some(auto_restart),
                    metadata: None,
                };
                let status: TunnelStatusResponse = post_json(
                    &client,
                    &base_url,
                    "/v1/tunnel/start",
                    &payload,
                    token.as_deref(),
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
            TunnelCommand::Logs { lines, follow } => {
                if follow {
                    stream_logs(&client, &base_url, token.as_deref(), lines).await?;
                } else {
                    let url = format!("{}/v1/tunnel/logs", base_url);
                    let response = request_with_token(client.get(&url), token.as_deref())
                        .query(&[("lines", lines)])
                        .send()
                        .await
                        .with_context(|| format!("request failed: {url}"))?;
                    let logs: TunnelLogsResponse = decode_response(response).await?;
                    println!("{}", serde_json::to_string_pretty(&logs)?);
                }
            }
            TunnelCommand::Stop => {
                let status: TunnelStatusResponse = post_json(
                    &client,
                    &base_url,
                    "/v1/tunnel/stop",
                    &json!({}),
                    token.as_deref(),
                )
                .await?;
                println!("{}", serde_json::to_string_pretty(&status)?);
            }
        },
        Command::Routes { command } => match command {
            RoutesCommand::List => {
                let routes: RoutesResponse =
                    get_json(&client, &base_url, "/v1/routes", token.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&routes)?);
            }
            RoutesCommand::Add {
                id,
                upstream_url,
                fallback_upstream_url,
                health_check_path,
                host,
                path_prefix,
                strip_path_prefix,
                disabled,
            } => {
                let payload = CreateRouteRequest {
                    id,
                    match_host: host,
                    match_path_prefix: path_prefix,
                    strip_path_prefix,
                    upstream_url,
                    fallback_upstream_url,
                    health_check_path,
                    enabled: Some(!disabled),
                };
                let route: tunnelmux_core::RouteRule =
                    post_json(&client, &base_url, "/v1/routes", &payload, token.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&route)?);
            }
            RoutesCommand::Remove { id } => {
                let endpoint = format!("/v1/routes/{id}");
                let response: DeleteRouteResponse =
                    delete_json(&client, &base_url, &endpoint, token.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            RoutesCommand::Update {
                id,
                upstream_url,
                fallback_upstream_url,
                health_check_path,
                host,
                path_prefix,
                strip_path_prefix,
                disabled,
            } => {
                let payload = CreateRouteRequest {
                    id: id.clone(),
                    match_host: host,
                    match_path_prefix: path_prefix,
                    strip_path_prefix,
                    upstream_url,
                    fallback_upstream_url,
                    health_check_path,
                    enabled: Some(!disabled),
                };
                let endpoint = format!("/v1/routes/{id}");
                let route: tunnelmux_core::RouteRule =
                    put_json(&client, &base_url, &endpoint, &payload, token.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&route)?);
            }
        },
        Command::Upstreams { command } => match command {
            UpstreamsCommand::Health {
                watch,
                interval_ms,
                json,
                table: _,
            } => {
                let format = if json {
                    UpstreamsOutputFormat::Json
                } else {
                    UpstreamsOutputFormat::Table
                };
                if watch {
                    watch_upstreams_health(
                        &client,
                        &base_url,
                        token.as_deref(),
                        normalize_watch_interval_ms(interval_ms)?,
                        format,
                    )
                    .await?;
                } else {
                    let response: UpstreamsHealthResponse =
                        get_json(&client, &base_url, "/v1/upstreams/health", token.as_deref())
                            .await?;
                    println!("{}", format_upstreams_health(&response, format)?);
                }
            }
        },
        Command::Settings { command } => match command {
            SettingsCommand::HealthCheck {
                interval_ms,
                timeout_ms,
                path,
            } => {
                if interval_ms.is_none() && timeout_ms.is_none() && path.is_none() {
                    let response: HealthCheckSettingsResponse = get_json(
                        &client,
                        &base_url,
                        "/v1/settings/health-check",
                        token.as_deref(),
                    )
                    .await?;
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    let payload = UpdateHealthCheckSettingsRequest {
                        interval_ms,
                        timeout_ms,
                        path,
                    };
                    let response: HealthCheckSettingsResponse = put_json(
                        &client,
                        &base_url,
                        "/v1/settings/health-check",
                        &payload,
                        token.as_deref(),
                    )
                    .await?;
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
            }
        },
    }

    Ok(())
}

async fn get_json<T: serde::de::DeserializeOwned>(
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

async fn post_json<T: serde::de::DeserializeOwned>(
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

async fn delete_json<T: serde::de::DeserializeOwned>(
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

async fn put_json<T: serde::de::DeserializeOwned>(
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

fn request_with_token(
    builder: reqwest::RequestBuilder,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    match token {
        Some(token) => builder.bearer_auth(token),
        None => builder,
    }
}

async fn stream_logs(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    lines: usize,
) -> anyhow::Result<()> {
    let url = format!("{}/v1/tunnel/logs/stream", base_url);
    let mut response = request_with_token(client.get(&url), token)
        .query(&[("lines", lines)])
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .context("failed to read error response body")?;
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|err| err.error)
            .unwrap_or(body);
        return Err(anyhow!("HTTP {}: {}", status, message));
    }

    let mut buffer = String::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("failed to read stream chunk")?
    {
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim_end().to_string();
            buffer.drain(..=pos);
            if let Some(payload) = line.strip_prefix("data:") {
                println!("{}", payload.trim_start());
            }
        }
    }

    Ok(())
}

async fn watch_upstreams_health(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
    format: UpstreamsOutputFormat,
) -> anyhow::Result<()> {
    loop {
        let response: UpstreamsHealthResponse =
            get_json(client, base_url, "/v1/upstreams/health", token).await?;
        print!("\x1B[2J\x1B[H");
        println!("{}", format_upstreams_health(&response, format)?);
        println!();
        println!("refresh every {}ms, press Ctrl+C to stop", interval_ms);

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {}
        }
    }

    Ok(())
}

async fn watch_metrics(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
) -> anyhow::Result<()> {
    loop {
        let metrics: MetricsResponse = get_json(client, base_url, "/v1/metrics", token).await?;
        print!("\x1B[2J\x1B[H");
        println!("{}", serde_json::to_string_pretty(&metrics)?);
        println!();
        println!("refresh every {}ms, press Ctrl+C to stop", interval_ms);

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {}
        }
    }

    Ok(())
}

fn format_upstreams_health(
    response: &UpstreamsHealthResponse,
    format: UpstreamsOutputFormat,
) -> anyhow::Result<String> {
    match format {
        UpstreamsOutputFormat::Json => Ok(serde_json::to_string_pretty(response)?),
        UpstreamsOutputFormat::Table => Ok(render_upstreams_health_table(response)),
    }
}

fn render_upstreams_health_table(response: &UpstreamsHealthResponse) -> String {
    let headers = [
        "UPSTREAM_URL",
        "CHECK_PATH",
        "HEALTH",
        "LAST_CHECKED_AT",
        "LAST_ERROR",
    ];
    let mut rows = Vec::with_capacity(response.upstreams.len());
    for item in &response.upstreams {
        rows.push(vec![
            truncate_cell(&item.upstream_url, 60),
            truncate_cell(&item.health_check_path, 24),
            upstream_health_label(item.healthy).to_string(),
            item.last_checked_at
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            truncate_cell(item.last_error.as_deref().unwrap_or("-"), 72),
        ]);
    }

    let mut widths = headers.map(str::len);
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            if cell.len() > widths[index] {
                widths[index] = cell.len();
            }
        }
    }

    let mut output = String::new();
    output.push_str(&format_table_separator(&widths));
    output.push('\n');
    output.push_str(&format_table_row(&headers, &widths));
    output.push('\n');
    output.push_str(&format_table_separator(&widths));
    for row in &rows {
        output.push('\n');
        output.push_str(&format_table_row(
            &[&row[0], &row[1], &row[2], &row[3], &row[4]],
            &widths,
        ));
    }
    output.push('\n');
    output.push_str(&format_table_separator(&widths));
    output
}

fn format_table_separator(widths: &[usize; 5]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

fn format_table_row(values: &[&str; 5], widths: &[usize; 5]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
}

fn upstream_health_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "healthy",
        Some(false) => "unhealthy",
        None => "unknown",
    }
}

fn truncate_cell(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let keep = max_len.saturating_sub(3);
    format!("{}...", &value[..keep])
}

async fn decode_response<T: serde::de::DeserializeOwned>(
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

fn normalize_base_url(server: &str) -> String {
    let trimmed = server.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    format!("http://{trimmed}")
}

fn resolve_api_token(arg_token: Option<String>) -> Option<String> {
    arg_token
        .or_else(|| std::env::var("TUNNELMUX_API_TOKEN").ok())
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn normalize_watch_interval_ms(value: u64) -> anyhow::Result<u64> {
    if !(200..=60_000).contains(&value) {
        return Err(anyhow!(
            "interval_ms out of range: expected 200..=60000, got {}",
            value
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tunnelmux_core::UpstreamHealthEntry;

    #[test]
    fn normalize_watch_interval_ms_applies_bounds() {
        assert_eq!(normalize_watch_interval_ms(200).expect("min"), 200);
        assert_eq!(normalize_watch_interval_ms(2000).expect("normal"), 2000);
        assert_eq!(normalize_watch_interval_ms(60_000).expect("max"), 60_000);
    }

    #[test]
    fn normalize_watch_interval_ms_rejects_out_of_range() {
        assert!(normalize_watch_interval_ms(199).is_err());
        assert!(normalize_watch_interval_ms(60_001).is_err());
    }

    #[test]
    fn render_upstreams_health_table_contains_headers_and_rows() {
        let response = UpstreamsHealthResponse {
            upstreams: vec![
                UpstreamHealthEntry {
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    health_check_path: "/healthz".to_string(),
                    healthy: Some(true),
                    last_checked_at: Some("2026-03-05T10:00:00Z".to_string()),
                    last_error: None,
                },
                UpstreamHealthEntry {
                    upstream_url: "http://127.0.0.1:3001".to_string(),
                    health_check_path: "/".to_string(),
                    healthy: None,
                    last_checked_at: None,
                    last_error: Some("status 503".to_string()),
                },
            ],
        };

        let table = render_upstreams_health_table(&response);
        assert!(table.contains("UPSTREAM_URL"));
        assert!(table.contains("CHECK_PATH"));
        assert!(table.contains("healthy"));
        assert!(table.contains("unknown"));
        assert!(table.contains("status 503"));
    }

    #[test]
    fn format_upstreams_health_outputs_json() {
        let response = UpstreamsHealthResponse {
            upstreams: vec![UpstreamHealthEntry {
                upstream_url: "http://127.0.0.1:3000".to_string(),
                health_check_path: "/healthz".to_string(),
                healthy: Some(true),
                last_checked_at: Some("2026-03-05T10:00:00Z".to_string()),
                last_error: None,
            }],
        };

        let json = format_upstreams_health(&response, UpstreamsOutputFormat::Json)
            .expect("json format should succeed");
        assert!(json.contains("\"health_check_path\": \"/healthz\""));
    }
}
