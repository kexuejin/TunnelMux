use std::collections::HashSet;
use std::io::{self, Read};
use std::time::Duration;
use std::{fs, path::Path, path::PathBuf};

use anyhow::{Context, anyhow};
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::Client;
use serde_json::json;
use tunnelmux_control_client::{ControlClientConfig, TunnelmuxControlClient};
use tunnelmux_core::{
    ApplyRoutesRequest, ApplyRoutesResponse, CreateRouteRequest, DEFAULT_CONTROL_ADDR,
    DEFAULT_GATEWAY_TARGET_URL, DashboardResponse, DeleteRouteResponse, DiagnosticsResponse,
    ErrorResponse, HealthCheckSettingsResponse, HealthResponse, MetricsResponse,
    ReloadSettingsResponse, RouteMatchResponse, RoutesResponse, TunnelLogsResponse, TunnelProvider,
    TunnelStartRequest, TunnelState, TunnelStatusResponse, UpdateHealthCheckSettingsRequest,
    UpstreamsHealthResponse,
};

mod client;
mod commands;
mod output;

use client::*;
use commands::*;
use output::*;

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
    Status {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = false, conflicts_with = "watch")]
        stream: bool,

        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,

        #[command(flatten)]
        retry: StreamRetryArgs,
    },
    /// Read composite dashboard snapshot (tunnel, metrics, routes, upstreams)
    Dashboard {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = false, conflicts_with = "watch")]
        stream: bool,

        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,

        #[command(flatten)]
        retry: StreamRetryArgs,
    },
    /// Read runtime metrics snapshot
    Metrics {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = false, conflicts_with = "watch")]
        stream: bool,

        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,

        #[command(flatten)]
        retry: StreamRetryArgs,
    },
    /// Read local diagnostics snapshot
    Diagnostics,
    /// Tunnel lifecycle controls
    Tunnel {
        #[command(subcommand)]
        command: TunnelCommand,
    },
    /// Idempotent one-shot expose flow (upsert route + ensure tunnel running)
    Expose {
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

        #[arg(long, value_enum, default_value_t = CliTunnelProvider::Cloudflared)]
        provider: CliTunnelProvider,

        #[arg(long, default_value = DEFAULT_GATEWAY_TARGET_URL)]
        target_url: String,

        #[arg(long, default_value_t = true)]
        auto_restart: bool,

        #[arg(
            long,
            default_value_t = false,
            help = "Preview planned actions without applying changes"
        )]
        dry_run: bool,

        #[arg(
            long,
            default_value_t = false,
            help = "Restart tunnel when running provider/target does not match requested values"
        )]
        restart_if_mismatch: bool,

        #[arg(
            long,
            default_value_t = false,
            help = "Wait until tunnel is ready (running with public_base_url)"
        )]
        wait_ready: bool,

        #[arg(
            long,
            default_value_t = 60_000,
            requires = "wait_ready",
            help = "Max wait time for --wait-ready"
        )]
        wait_ready_timeout_ms: u64,

        #[arg(
            long,
            default_value_t = 500,
            requires = "wait_ready",
            help = "Polling interval for --wait-ready"
        )]
        wait_ready_poll_ms: u64,
    },
    /// Idempotent one-shot unexpose flow (remove route + optional tunnel auto-stop)
    Unexpose {
        #[arg(long)]
        id: String,

        #[arg(
            long,
            default_value_t = false,
            help = "Keep tunnel running even when no routes remain"
        )]
        keep_tunnel: bool,

        #[arg(long, default_value_t = false, help = "Treat missing route as success")]
        ignore_missing: bool,

        #[arg(
            long,
            default_value_t = false,
            help = "Preview planned actions without applying changes"
        )]
        dry_run: bool,
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

        #[arg(
            long,
            default_value_t = 1_000,
            help = "poll interval for --follow mode"
        )]
        poll_ms: u64,

        #[command(flatten)]
        retry: StreamRetryArgs,
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
    List {
        #[arg(long, default_value_t = false)]
        watch: bool,

        #[arg(long, default_value_t = false, conflicts_with = "watch")]
        stream: bool,

        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,

        #[command(flatten)]
        retry: StreamRetryArgs,

        #[arg(long, default_value_t = false)]
        table: bool,
    },
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
            default_value_t = false,
            help = "Create route when id does not exist"
        )]
        upsert: bool,

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
    /// Debug route selection by host/path
    Match {
        #[arg(long)]
        path: String,

        #[arg(long)]
        host: Option<String>,

        #[arg(long, default_value_t = false)]
        table: bool,
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

        #[command(flatten)]
        retry: StreamRetryArgs,
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
    /// Reload persisted configuration from disk
    Reload,
}

