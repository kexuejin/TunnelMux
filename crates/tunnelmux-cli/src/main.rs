use std::collections::HashSet;
use std::io::{self, Read};
use std::time::Duration;
use std::{fs, path::Path, path::PathBuf};

use anyhow::{Context, anyhow};
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::Client;
use serde_json::json;
use tunnelmux_core::{
    ApplyRoutesRequest, ApplyRoutesResponse, CreateRouteRequest, DEFAULT_CONTROL_ADDR,
    DEFAULT_GATEWAY_TARGET_URL, DashboardResponse, DeleteRouteResponse, ErrorResponse,
    HealthCheckSettingsResponse, HealthResponse, MetricsResponse, RoutesResponse,
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
    /// Read composite dashboard snapshot (tunnel, metrics, routes, upstreams)
    Dashboard {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = false, conflicts_with = "watch")]
        stream: bool,

        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,
    },
    /// Read runtime metrics snapshot
    Metrics {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = false, conflicts_with = "watch")]
        stream: bool,

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
        #[arg(long, required_unless_present = "from_json")]
        id: Option<String>,

        #[arg(long, required_unless_present = "from_json")]
        upstream_url: Option<String>,

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

        #[arg(
            long,
            help = "Load route payload from JSON file (use '-' for stdin)",
            conflicts_with_all = [
                "id",
                "upstream_url",
                "fallback_upstream_url",
                "health_check_path",
                "host",
                "path_prefix",
                "strip_path_prefix",
                "disabled"
            ]
        )]
        from_json: Option<String>,
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

        #[arg(
            long,
            help = "Load route payload from JSON file (use '-' for stdin)",
            conflicts_with_all = [
                "upstream_url",
                "fallback_upstream_url",
                "health_check_path",
                "host",
                "path_prefix",
                "strip_path_prefix",
                "disabled"
            ]
        )]
        from_json: Option<String>,
    },
    /// Export routes as JSON payloads for --from-json reuse
    Export {
        #[arg(long)]
        id: Option<String>,

        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Apply route payload(s) from JSON file (upsert by id)
    Apply {
        #[arg(long, help = "Load route payloads from JSON file (use '-' for stdin)")]
        from_json: String,

        #[arg(long, default_value_t = false)]
        replace: bool,

        #[arg(long, default_value_t = false)]
        dry_run: bool,

        #[arg(long, default_value_t = false)]
        allow_empty: bool,
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

        #[arg(long, default_value_t = false, conflicts_with = "watch")]
        stream: bool,

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
        Command::Dashboard {
            watch,
            stream,
            interval_ms,
        } => {
            let interval_ms = normalize_watch_interval_ms(interval_ms)?;
            if stream {
                stream_dashboard(&client, &base_url, token.as_deref(), interval_ms).await?;
            } else if watch {
                watch_dashboard(&client, &base_url, token.as_deref(), interval_ms).await?;
            } else {
                let dashboard: DashboardResponse =
                    get_json(&client, &base_url, "/v1/dashboard", token.as_deref()).await?;
                println!("{}", serde_json::to_string_pretty(&dashboard)?);
            }
        }
        Command::Metrics {
            watch,
            stream,
            interval_ms,
        } => {
            let interval_ms = normalize_watch_interval_ms(interval_ms)?;
            if stream {
                stream_metrics(&client, &base_url, token.as_deref(), interval_ms).await?;
            } else if watch {
                watch_metrics(&client, &base_url, token.as_deref(), interval_ms).await?;
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
                from_json,
            } => {
                let payload = if let Some(path) = from_json {
                    load_route_request_from_file(Path::new(&path))?
                } else {
                    CreateRouteRequest {
                        id: id.ok_or_else(|| anyhow!("missing --id"))?,
                        match_host: host,
                        match_path_prefix: path_prefix,
                        strip_path_prefix,
                        upstream_url: upstream_url
                            .ok_or_else(|| anyhow!("missing --upstream-url"))?,
                        fallback_upstream_url,
                        health_check_path,
                        enabled: Some(!disabled),
                    }
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
            RoutesCommand::Export { id, out } => {
                let routes: RoutesResponse =
                    get_json(&client, &base_url, "/v1/routes", token.as_deref()).await?;
                if let Some(id) = id {
                    let route = routes
                        .routes
                        .iter()
                        .find(|item| item.id == id)
                        .ok_or_else(|| anyhow!("route '{}' not found", id))?;
                    let payload = route_rule_to_create_request(route);
                    let rendered = serde_json::to_string_pretty(&payload)?;
                    write_output_or_stdout(&rendered, out.as_deref())?;
                } else {
                    let payloads = routes
                        .routes
                        .iter()
                        .map(route_rule_to_create_request)
                        .collect::<Vec<_>>();
                    let rendered = serde_json::to_string_pretty(&payloads)?;
                    write_output_or_stdout(&rendered, out.as_deref())?;
                }
            }
            RoutesCommand::Apply {
                from_json,
                replace,
                dry_run,
                allow_empty,
            } => {
                let requests = load_route_requests_from_file(Path::new(&from_json))?;
                ensure_unique_route_ids(&requests)?;
                let payload = ApplyRoutesRequest {
                    routes: requests,
                    replace: Some(replace),
                    dry_run: Some(dry_run),
                    allow_empty: Some(allow_empty),
                };
                let response: ApplyRoutesResponse = post_json(
                    &client,
                    &base_url,
                    "/v1/routes/apply",
                    &payload,
                    token.as_deref(),
                )
                .await?;
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
                from_json,
            } => {
                let payload = if let Some(path) = from_json {
                    let mut loaded = load_route_request_from_file(Path::new(&path))?;
                    loaded.id = id.clone();
                    loaded
                } else {
                    CreateRouteRequest {
                        id: id.clone(),
                        match_host: host,
                        match_path_prefix: path_prefix,
                        strip_path_prefix,
                        upstream_url,
                        fallback_upstream_url,
                        health_check_path,
                        enabled: Some(!disabled),
                    }
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
                stream,
                interval_ms,
                json,
                table: _,
            } => {
                let format = if json {
                    UpstreamsOutputFormat::Json
                } else {
                    UpstreamsOutputFormat::Table
                };
                let interval_ms = normalize_watch_interval_ms(interval_ms)?;
                if stream {
                    stream_upstreams_health(
                        &client,
                        &base_url,
                        token.as_deref(),
                        interval_ms,
                        format,
                    )
                    .await?;
                } else if watch {
                    watch_upstreams_health(
                        &client,
                        &base_url,
                        token.as_deref(),
                        interval_ms,
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

async fn stream_metrics(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
) -> anyhow::Result<()> {
    let url = format!("{}/v1/metrics/stream", base_url);
    let mut response = request_with_token(client.get(&url), token)
        .query(&[("interval_ms", interval_ms)])
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .context("failed to read stream error response body")?;
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|err| err.error)
            .unwrap_or(body);
        return Err(anyhow!("HTTP {}: {}", status, message));
    }

    let mut pending = String::new();
    let mut builder = SseFrameBuilder::default();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            chunk = response.chunk() => {
                let chunk = chunk.context("failed to read metrics stream chunk")?;
                let Some(chunk) = chunk else {
                    break;
                };
                pending.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(line) = take_next_sse_line(&mut pending) {
                    if let Some(frame) = builder.push_line(&line) {
                        render_metrics_stream_frame(&frame, interval_ms)?;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn watch_dashboard(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
) -> anyhow::Result<()> {
    loop {
        let dashboard: DashboardResponse =
            get_json(client, base_url, "/v1/dashboard", token).await?;
        print!("\x1B[2J\x1B[H");
        println!("{}", serde_json::to_string_pretty(&dashboard)?);
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

async fn stream_dashboard(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
) -> anyhow::Result<()> {
    let url = format!("{}/v1/dashboard/stream", base_url);
    let mut response = request_with_token(client.get(&url), token)
        .query(&[("interval_ms", interval_ms)])
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .context("failed to read stream error response body")?;
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|err| err.error)
            .unwrap_or(body);
        return Err(anyhow!("HTTP {}: {}", status, message));
    }

    let mut pending = String::new();
    let mut builder = SseFrameBuilder::default();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            chunk = response.chunk() => {
                let chunk = chunk.context("failed to read dashboard stream chunk")?;
                let Some(chunk) = chunk else {
                    break;
                };
                pending.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(line) = take_next_sse_line(&mut pending) {
                    if let Some(frame) = builder.push_line(&line) {
                        render_dashboard_stream_frame(&frame, interval_ms)?;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn stream_upstreams_health(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
    format: UpstreamsOutputFormat,
) -> anyhow::Result<()> {
    let url = format!("{}/v1/upstreams/health/stream", base_url);
    let mut response = request_with_token(client.get(&url), token)
        .query(&[("interval_ms", interval_ms)])
        .send()
        .await
        .with_context(|| format!("request failed: {url}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .context("failed to read stream error response body")?;
        let message = serde_json::from_str::<ErrorResponse>(&body)
            .map(|err| err.error)
            .unwrap_or(body);
        return Err(anyhow!("HTTP {}: {}", status, message));
    }

    let mut pending = String::new();
    let mut builder = SseFrameBuilder::default();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            chunk = response.chunk() => {
                let chunk = chunk.context("failed to read upstreams stream chunk")?;
                let Some(chunk) = chunk else {
                    break;
                };
                pending.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(line) = take_next_sse_line(&mut pending) {
                    if let Some(frame) = builder.push_line(&line) {
                        render_upstreams_stream_frame(&frame, interval_ms, format)?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SseFrame {
    event: String,
    data: String,
}

#[derive(Debug, Default)]
struct SseFrameBuilder {
    event: Option<String>,
    data_lines: Vec<String>,
}

impl SseFrameBuilder {
    fn push_line(&mut self, line: &str) -> Option<SseFrame> {
        if line.is_empty() {
            return self.flush();
        }
        if line.starts_with(':') {
            return None;
        }
        if let Some(value) = line.strip_prefix("event:") {
            self.event = Some(trim_sse_field_value(value).to_string());
            return None;
        }
        if let Some(value) = line.strip_prefix("data:") {
            self.data_lines
                .push(trim_sse_field_value(value).to_string());
            return None;
        }
        None
    }

    fn flush(&mut self) -> Option<SseFrame> {
        if self.event.is_none() && self.data_lines.is_empty() {
            return None;
        }
        let frame = SseFrame {
            event: self.event.take().unwrap_or_else(|| "message".to_string()),
            data: self.data_lines.join("\n"),
        };
        self.data_lines.clear();
        Some(frame)
    }
}

fn trim_sse_field_value(value: &str) -> &str {
    value.strip_prefix(' ').unwrap_or(value)
}

fn take_next_sse_line(buffer: &mut String) -> Option<String> {
    let index = buffer.find('\n')?;
    let mut line = buffer[..index].to_string();
    buffer.drain(..=index);
    if line.ends_with('\r') {
        line.pop();
    }
    Some(line)
}

fn render_metrics_stream_frame(frame: &SseFrame, interval_ms: u64) -> anyhow::Result<()> {
    match frame.event.as_str() {
        "snapshot" => {
            let snapshot: MetricsResponse =
                serde_json::from_str(&frame.data).with_context(|| {
                    format!("failed to parse metrics snapshot event: {}", frame.data)
                })?;
            print!("\x1B[2J\x1B[H");
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            println!();
            println!(
                "metrics stream interval {}ms, press Ctrl+C to stop",
                interval_ms
            );
        }
        "error" => {
            eprintln!("metrics stream error: {}", frame.data);
        }
        _ => {}
    }
    Ok(())
}

fn render_dashboard_stream_frame(frame: &SseFrame, interval_ms: u64) -> anyhow::Result<()> {
    match frame.event.as_str() {
        "snapshot" => {
            let snapshot: DashboardResponse =
                serde_json::from_str(&frame.data).with_context(|| {
                    format!("failed to parse dashboard snapshot event: {}", frame.data)
                })?;
            print!("\x1B[2J\x1B[H");
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
            println!();
            println!(
                "dashboard stream interval {}ms, press Ctrl+C to stop",
                interval_ms
            );
        }
        "error" => {
            eprintln!("dashboard stream error: {}", frame.data);
        }
        _ => {}
    }
    Ok(())
}

fn render_upstreams_stream_frame(
    frame: &SseFrame,
    interval_ms: u64,
    format: UpstreamsOutputFormat,
) -> anyhow::Result<()> {
    match frame.event.as_str() {
        "snapshot" => {
            let snapshot: UpstreamsHealthResponse = serde_json::from_str(&frame.data)
                .with_context(|| {
                    format!("failed to parse upstreams snapshot event: {}", frame.data)
                })?;
            print!("\x1B[2J\x1B[H");
            println!("{}", format_upstreams_health(&snapshot, format)?);
            println!();
            println!(
                "upstreams stream interval {}ms, press Ctrl+C to stop",
                interval_ms
            );
        }
        "error" => {
            eprintln!("upstreams stream error: {}", frame.data);
        }
        _ => {}
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

fn load_route_request_from_file(path: &Path) -> anyhow::Result<CreateRouteRequest> {
    let mut requests = load_route_requests_from_file(path)?;
    if requests.len() != 1 {
        return Err(anyhow!(
            "expected exactly one route payload in {}",
            path.display()
        ));
    }
    Ok(requests.remove(0))
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum RoutePayloadFile {
    Single(CreateRouteRequest),
    Many(Vec<CreateRouteRequest>),
}

fn load_route_requests_from_file(path: &Path) -> anyhow::Result<Vec<CreateRouteRequest>> {
    let raw = read_route_payload_source(path)?;
    let parsed: RoutePayloadFile = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse route json file: {}", path.display()))?;
    match parsed {
        RoutePayloadFile::Single(route) => Ok(vec![route]),
        RoutePayloadFile::Many(routes) => Ok(routes),
    }
}

fn read_route_payload_source(path: &Path) -> anyhow::Result<String> {
    if path == Path::new("-") {
        let mut raw = String::new();
        io::stdin()
            .read_to_string(&mut raw)
            .context("failed to read route json from stdin")?;
        return Ok(raw);
    }

    fs::read_to_string(path)
        .with_context(|| format!("failed to read route json file: {}", path.display()))
}

fn ensure_unique_route_ids(routes: &[CreateRouteRequest]) -> anyhow::Result<()> {
    let mut ids = HashSet::new();
    for route in routes {
        if !ids.insert(route.id.clone()) {
            return Err(anyhow!("duplicate route id in payload: {}", route.id));
        }
    }
    Ok(())
}

fn route_rule_to_create_request(route: &tunnelmux_core::RouteRule) -> CreateRouteRequest {
    CreateRouteRequest {
        id: route.id.clone(),
        match_host: route.match_host.clone(),
        match_path_prefix: route.match_path_prefix.clone(),
        strip_path_prefix: route.strip_path_prefix.clone(),
        upstream_url: route.upstream_url.clone(),
        fallback_upstream_url: route.fallback_upstream_url.clone(),
        health_check_path: route.health_check_path.clone(),
        enabled: Some(route.enabled),
    }
}

fn write_output_or_stdout(output: &str, out: Option<&Path>) -> anyhow::Result<()> {
    if let Some(path) = out {
        fs::write(path, output)
            .with_context(|| format!("failed to write export output: {}", path.display()))?;
        return Ok(());
    }

    println!("{output}");
    Ok(())
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
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

    fn next_test_id() -> u64 {
        static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);
        NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
    }

    fn temp_route_json_path() -> PathBuf {
        std::env::temp_dir().join(format!("tunnelmux-cli-route-{}.json", next_test_id()))
    }

    #[test]
    fn load_route_request_from_file_reads_valid_json() {
        let path = temp_route_json_path();
        fs::write(
            &path,
            r#"{
  "id":"svc-a",
  "match_host":"demo.local",
  "match_path_prefix":"/",
  "strip_path_prefix":null,
  "upstream_url":"http://127.0.0.1:3000",
  "fallback_upstream_url":"http://127.0.0.1:3001",
  "health_check_path":"/healthz",
  "enabled":true
}"#,
        )
        .expect("write route file");
        let route = load_route_request_from_file(&path).expect("load route file");
        assert_eq!(route.id, "svc-a");
        assert_eq!(route.health_check_path.as_deref(), Some("/healthz"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_route_request_from_file_rejects_invalid_json() {
        let path = temp_route_json_path();
        fs::write(&path, r#"{"id":"svc-a","upstream_url":123}"#).expect("write route file");
        let result = load_route_request_from_file(&path);
        assert!(result.is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_route_requests_from_file_reads_array_json() {
        let path = temp_route_json_path();
        fs::write(
            &path,
            r#"[
  {"id":"svc-a","upstream_url":"http://127.0.0.1:3000"},
  {"id":"svc-b","upstream_url":"http://127.0.0.1:3001","enabled":false}
]"#,
        )
        .expect("write route file");
        let routes = load_route_requests_from_file(&path).expect("load route file");
        assert_eq!(routes.len(), 2);
        assert_eq!(routes[0].id, "svc-a");
        assert_eq!(routes[1].id, "svc-b");
        assert_eq!(routes[1].enabled, Some(false));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn load_route_request_from_file_rejects_array_payload() {
        let path = temp_route_json_path();
        fs::write(
            &path,
            r#"[
  {"id":"svc-a","upstream_url":"http://127.0.0.1:3000"},
  {"id":"svc-b","upstream_url":"http://127.0.0.1:3001"}
]"#,
        )
        .expect("write route file");
        let result = load_route_request_from_file(&path);
        assert!(result.is_err());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn route_rule_to_create_request_preserves_fields() {
        let route = tunnelmux_core::RouteRule {
            id: "svc-a".to_string(),
            match_host: Some("demo.local".to_string()),
            match_path_prefix: Some("/".to_string()),
            strip_path_prefix: Some("/api".to_string()),
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: Some("/healthz".to_string()),
            enabled: false,
        };

        let payload = route_rule_to_create_request(&route);
        assert_eq!(payload.id, route.id);
        assert_eq!(payload.match_host, route.match_host);
        assert_eq!(payload.match_path_prefix, route.match_path_prefix);
        assert_eq!(payload.strip_path_prefix, route.strip_path_prefix);
        assert_eq!(payload.upstream_url, route.upstream_url);
        assert_eq!(payload.fallback_upstream_url, route.fallback_upstream_url);
        assert_eq!(payload.health_check_path, route.health_check_path);
        assert_eq!(payload.enabled, Some(false));
    }

    #[test]
    fn write_output_or_stdout_writes_file_when_path_is_set() {
        let path = std::env::temp_dir().join(format!(
            "tunnelmux-export-output-{}.json",
            std::process::id()
        ));
        let output = r#"{"id":"svc-a"}"#;

        write_output_or_stdout(output, Some(&path)).expect("write output file");
        let written = fs::read_to_string(&path).expect("read output file");
        assert_eq!(written, output);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn ensure_unique_route_ids_rejects_duplicates() {
        let routes = vec![
            CreateRouteRequest {
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: None,
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: Some(true),
            },
            CreateRouteRequest {
                id: "svc-a".to_string(),
                match_host: None,
                match_path_prefix: None,
                strip_path_prefix: None,
                upstream_url: "http://127.0.0.1:3001".to_string(),
                fallback_upstream_url: None,
                health_check_path: None,
                enabled: Some(true),
            },
        ];
        assert!(ensure_unique_route_ids(&routes).is_err());
    }

    #[test]
    fn take_next_sse_line_handles_lf_and_crlf() {
        let mut buffer = "event: snapshot\r\ndata: {\"ok\":true}\n\n".to_string();
        assert_eq!(
            take_next_sse_line(&mut buffer).expect("first line"),
            "event: snapshot"
        );
        assert_eq!(
            take_next_sse_line(&mut buffer).expect("second line"),
            "data: {\"ok\":true}"
        );
        assert_eq!(take_next_sse_line(&mut buffer).expect("blank line"), "");
        assert!(take_next_sse_line(&mut buffer).is_none());
    }

    #[test]
    fn sse_frame_builder_builds_snapshot_frame() {
        let mut builder = SseFrameBuilder::default();
        assert!(builder.push_line("event: snapshot").is_none());
        assert!(builder.push_line("data: {\"a\":1}").is_none());
        let frame = builder.push_line("").expect("frame should flush");
        assert_eq!(frame.event, "snapshot");
        assert_eq!(frame.data, "{\"a\":1}");
    }

    #[test]
    fn sse_frame_builder_combines_multiline_data() {
        let mut builder = SseFrameBuilder::default();
        assert!(builder.push_line("data: one").is_none());
        assert!(builder.push_line("data: two").is_none());
        let frame = builder.push_line("").expect("frame should flush");
        assert_eq!(frame.event, "message");
        assert_eq!(frame.data, "one\ntwo");
    }
}
