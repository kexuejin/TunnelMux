use std::collections::HashSet;
use std::io::{self, Read};
use std::time::Duration;
use std::{fs, path::Path, path::PathBuf};

use anyhow::{Context, anyhow};
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::{Client, StatusCode as ReqwestStatusCode};
use serde_json::json;
use tunnelmux_core::{
    ApplyRoutesRequest, ApplyRoutesResponse, CreateRouteRequest, DEFAULT_CONTROL_ADDR,
    DEFAULT_GATEWAY_TARGET_URL, DashboardResponse, DeleteRouteResponse, ErrorResponse,
    HealthCheckSettingsResponse, HealthResponse, MetricsResponse, RouteMatchResponse,
    RoutesResponse, TunnelLogsResponse, TunnelProvider, TunnelStartRequest, TunnelState,
    TunnelStatusResponse, UpdateHealthCheckSettingsRequest, UpstreamsHealthResponse,
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
    let cli = Cli::parse();
    let base_url = normalize_base_url(&cli.server);
    let token = resolve_api_token(cli.token);
    let client = Client::new();

    match cli.command {
        Command::Status {
            watch,
            stream,
            interval_ms,
            retry,
        } => {
            let interval_ms = normalize_watch_interval_ms(interval_ms)?;
            if stream {
                let retry_policy = retry.normalize()?;
                stream_status(
                    &client,
                    &base_url,
                    token.as_deref(),
                    interval_ms,
                    retry_policy,
                )
                .await?;
            } else if watch {
                watch_status(&client, &base_url, token.as_deref(), interval_ms).await?;
            } else {
                let health: HealthResponse =
                    get_json(&client, &base_url, "/v1/health", None).await?;
                let tunnel: TunnelStatusResponse =
                    get_json(&client, &base_url, "/v1/tunnel/status", token.as_deref()).await?;
                println!("{}", format_status_output(&health, &tunnel)?);
            }
        }
        Command::Dashboard {
            watch,
            stream,
            interval_ms,
            retry,
        } => {
            let interval_ms = normalize_watch_interval_ms(interval_ms)?;
            if stream {
                let retry_policy = retry.normalize()?;
                stream_dashboard(
                    &client,
                    &base_url,
                    token.as_deref(),
                    interval_ms,
                    retry_policy,
                )
                .await?;
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
            retry,
        } => {
            let interval_ms = normalize_watch_interval_ms(interval_ms)?;
            if stream {
                let retry_policy = retry.normalize()?;
                stream_metrics(
                    &client,
                    &base_url,
                    token.as_deref(),
                    interval_ms,
                    retry_policy,
                )
                .await?;
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
            TunnelCommand::Logs {
                lines,
                follow,
                poll_ms,
                retry,
            } => {
                if follow {
                    let poll_ms = normalize_log_stream_poll_ms(poll_ms)?;
                    let retry_policy = retry.normalize()?;
                    stream_logs(
                        &client,
                        &base_url,
                        token.as_deref(),
                        lines,
                        poll_ms,
                        retry_policy,
                    )
                    .await?;
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
        Command::Expose {
            id,
            upstream_url,
            fallback_upstream_url,
            health_check_path,
            host,
            path_prefix,
            strip_path_prefix,
            disabled,
            provider,
            target_url,
            auto_restart,
            dry_run,
            restart_if_mismatch,
            wait_ready,
            wait_ready_timeout_ms,
            wait_ready_poll_ms,
        } => {
            let provider: TunnelProvider = provider.into();
            let route_payload = CreateRouteRequest {
                id: id.clone(),
                match_host: host,
                match_path_prefix: path_prefix,
                strip_path_prefix,
                upstream_url,
                fallback_upstream_url,
                health_check_path,
                enabled: Some(!disabled),
            };
            let routes: RoutesResponse =
                get_json(&client, &base_url, "/v1/routes", token.as_deref()).await?;
            let route_exists = routes.routes.iter().any(|item| item.id == id);
            let route_endpoint = build_route_update_endpoint(&id, true);
            let mut tunnel: TunnelStatusResponse =
                get_json(&client, &base_url, "/v1/tunnel/status", token.as_deref()).await?;
            let tunnel_action = resolve_expose_tunnel_action(
                &tunnel,
                &provider,
                &target_url,
                restart_if_mismatch,
                dry_run,
            )?;
            let (wait_ready_timeout_ms, wait_ready_poll_ms) = if wait_ready {
                (
                    normalize_wait_ready_timeout_ms(wait_ready_timeout_ms)?,
                    normalize_wait_ready_poll_ms(wait_ready_poll_ms)?,
                )
            } else {
                (wait_ready_timeout_ms, wait_ready_poll_ms)
            };
            if dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "dry_run": true,
                        "route_action": infer_expose_route_upsert_action(route_exists),
                        "tunnel_action": expose_tunnel_action_name(tunnel_action.action_or_blocked()),
                        "tunnel_action_error": tunnel_action.blocked_reason(),
                        "would_wait_ready": wait_ready,
                        "wait_ready_timeout_ms": wait_ready_timeout_ms,
                        "wait_ready_poll_ms": wait_ready_poll_ms,
                        "current_tunnel": tunnel.tunnel,
                    }))?
                );
            } else {
                let tunnel_action = tunnel_action
                    .action()
                    .ok_or_else(|| anyhow!("unexpected blocked expose action in apply mode"))?;
                let route: tunnelmux_core::RouteRule = put_json(
                    &client,
                    &base_url,
                    &route_endpoint,
                    &route_payload,
                    token.as_deref(),
                )
                .await?;

                let mut tunnel_started = false;
                let mut tunnel_restarted = false;
                match tunnel_action {
                    ExposeTunnelAction::Noop => {}
                    ExposeTunnelAction::Start => {
                        tunnel = post_json(
                            &client,
                            &base_url,
                            "/v1/tunnel/start",
                            &TunnelStartRequest {
                                provider: provider.clone(),
                                target_url: target_url.clone(),
                                auto_restart: Some(auto_restart),
                                metadata: None,
                            },
                            token.as_deref(),
                        )
                        .await?;
                        tunnel_started = true;
                    }
                    ExposeTunnelAction::Restart => {
                        let _stopped: TunnelStatusResponse = post_json(
                            &client,
                            &base_url,
                            "/v1/tunnel/stop",
                            &json!({}),
                            token.as_deref(),
                        )
                        .await?;
                        tunnel = post_json(
                            &client,
                            &base_url,
                            "/v1/tunnel/start",
                            &TunnelStartRequest {
                                provider: provider.clone(),
                                target_url: target_url.clone(),
                                auto_restart: Some(auto_restart),
                                metadata: None,
                            },
                            token.as_deref(),
                        )
                        .await?;
                        tunnel_started = true;
                        tunnel_restarted = true;
                    }
                };

                let mut waited_ready = false;
                if wait_ready {
                    tunnel = wait_for_tunnel_ready(
                        &client,
                        &base_url,
                        token.as_deref(),
                        wait_ready_timeout_ms,
                        wait_ready_poll_ms,
                    )
                    .await?;
                    waited_ready = true;
                }

                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "route": route,
                        "tunnel": tunnel.tunnel,
                        "tunnel_started": tunnel_started,
                        "tunnel_restarted": tunnel_restarted,
                        "waited_ready": waited_ready,
                    }))?
                );
            }
        }
        Command::Unexpose {
            id,
            keep_tunnel,
            ignore_missing,
            dry_run,
        } => {
            let routes: RoutesResponse =
                get_json(&client, &base_url, "/v1/routes", token.as_deref()).await?;
            let route_exists = routes.routes.iter().any(|item| item.id == id);
            if !route_exists && !ignore_missing {
                return Err(anyhow!("route '{}' not found", id));
            }
            let mut tunnel: TunnelStatusResponse =
                get_json(&client, &base_url, "/v1/tunnel/status", token.as_deref()).await?;
            let remaining_routes =
                project_remaining_routes_after_unexpose(routes.routes.len(), route_exists);
            let tunnel_stopped =
                should_auto_stop_tunnel_after_unexpose(remaining_routes, keep_tunnel, &tunnel);
            if dry_run {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "dry_run": true,
                        "route_exists": route_exists,
                        "route_removed": route_exists,
                        "remaining_routes": remaining_routes,
                        "tunnel_stopped": tunnel_stopped,
                        "current_tunnel": tunnel.tunnel,
                    }))?
                );
            } else {
                let remove =
                    delete_route_by_id(&client, &base_url, &id, token.as_deref(), ignore_missing)
                        .await?;
                if tunnel_stopped {
                    tunnel = post_json(
                        &client,
                        &base_url,
                        "/v1/tunnel/stop",
                        &json!({}),
                        token.as_deref(),
                    )
                    .await?;
                }
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "route_removed": remove.removed,
                        "remaining_routes": remaining_routes,
                        "tunnel_stopped": tunnel_stopped,
                        "tunnel": tunnel.tunnel,
                    }))?
                );
            }
        }
        Command::Routes { command } => match command {
            RoutesCommand::List {
                watch,
                stream,
                interval_ms,
                retry,
                table,
            } => {
                let format = if table {
                    RoutesOutputFormat::Table
                } else {
                    RoutesOutputFormat::Json
                };
                let interval_ms = normalize_watch_interval_ms(interval_ms)?;
                if stream {
                    let retry_policy = retry.normalize()?;
                    stream_routes(
                        &client,
                        &base_url,
                        token.as_deref(),
                        interval_ms,
                        format,
                        retry_policy,
                    )
                    .await?;
                } else if watch {
                    watch_routes(&client, &base_url, token.as_deref(), interval_ms, format).await?;
                } else {
                    let routes: RoutesResponse =
                        get_json(&client, &base_url, "/v1/routes", token.as_deref()).await?;
                    println!("{}", format_routes(&routes, format)?);
                }
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
            RoutesCommand::Match { path, host, table } => {
                let path = normalize_match_route_path(path)?;
                let url = format!("{}/v1/routes/match", base_url);
                let mut request = request_with_token(client.get(&url), token.as_deref());
                request = request.query(&[("path", path.as_str())]);
                if let Some(host) = host
                    .as_deref()
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                {
                    request = request.query(&[("host", host)]);
                }
                let response = request
                    .send()
                    .await
                    .with_context(|| format!("request failed: {url}"))?;
                let payload: RouteMatchResponse = decode_response(response).await?;
                if table {
                    println!("{}", format_route_match_table(&payload));
                } else {
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                }
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
                upsert,
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
                let endpoint = build_route_update_endpoint(&id, upsert);
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
                retry,
            } => {
                let format = if json {
                    UpstreamsOutputFormat::Json
                } else {
                    UpstreamsOutputFormat::Table
                };
                let interval_ms = normalize_watch_interval_ms(interval_ms)?;
                if stream {
                    let retry_policy = retry.normalize()?;
                    stream_upstreams_health(
                        &client,
                        &base_url,
                        token.as_deref(),
                        interval_ms,
                        format,
                        retry_policy,
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

async fn delete_route_by_id(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamAttemptOutcome {
    Stopped,
    Disconnected,
}

#[derive(Debug)]
enum StreamAttemptError {
    Retryable(anyhow::Error),
    Fatal(anyhow::Error),
}

async fn stream_sse_with_reconnect<F>(
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

async fn stream_sse_once<F>(
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

async fn wait_before_stream_retry(delay_ms: u64) -> anyhow::Result<bool> {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Ok(true),
        _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => Ok(false),
    }
}

fn next_stream_retry_delay_ms(current_delay_ms: u64, retry_policy: StreamRetryPolicy) -> u64 {
    current_delay_ms
        .saturating_mul(2)
        .clamp(retry_policy.initial_ms, retry_policy.max_ms)
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

fn format_status_output(
    health: &HealthResponse,
    tunnel: &TunnelStatusResponse,
) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(&json!({
        "health": health,
        "tunnel": tunnel.tunnel,
    }))?)
}

fn render_status_stream_frame(
    frame: &SseFrame,
    health: &HealthResponse,
    interval_ms: u64,
) -> anyhow::Result<()> {
    match frame.event.as_str() {
        "snapshot" => {
            let snapshot: TunnelStatusResponse =
                serde_json::from_str(&frame.data).with_context(|| {
                    format!(
                        "failed to parse tunnel status snapshot event: {}",
                        frame.data
                    )
                })?;
            print!("\x1B[2J\x1B[H");
            println!("{}", format_status_output(health, &snapshot)?);
            println!();
            println!(
                "status stream interval {}ms, press Ctrl+C to stop",
                interval_ms
            );
        }
        "error" => {
            eprintln!("status stream error: {}", frame.data);
        }
        _ => {}
    }
    Ok(())
}

fn render_logs_stream_frame(frame: &SseFrame) -> anyhow::Result<()> {
    match frame.event.as_str() {
        "line" | "message" => println!("{}", frame.data),
        "error" => {
            eprintln!("logs stream error: {}", frame.data);
        }
        _ => {}
    }
    Ok(())
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

fn render_routes_stream_frame(
    frame: &SseFrame,
    interval_ms: u64,
    format: RoutesOutputFormat,
) -> anyhow::Result<()> {
    match frame.event.as_str() {
        "snapshot" => {
            let snapshot: RoutesResponse =
                serde_json::from_str(&frame.data).with_context(|| {
                    format!("failed to parse routes snapshot event: {}", frame.data)
                })?;
            print!("\x1B[2J\x1B[H");
            println!("{}", format_routes(&snapshot, format)?);
            println!();
            println!(
                "routes stream interval {}ms, press Ctrl+C to stop",
                interval_ms
            );
        }
        "error" => {
            eprintln!("routes stream error: {}", frame.data);
        }
        _ => {}
    }
    Ok(())
}

fn format_routes(response: &RoutesResponse, format: RoutesOutputFormat) -> anyhow::Result<String> {
    match format {
        RoutesOutputFormat::Json => Ok(serde_json::to_string_pretty(response)?),
        RoutesOutputFormat::Table => Ok(render_routes_table(response)),
    }
}

fn render_routes_table(response: &RoutesResponse) -> String {
    let headers = [
        "ID",
        "HOST",
        "PATH_PREFIX",
        "STRIP_PREFIX",
        "UPSTREAM_URL",
        "FALLBACK_UPSTREAM_URL",
        "ENABLED",
    ];
    let mut rows = Vec::with_capacity(response.routes.len());
    for route in &response.routes {
        rows.push(vec![
            truncate_cell(&route.id, 24),
            truncate_cell(route.match_host.as_deref().unwrap_or("*"), 24),
            truncate_cell(route.match_path_prefix.as_deref().unwrap_or("/"), 18),
            truncate_cell(route.strip_path_prefix.as_deref().unwrap_or("-"), 18),
            truncate_cell(&route.upstream_url, 48),
            truncate_cell(route.fallback_upstream_url.as_deref().unwrap_or("-"), 48),
            if route.enabled { "true" } else { "false" }.to_string(),
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
    output.push_str(&format_routes_table_separator(&widths));
    output.push('\n');
    output.push_str(&format_routes_table_row(&headers, &widths));
    output.push('\n');
    output.push_str(&format_routes_table_separator(&widths));
    for row in &rows {
        output.push('\n');
        output.push_str(&format_routes_table_row(
            &[
                &row[0], &row[1], &row[2], &row[3], &row[4], &row[5], &row[6],
            ],
            &widths,
        ));
    }
    output.push('\n');
    output.push_str(&format_routes_table_separator(&widths));
    output
}

fn format_routes_table_separator(widths: &[usize; 7]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

fn format_routes_table_row(values: &[&str; 7], widths: &[usize; 7]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
}

fn format_route_match_table(response: &RouteMatchResponse) -> String {
    let headers = ["FIELD", "VALUE"];
    let route_id = response
        .route
        .as_ref()
        .map(|item| item.id.as_str())
        .unwrap_or("-");
    let summary_rows = vec![
        vec!["MATCHED".to_string(), response.matched.to_string()],
        vec![
            "HOST".to_string(),
            response
                .host
                .as_deref()
                .filter(|item| !item.is_empty())
                .unwrap_or("-")
                .to_string(),
        ],
        vec!["PATH".to_string(), truncate_cell(&response.path, 72)],
        vec!["ROUTE_ID".to_string(), truncate_cell(route_id, 40)],
        vec![
            "FORWARDED_PATH".to_string(),
            truncate_cell(response.forwarded_path.as_deref().unwrap_or("-"), 72),
        ],
        vec![
            "HEALTH_CHECK_PATH".to_string(),
            truncate_cell(response.health_check_path.as_deref().unwrap_or("-"), 48),
        ],
    ];

    let mut widths = headers.map(str::len);
    for row in &summary_rows {
        for (index, cell) in row.iter().enumerate() {
            if cell.len() > widths[index] {
                widths[index] = cell.len();
            }
        }
    }

    let mut output = String::new();
    output.push_str(&format_kv_table_separator(&widths));
    output.push('\n');
    output.push_str(&format_kv_table_row(&headers, &widths));
    output.push('\n');
    output.push_str(&format_kv_table_separator(&widths));
    for row in &summary_rows {
        output.push('\n');
        output.push_str(&format_kv_table_row(&[&row[0], &row[1]], &widths));
    }
    output.push('\n');
    output.push_str(&format_kv_table_separator(&widths));

    output.push_str("\n\nTARGETS\n");
    let target_headers = ["UPSTREAM_URL", "HEALTH", "LAST_CHECKED_AT", "LAST_ERROR"];
    if response.targets.is_empty() {
        output.push_str("(none)");
        return output;
    }

    let mut target_rows = Vec::with_capacity(response.targets.len());
    for target in &response.targets {
        target_rows.push(vec![
            truncate_cell(&target.upstream_url, 56),
            upstream_health_label(target.healthy).to_string(),
            target
                .last_checked_at
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            truncate_cell(target.last_error.as_deref().unwrap_or("-"), 72),
        ]);
    }

    let mut target_widths = target_headers.map(str::len);
    for row in &target_rows {
        for (index, cell) in row.iter().enumerate() {
            if cell.len() > target_widths[index] {
                target_widths[index] = cell.len();
            }
        }
    }

    output.push_str(&format_targets_table_separator(&target_widths));
    output.push('\n');
    output.push_str(&format_targets_table_row(&target_headers, &target_widths));
    output.push('\n');
    output.push_str(&format_targets_table_separator(&target_widths));
    for row in &target_rows {
        output.push('\n');
        output.push_str(&format_targets_table_row(
            &[&row[0], &row[1], &row[2], &row[3]],
            &target_widths,
        ));
    }
    output.push('\n');
    output.push_str(&format_targets_table_separator(&target_widths));
    output
}

fn format_kv_table_separator(widths: &[usize; 2]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

fn format_kv_table_row(values: &[&str; 2], widths: &[usize; 2]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
}

fn format_targets_table_separator(widths: &[usize; 4]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

fn format_targets_table_row(values: &[&str; 4], widths: &[usize; 4]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
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

fn build_route_update_endpoint(id: &str, upsert: bool) -> String {
    if upsert {
        format!("/v1/routes/{id}?upsert=true")
    } else {
        format!("/v1/routes/{id}")
    }
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

fn infer_expose_route_upsert_action(route_exists: bool) -> &'static str {
    if route_exists { "update" } else { "create" }
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

fn extract_error_message(body: &str) -> String {
    serde_json::from_str::<ErrorResponse>(body)
        .map(|err| err.error)
        .unwrap_or_else(|_| body.to_string())
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
    fn infer_expose_route_upsert_action_classifies_create_or_update() {
        assert_eq!(infer_expose_route_upsert_action(false), "create");
        assert_eq!(infer_expose_route_upsert_action(true), "update");
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