#[derive(Debug, Clone, Copy, Args)]
struct StreamRetryArgs {
    #[arg(
        long,
        default_value_t = STREAM_RETRY_INITIAL_MS,
        help = "initial reconnect delay for stream mode"
    )]
    stream_retry_initial_ms: u64,

    #[arg(
        long,
        default_value_t = STREAM_RETRY_MAX_MS,
        help = "maximum reconnect delay for stream mode"
    )]
    stream_retry_max_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpstreamsOutputFormat {
    Table,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RoutesOutputFormat {
    Json,
    Table,
}

const STREAM_RETRY_INITIAL_MS: u64 = 500;
const STREAM_RETRY_MAX_MS: u64 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StreamRetryPolicy {
    initial_ms: u64,
    max_ms: u64,
}

impl StreamRetryArgs {
    fn normalize(self) -> anyhow::Result<StreamRetryPolicy> {
        normalize_stream_retry_policy(self.stream_retry_initial_ms, self.stream_retry_max_ms)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    run(Cli::parse()).await
}

async fn wait_for_tunnel_ready(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    timeout_ms: u64,
    poll_ms: u64,
) -> anyhow::Result<TunnelStatusResponse> {
    let start = tokio::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    loop {
        let status: TunnelStatusResponse =
            get_json(client, base_url, "/v1/tunnel/status", token).await?;
        if is_tunnel_ready(&status) {
            return Ok(status);
        }
        if start.elapsed() >= timeout {
            return Err(anyhow!(
                "timed out waiting for tunnel ready after {}ms (state={:?}, public_base_url={:?})",
                timeout_ms,
                status.tunnel.state,
                status.tunnel.public_base_url
            ));
        }
        tokio::time::sleep(Duration::from_millis(poll_ms)).await;
    }
}

async fn stream_logs(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    lines: usize,
    poll_ms: u64,
    retry_policy: StreamRetryPolicy,
) -> anyhow::Result<()> {
    let url = format!("{}/v1/tunnel/logs/stream", base_url);
    let mut retry_delay_ms = retry_policy.initial_ms;

    loop {
        let backoff_after_wait = match stream_logs_once(client, &url, token, lines, poll_ms).await {
            Ok(StreamAttemptOutcome::Stopped) => return Ok(()),
            Ok(StreamAttemptOutcome::Disconnected) => {
                retry_delay_ms = retry_policy.initial_ms;
                eprintln!(
                    "logs stream disconnected; reconnecting in {}ms",
                    retry_delay_ms
                );
                false
            }
            Err(StreamAttemptError::Retryable(error)) => {
                eprintln!(
                    "logs stream interrupted; reconnecting in {}ms: {:#}",
                    retry_delay_ms, error
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

async fn stream_logs_once(
    client: &Client,
    url: &str,
    token: Option<&str>,
    lines: usize,
    poll_ms: u64,
) -> Result<StreamAttemptOutcome, StreamAttemptError> {
    let mut response = request_with_token(client.get(url), token)
        .query(&[
            ("lines", lines.to_string()),
            ("poll_ms", poll_ms.to_string()),
        ])
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
            "HTTP {} while opening logs stream: {}",
            status,
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
                    StreamAttemptError::Retryable(anyhow!(error).context("failed to read logs stream chunk"))
                })?;
                let Some(chunk) = chunk else {
                    return Ok(StreamAttemptOutcome::Disconnected);
                };
                pending.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(line) = take_next_sse_line(&mut pending) {
                    if let Some(frame) = builder.push_line(&line) {
                        render_logs_stream_frame(&frame).map_err(StreamAttemptError::Fatal)?;
                    }
                }
            }
        }
    }
}

async fn watch_status(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
) -> anyhow::Result<()> {
    loop {
        let health: HealthResponse = get_json(client, base_url, "/v1/health", None).await?;
        let tunnel: TunnelStatusResponse =
            get_json(client, base_url, "/v1/tunnel/status", token).await?;
        print!("\x1B[2J\x1B[H");
        println!("{}", format_status_output(&health, &tunnel)?);
        println!();
        println!(
            "status refresh every {}ms, press Ctrl+C to stop",
            interval_ms
        );

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {}
        }
    }
    Ok(())
}

async fn stream_status(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
    retry_policy: StreamRetryPolicy,
) -> anyhow::Result<()> {
    let health: HealthResponse = get_json(client, base_url, "/v1/health", None).await?;
    stream_sse_with_reconnect(
        client,
        base_url,
        token,
        "/v1/tunnel/status/stream",
        interval_ms,
        retry_policy,
        "status",
        |frame| render_status_stream_frame(frame, &health, interval_ms),
    )
    .await
}

async fn watch_routes(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
    format: RoutesOutputFormat,
) -> anyhow::Result<()> {
    loop {
        let routes: RoutesResponse = get_json(client, base_url, "/v1/routes", token).await?;
        print!("\x1B[2J\x1B[H");
        println!("{}", format_routes(&routes, format)?);
        println!();
        println!(
            "routes refresh every {}ms, press Ctrl+C to stop",
            interval_ms
        );

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                break;
            }
            _ = tokio::time::sleep(Duration::from_millis(interval_ms)) => {}
        }
    }
    Ok(())
}

async fn stream_routes(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
    format: RoutesOutputFormat,
    retry_policy: StreamRetryPolicy,
) -> anyhow::Result<()> {
    stream_sse_with_reconnect(
        client,
        base_url,
        token,
        "/v1/routes/stream",
        interval_ms,
        retry_policy,
        "routes",
        |frame| render_routes_stream_frame(frame, interval_ms, format),
    )
    .await
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
    retry_policy: StreamRetryPolicy,
) -> anyhow::Result<()> {
    stream_sse_with_reconnect(
        client,
        base_url,
        token,
        "/v1/metrics/stream",
        interval_ms,
        retry_policy,
        "metrics",
        |frame| render_metrics_stream_frame(frame, interval_ms),
    )
    .await
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
    retry_policy: StreamRetryPolicy,
) -> anyhow::Result<()> {
    stream_sse_with_reconnect(
        client,
        base_url,
        token,
        "/v1/dashboard/stream",
        interval_ms,
        retry_policy,
        "dashboard",
        |frame| render_dashboard_stream_frame(frame, interval_ms),
    )
    .await
}

async fn stream_upstreams_health(
    client: &Client,
    base_url: &str,
    token: Option<&str>,
    interval_ms: u64,
    format: UpstreamsOutputFormat,
    retry_policy: StreamRetryPolicy,
) -> anyhow::Result<()> {
    stream_sse_with_reconnect(
        client,
        base_url,
        token,
        "/v1/upstreams/health/stream",
        interval_ms,
        retry_policy,
        "upstreams",
        |frame| render_upstreams_stream_frame(frame, interval_ms, format),
    )
    .await
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

fn build_control_client(cli: &Cli) -> TunnelmuxControlClient {
    TunnelmuxControlClient::new(ControlClientConfig::new(
        normalize_base_url(&cli.server),
        resolve_api_token(cli.token.clone()),
    ))
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

fn normalize_log_stream_poll_ms(value: u64) -> anyhow::Result<u64> {
    if !(100..=10_000).contains(&value) {
        return Err(anyhow!(
            "poll_ms out of range: expected 100..=10000, got {}",
            value
        ));
    }
    Ok(value)
}

fn normalize_match_route_path(value: String) -> anyhow::Result<String> {
    let path = value.trim();
    if path.is_empty() {
        return Err(anyhow!("path is required"));
    }
    if !path.starts_with('/') {
        return Err(anyhow!("path must start with '/'"));
    }
    Ok(path.to_string())
}

fn should_start_tunnel(status: &TunnelStatusResponse) -> bool {
    !matches!(
        status.tunnel.state,
        TunnelState::Running | TunnelState::Starting
    )
}

fn is_tunnel_ready(status: &TunnelStatusResponse) -> bool {
    if !matches!(status.tunnel.state, TunnelState::Running) {
        return false;
    }
    status
        .tunnel
        .public_base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some()
}

fn should_auto_stop_tunnel_after_unexpose(
    remaining_routes: usize,
    keep_tunnel: bool,
    status: &TunnelStatusResponse,
) -> bool {
    if keep_tunnel || remaining_routes > 0 {
        return false;
    }
    matches!(
        status.tunnel.state,
        TunnelState::Running | TunnelState::Starting
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExposeRouteAction {
    Create,
    Update,
    Unchanged,
}

fn infer_expose_route_action(
    existing_route: Option<&tunnelmux_core::RouteRule>,
    desired_route: &CreateRouteRequest,
) -> ExposeRouteAction {
    let Some(existing_route) = existing_route else {
        return ExposeRouteAction::Create;
    };
    let current = route_rule_to_create_request(existing_route);
    if current == *desired_route {
        ExposeRouteAction::Unchanged
    } else {
        ExposeRouteAction::Update
    }
}

fn expose_route_action_name(action: ExposeRouteAction) -> &'static str {
    match action {
        ExposeRouteAction::Create => "create",
        ExposeRouteAction::Update => "update",
        ExposeRouteAction::Unchanged => "unchanged",
    }
}

fn expose_tunnel_action_name(action: ExposeTunnelActionName) -> &'static str {
    match action {
        ExposeTunnelActionName::Action(ExposeTunnelAction::Noop) => "noop",
        ExposeTunnelActionName::Action(ExposeTunnelAction::Start) => "start",
        ExposeTunnelActionName::Action(ExposeTunnelAction::Restart) => "restart",
        ExposeTunnelActionName::Blocked => "blocked",
    }
}

fn project_remaining_routes_after_unexpose(current_count: usize, route_exists: bool) -> usize {
    if route_exists {
        current_count.saturating_sub(1)
    } else {
        current_count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExposeTunnelAction {
    Noop,
    Start,
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ExposeTunnelActionResolution {
    Action(ExposeTunnelAction),
    Blocked(String),
}

impl ExposeTunnelActionResolution {
    fn action(&self) -> Option<ExposeTunnelAction> {
        match self {
            Self::Action(action) => Some(*action),
            Self::Blocked(_) => None,
        }
    }

    fn action_or_blocked(&self) -> ExposeTunnelActionName {
        match self {
            Self::Action(action) => ExposeTunnelActionName::Action(*action),
            Self::Blocked(_) => ExposeTunnelActionName::Blocked,
        }
    }

    fn blocked_reason(&self) -> Option<&str> {
        match self {
            Self::Action(_) => None,
            Self::Blocked(reason) => Some(reason.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExposeTunnelActionName {
    Action(ExposeTunnelAction),
    Blocked,
}

fn resolve_expose_tunnel_action(
    status: &TunnelStatusResponse,
    desired_provider: &TunnelProvider,
    desired_target_url: &str,
    restart_if_mismatch: bool,
    allow_blocked: bool,
) -> anyhow::Result<ExposeTunnelActionResolution> {
    match decide_expose_tunnel_action(
        status,
        desired_provider,
        desired_target_url,
        restart_if_mismatch,
    ) {
        Ok(action) => Ok(ExposeTunnelActionResolution::Action(action)),
        Err(err) => {
            if allow_blocked {
                Ok(ExposeTunnelActionResolution::Blocked(err.to_string()))
            } else {
                Err(err)
            }
        }
    }
}

fn decide_expose_tunnel_action(
    status: &TunnelStatusResponse,
    desired_provider: &TunnelProvider,
    desired_target_url: &str,
    restart_if_mismatch: bool,
) -> anyhow::Result<ExposeTunnelAction> {
    if should_start_tunnel(status) {
        return Ok(ExposeTunnelAction::Start);
    }

    if tunnel_matches_requested_config(status, desired_provider, desired_target_url) {
        return Ok(ExposeTunnelAction::Noop);
    }

    if restart_if_mismatch {
        return Ok(ExposeTunnelAction::Restart);
    }

    Err(anyhow!(
        "tunnel is running with different config (current provider={:?}, target_url={:?}); rerun with --restart-if-mismatch or align --provider/--target-url",
        status.tunnel.provider,
        status.tunnel.target_url
    ))
}

fn tunnel_matches_requested_config(
    status: &TunnelStatusResponse,
    desired_provider: &TunnelProvider,
    desired_target_url: &str,
) -> bool {
    if status.tunnel.provider.as_ref() != Some(desired_provider) {
        return false;
    }
    let Some(current_target_url) = status.tunnel.target_url.as_deref() else {
        return false;
    };
    normalize_target_url_for_compare(current_target_url)
        == normalize_target_url_for_compare(desired_target_url)
}

fn normalize_target_url_for_compare(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn normalize_wait_ready_timeout_ms(value: u64) -> anyhow::Result<u64> {
    if !(500..=300_000).contains(&value) {
        return Err(anyhow!(
            "wait_ready_timeout_ms out of range: expected 500..=300000, got {}",
            value
        ));
    }
    Ok(value)
}

fn normalize_wait_ready_poll_ms(value: u64) -> anyhow::Result<u64> {
    normalize_log_stream_poll_ms(value)
}

fn normalize_stream_retry_policy(
    initial_ms: u64,
    max_ms: u64,
) -> anyhow::Result<StreamRetryPolicy> {
    let initial_ms = normalize_stream_retry_delay_ms("stream_retry_initial_ms", initial_ms)?;
    let max_ms = normalize_stream_retry_delay_ms("stream_retry_max_ms", max_ms)?;
    if initial_ms > max_ms {
        return Err(anyhow!(
            "stream retry range invalid: stream_retry_initial_ms ({}) must be <= stream_retry_max_ms ({})",
            initial_ms,
            max_ms
        ));
    }
    Ok(StreamRetryPolicy { initial_ms, max_ms })
}

fn normalize_stream_retry_delay_ms(name: &str, value: u64) -> anyhow::Result<u64> {
    if !(100..=60_000).contains(&value) {
        return Err(anyhow!(
            "{} out of range: expected 100..=60000, got {}",
            name,
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
    fn parses_settings_reload_command() {
        let cli = Cli::try_parse_from(["tunnelmux", "settings", "reload"])
            .expect("settings reload should parse");

        assert!(matches!(
            cli.command,
            Command::Settings {
                command: SettingsCommand::Reload
            }
        ));
    }

    #[test]
    fn parses_diagnostics_command() {
        assert!(Cli::try_parse_from(["tunnelmux", "diagnostics"]).is_ok());
    }

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
    fn normalize_log_stream_poll_ms_applies_bounds() {
        assert_eq!(normalize_log_stream_poll_ms(100).expect("min"), 100);
        assert_eq!(normalize_log_stream_poll_ms(1_000).expect("default"), 1_000);
        assert_eq!(normalize_log_stream_poll_ms(10_000).expect("max"), 10_000);
    }

    #[test]
    fn normalize_log_stream_poll_ms_rejects_out_of_range() {
        assert!(normalize_log_stream_poll_ms(99).is_err());
        assert!(normalize_log_stream_poll_ms(10_001).is_err());
    }

    #[test]
    fn normalize_match_route_path_requires_leading_slash() {
        assert_eq!(
            normalize_match_route_path("/api/v1".to_string()).expect("valid path"),
            "/api/v1"
        );
        assert!(normalize_match_route_path("".to_string()).is_err());
        assert!(normalize_match_route_path("api/v1".to_string()).is_err());
    }

    #[test]
    fn normalize_stream_retry_policy_applies_bounds() {
        let policy = normalize_stream_retry_policy(100, 60_000).expect("valid policy");
        assert_eq!(
            policy,
            StreamRetryPolicy {
                initial_ms: 100,
                max_ms: 60_000,
            }
        );
    }

    #[test]
    fn normalize_stream_retry_policy_rejects_invalid_values() {
        assert!(normalize_stream_retry_policy(99, 10_000).is_err());
        assert!(normalize_stream_retry_policy(500, 60_001).is_err());
        assert!(normalize_stream_retry_policy(2_000, 1_000).is_err());
    }

    #[test]
    fn next_stream_retry_delay_ms_doubles_and_caps() {
        assert_eq!(
            next_stream_retry_delay_ms(
                STREAM_RETRY_INITIAL_MS,
                StreamRetryPolicy {
                    initial_ms: STREAM_RETRY_INITIAL_MS,
                    max_ms: STREAM_RETRY_MAX_MS
                }
            ),
            STREAM_RETRY_INITIAL_MS * 2
        );
        assert_eq!(
            next_stream_retry_delay_ms(
                STREAM_RETRY_MAX_MS,
                StreamRetryPolicy {
                    initial_ms: STREAM_RETRY_INITIAL_MS,
                    max_ms: STREAM_RETRY_MAX_MS
                }
            ),
            STREAM_RETRY_MAX_MS
        );
        assert_eq!(
            next_stream_retry_delay_ms(
                STREAM_RETRY_MAX_MS * 10,
                StreamRetryPolicy {
                    initial_ms: STREAM_RETRY_INITIAL_MS,
                    max_ms: STREAM_RETRY_MAX_MS
                }
            ),
            STREAM_RETRY_MAX_MS
        );
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

    #[test]
    fn format_routes_outputs_table() {
        let response = RoutesResponse {
            routes: vec![
                tunnelmux_core::RouteRule {
                    id: "svc-a".to_string(),
                    match_host: Some("demo.local".to_string()),
                    match_path_prefix: Some("/api".to_string()),
                    strip_path_prefix: Some("/api".to_string()),
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
                    health_check_path: Some("/healthz".to_string()),
                    enabled: true,
                },
                tunnelmux_core::RouteRule {
                    id: "svc-b".to_string(),
                    match_host: None,
                    match_path_prefix: None,
                    strip_path_prefix: None,
                    upstream_url: "http://127.0.0.1:4000".to_string(),
                    fallback_upstream_url: None,
                    health_check_path: None,
                    enabled: false,
                },
            ],
        };

        let rendered =
            format_routes(&response, RoutesOutputFormat::Table).expect("table format should work");
        assert!(rendered.contains("ID"));
        assert!(rendered.contains("FALLBACK_UPSTREAM_URL"));
        assert!(rendered.contains("svc-a"));
        assert!(rendered.contains("svc-b"));
        assert!(rendered.contains("true"));
        assert!(rendered.contains("false"));
    }

    #[test]
    fn format_route_match_table_contains_summary_and_targets() {
        let response = RouteMatchResponse {
            host: Some("demo.local".to_string()),
            path: "/api/v1/ping".to_string(),
            matched: true,
            route: Some(tunnelmux_core::RouteRule {
                id: "svc-api".to_string(),
                match_host: Some("demo.local".to_string()),
                match_path_prefix: Some("/api".to_string()),
                strip_path_prefix: Some("/api".to_string()),
                upstream_url: "http://127.0.0.1:3000".to_string(),
                fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
                health_check_path: Some("/ready".to_string()),
                enabled: true,
            }),
            forwarded_path: Some("/v1/ping".to_string()),
            health_check_path: Some("/ready".to_string()),
            targets: vec![
                tunnelmux_core::RouteMatchTarget {
                    upstream_url: "http://127.0.0.1:3001".to_string(),
                    healthy: Some(true),
                    last_checked_at: Some("2026-03-05T12:00:00Z".to_string()),
                    last_error: None,
                },
                tunnelmux_core::RouteMatchTarget {
                    upstream_url: "http://127.0.0.1:3000".to_string(),
                    healthy: Some(false),
                    last_checked_at: Some("2026-03-05T12:00:01Z".to_string()),
                    last_error: Some("status 503".to_string()),
                },
            ],
        };

        let rendered = format_route_match_table(&response);
        assert!(rendered.contains("MATCHED"));
        assert!(rendered.contains("ROUTE_ID"));
        assert!(rendered.contains("svc-api"));
        assert!(rendered.contains("TARGETS"));
        assert!(rendered.contains("http://127.0.0.1:3001"));
        assert!(rendered.contains("healthy"));
        assert!(rendered.contains("unhealthy"));
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

    #[test]
    fn format_status_output_contains_health_and_tunnel_fields() {
        let health = HealthResponse {
            ok: true,
            service: "tunnelmuxd".to_string(),
            version: "0.1.0".to_string(),
        };
        let tunnel = TunnelStatusResponse {
            tunnel: tunnelmux_core::TunnelStatus {
                state: tunnelmux_core::TunnelState::Running,
                provider: Some(tunnelmux_core::TunnelProvider::Cloudflared),
                target_url: Some("http://127.0.0.1:18080".to_string()),
                public_base_url: Some("https://example.com".to_string()),
                started_at: Some("2026-03-05T00:00:00Z".to_string()),
                updated_at: "2026-03-05T00:00:01Z".to_string(),
                process_id: Some(1234),
                auto_restart: true,
                restart_count: 0,
                last_error: None,
            },
        };

        let rendered = format_status_output(&health, &tunnel).expect("render status output");
        assert!(rendered.contains("\"health\""));
        assert!(rendered.contains("\"tunnel\""));
        assert!(rendered.contains("\"state\": \"running\""));
    }

    #[test]
    fn render_routes_stream_frame_accepts_snapshot_event() {
        let frame = SseFrame {
            event: "snapshot".to_string(),
            data: r#"{"routes":[{"id":"svc-a","match_host":"demo.local","match_path_prefix":"/","strip_path_prefix":null,"upstream_url":"http://127.0.0.1:3000","fallback_upstream_url":null,"health_check_path":null,"enabled":true}]}"#.to_string(),
        };
        let result = render_routes_stream_frame(&frame, 2000, RoutesOutputFormat::Json);
        assert!(result.is_ok());
    }

    #[test]
    fn should_start_tunnel_returns_false_for_running_or_starting_states() {
        assert!(!should_start_tunnel(&sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Running,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        )));
        assert!(!should_start_tunnel(&sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Starting,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        )));
    }

    #[test]
    fn should_start_tunnel_returns_true_for_idle_stopped_or_error_states() {
        assert!(should_start_tunnel(&sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Idle,
            None,
            None,
        )));
        assert!(should_start_tunnel(&sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Stopped,
            None,
            None,
        )));
        assert!(should_start_tunnel(&sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Error,
            None,
            None,
        )));
    }

    #[test]
    fn decide_expose_tunnel_action_starts_when_tunnel_not_running() {
        let status = sample_tunnel_status_response(tunnelmux_core::TunnelState::Idle, None, None);
        let action = decide_expose_tunnel_action(
            &status,
            &tunnelmux_core::TunnelProvider::Cloudflared,
            "http://127.0.0.1:18080",
            false,
        )
        .expect("decision should succeed");
        assert_eq!(action, ExposeTunnelAction::Start);
    }

    #[test]
    fn decide_expose_tunnel_action_noop_when_running_and_matching() {
        let status = sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Running,
            Some(tunnelmux_core::TunnelProvider::Ngrok),
            Some("http://127.0.0.1:18080/"),
        );
        let action = decide_expose_tunnel_action(
            &status,
            &tunnelmux_core::TunnelProvider::Ngrok,
            "http://127.0.0.1:18080",
            false,
        )
        .expect("decision should succeed");
        assert_eq!(action, ExposeTunnelAction::Noop);
    }

    #[test]
    fn decide_expose_tunnel_action_errors_when_running_but_mismatch_without_restart_flag() {
        let status = sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Running,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        );
        let result = decide_expose_tunnel_action(
            &status,
            &tunnelmux_core::TunnelProvider::Ngrok,
            "http://127.0.0.1:18080",
            false,
        );
        assert!(result.is_err());
    }

    #[test]
    fn decide_expose_tunnel_action_restarts_when_running_mismatch_and_restart_enabled() {
        let status = sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Running,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        );
        let action = decide_expose_tunnel_action(
            &status,
            &tunnelmux_core::TunnelProvider::Ngrok,
            "http://127.0.0.1:18080",
            true,
        )
        .expect("decision should succeed");
        assert_eq!(action, ExposeTunnelAction::Restart);
    }

    #[test]
    fn resolve_expose_tunnel_action_blocks_in_dry_run_when_mismatch_without_restart() {
        let status = sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Running,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        );
        let resolved = resolve_expose_tunnel_action(
            &status,
            &tunnelmux_core::TunnelProvider::Ngrok,
            "http://127.0.0.1:18080",
            false,
            true,
        )
        .expect("dry run should capture blocked action");
        assert!(matches!(resolved, ExposeTunnelActionResolution::Blocked(_)));
    }

    #[test]
    fn resolve_expose_tunnel_action_errors_when_apply_mode_is_blocked() {
        let status = sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Running,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        );
        let resolved = resolve_expose_tunnel_action(
            &status,
            &tunnelmux_core::TunnelProvider::Ngrok,
            "http://127.0.0.1:18080",
            false,
            false,
        );
        assert!(resolved.is_err());
    }

    #[test]
    fn should_auto_stop_tunnel_after_unexpose_requires_no_routes_and_active_tunnel() {
        let active = sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Running,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        );
        assert!(should_auto_stop_tunnel_after_unexpose(0, false, &active));
        assert!(!should_auto_stop_tunnel_after_unexpose(1, false, &active));
    }

    #[test]
    fn should_auto_stop_tunnel_after_unexpose_respects_keep_tunnel_and_state() {
        let active = sample_tunnel_status_response(
            tunnelmux_core::TunnelState::Starting,
            Some(tunnelmux_core::TunnelProvider::Cloudflared),
            Some("http://127.0.0.1:18080"),
        );
        let inactive =
            sample_tunnel_status_response(tunnelmux_core::TunnelState::Stopped, None, None);
        assert!(!should_auto_stop_tunnel_after_unexpose(0, true, &active));
        assert!(!should_auto_stop_tunnel_after_unexpose(0, false, &inactive));
    }

    #[test]
    fn is_tunnel_ready_requires_running_state_and_public_url() {
        let running_ready = TunnelStatusResponse {
            tunnel: tunnelmux_core::TunnelStatus {
                state: tunnelmux_core::TunnelState::Running,
                provider: Some(tunnelmux_core::TunnelProvider::Cloudflared),
                target_url: Some("http://127.0.0.1:18080".to_string()),
                public_base_url: Some("https://demo.trycloudflare.com".to_string()),
                started_at: Some("2026-03-05T00:00:00Z".to_string()),
                updated_at: "2026-03-05T00:00:01Z".to_string(),
                process_id: Some(42),
                auto_restart: true,
                restart_count: 0,
                last_error: None,
            },
        };
        assert!(is_tunnel_ready(&running_ready));

        let running_without_public = TunnelStatusResponse {
            tunnel: tunnelmux_core::TunnelStatus {
                public_base_url: None,
                ..running_ready.tunnel.clone()
            },
        };
        assert!(!is_tunnel_ready(&running_without_public));

        let starting_with_public = TunnelStatusResponse {
            tunnel: tunnelmux_core::TunnelStatus {
                state: tunnelmux_core::TunnelState::Starting,
                ..running_ready.tunnel
            },
        };
        assert!(!is_tunnel_ready(&starting_with_public));
    }

    #[test]
    fn normalize_wait_ready_timeout_ms_applies_bounds() {
        assert_eq!(normalize_wait_ready_timeout_ms(500).expect("min"), 500);
        assert_eq!(
            normalize_wait_ready_timeout_ms(30_000).expect("normal"),
            30_000
        );
        assert_eq!(
            normalize_wait_ready_timeout_ms(300_000).expect("max"),
            300_000
        );
    }

    #[test]
    fn normalize_wait_ready_timeout_ms_rejects_out_of_range() {
        assert!(normalize_wait_ready_timeout_ms(499).is_err());
        assert!(normalize_wait_ready_timeout_ms(300_001).is_err());
    }

    #[test]
    fn normalize_wait_ready_poll_ms_applies_bounds() {
        assert_eq!(normalize_wait_ready_poll_ms(100).expect("min"), 100);
        assert_eq!(normalize_wait_ready_poll_ms(1_000).expect("normal"), 1_000);
        assert_eq!(normalize_wait_ready_poll_ms(10_000).expect("max"), 10_000);
    }

    #[test]
    fn normalize_wait_ready_poll_ms_rejects_out_of_range() {
        assert!(normalize_wait_ready_poll_ms(99).is_err());
        assert!(normalize_wait_ready_poll_ms(10_001).is_err());
    }

    #[test]
    fn infer_expose_route_action_classifies_create_update_and_unchanged() {
        let desired = CreateRouteRequest {
            id: "svc-a".to_string(),
            match_host: Some("demo.local".to_string()),
            match_path_prefix: Some("/api".to_string()),
            strip_path_prefix: Some("/api".to_string()),
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: Some("/healthz".to_string()),
            enabled: Some(true),
        };

        assert_eq!(
            infer_expose_route_action(None, &desired),
            ExposeRouteAction::Create
        );

        let unchanged = tunnelmux_core::RouteRule {
            id: "svc-a".to_string(),
            match_host: Some("demo.local".to_string()),
            match_path_prefix: Some("/api".to_string()),
            strip_path_prefix: Some("/api".to_string()),
            upstream_url: "http://127.0.0.1:3000".to_string(),
            fallback_upstream_url: Some("http://127.0.0.1:3001".to_string()),
            health_check_path: Some("/healthz".to_string()),
            enabled: true,
        };
        assert_eq!(
            infer_expose_route_action(Some(&unchanged), &desired),
            ExposeRouteAction::Unchanged
        );

        let changed = tunnelmux_core::RouteRule {
            upstream_url: "http://127.0.0.1:3010".to_string(),
            ..unchanged
        };
        assert_eq!(
            infer_expose_route_action(Some(&changed), &desired),
            ExposeRouteAction::Update
        );
    }

    #[test]
    fn project_remaining_routes_after_unexpose_decrements_only_when_route_exists() {
        assert_eq!(project_remaining_routes_after_unexpose(3, true), 2);
        assert_eq!(project_remaining_routes_after_unexpose(3, false), 3);
        assert_eq!(project_remaining_routes_after_unexpose(0, false), 0);
        assert_eq!(project_remaining_routes_after_unexpose(0, true), 0);
    }

    fn sample_tunnel_status_response(
        state: tunnelmux_core::TunnelState,
        provider: Option<tunnelmux_core::TunnelProvider>,
        target_url: Option<&str>,
    ) -> TunnelStatusResponse {
        TunnelStatusResponse {
            tunnel: tunnelmux_core::TunnelStatus {
                state,
                provider,
                target_url: target_url.map(str::to_string),
                public_base_url: None,
                started_at: None,
                updated_at: "2026-03-05T00:00:00Z".to_string(),
                process_id: None,
                auto_restart: true,
                restart_count: 0,
                last_error: None,
            },
        }
    }
}

#[cfg(test)]
mod shared_client_refactor_tests {
    use super::*;

    #[test]
    fn build_control_client_normalizes_server_and_token() {
        let cli = Cli::try_parse_from([
            "tunnelmux",
            "--server",
            "127.0.0.1:4765/",
            "--token",
            "  dev-token  ",
            "diagnostics",
        ])
        .expect("cli should parse");

        let client = build_control_client(&cli);
        assert_eq!(client.base_url(), "http://127.0.0.1:4765");
        assert_eq!(client.token(), Some("dev-token"));
    }
}
