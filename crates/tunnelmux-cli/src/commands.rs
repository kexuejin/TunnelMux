use super::*;

const PRIMARY_TUNNEL_ID: &str = "primary";

pub(super) async fn run(cli: Cli) -> anyhow::Result<()> {
    let control_client = build_control_client(&cli);
    let base_url = control_client.base_url().to_string();
    let token = control_client.token().map(str::to_string);
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
                let health: HealthResponse = control_client.health().await?;
                let tunnel: TunnelStatusResponse =
                    control_client.tunnel_status(PRIMARY_TUNNEL_ID).await?;
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
                let dashboard: DashboardResponse = control_client.dashboard().await?;
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
                let metrics: MetricsResponse = control_client.metrics().await?;
                println!("{}", serde_json::to_string_pretty(&metrics)?);
            }
        }
        Command::Diagnostics => {
            let diagnostics: DiagnosticsResponse = control_client.diagnostics().await?;
            println!("{}", serde_json::to_string_pretty(&diagnostics)?);
        }
        Command::Tunnel { command } => match command {
            TunnelCommand::Start {
                provider,
                target_url,
                auto_restart,
            } => {
                let payload = TunnelStartRequest {
                    tunnel_id: PRIMARY_TUNNEL_ID.to_string(),
                    provider: provider.into(),
                    target_url,
                    auto_restart: Some(auto_restart),
                    metadata: None,
                };
                let status: TunnelStatusResponse = control_client.start_tunnel(&payload).await?;
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
                let status: TunnelStatusResponse =
                    control_client.stop_tunnel(PRIMARY_TUNNEL_ID).await?;
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
            let routes: RoutesResponse = control_client.list_routes().await?;
            let existing_route = routes.routes.iter().find(|item| item.id == id).cloned();
            let route_action = infer_expose_route_action(existing_route.as_ref(), &route_payload);
            let mut tunnel: TunnelStatusResponse =
                control_client.tunnel_status(PRIMARY_TUNNEL_ID).await?;
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
                        "route_action": expose_route_action_name(route_action),
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
                let route: tunnelmux_core::RouteRule = match route_action {
                    ExposeRouteAction::Unchanged => existing_route
                        .ok_or_else(|| anyhow!("existing route disappeared during expose apply"))?,
                    ExposeRouteAction::Create | ExposeRouteAction::Update => {
                        control_client
                            .update_route_with_options(&id, &route_payload, true)
                            .await?
                    }
                };

                let mut tunnel_started = false;
                let mut tunnel_restarted = false;
                match tunnel_action {
                    ExposeTunnelAction::Noop => {}
                    ExposeTunnelAction::Start => {
                        let start_request = TunnelStartRequest {
                            tunnel_id: PRIMARY_TUNNEL_ID.to_string(),
                            provider: provider.clone(),
                            target_url: target_url.clone(),
                            auto_restart: Some(auto_restart),
                            metadata: None,
                        };
                        tunnel = control_client.start_tunnel(&start_request).await?;
                        tunnel_started = true;
                    }
                    ExposeTunnelAction::Restart => {
                        let _stopped: TunnelStatusResponse =
                            control_client.stop_tunnel(PRIMARY_TUNNEL_ID).await?;
                        let start_request = TunnelStartRequest {
                            tunnel_id: PRIMARY_TUNNEL_ID.to_string(),
                            provider: provider.clone(),
                            target_url: target_url.clone(),
                            auto_restart: Some(auto_restart),
                            metadata: None,
                        };
                        tunnel = control_client.start_tunnel(&start_request).await?;
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
                        "route_action": expose_route_action_name(route_action),
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
            let routes: RoutesResponse = control_client.list_routes().await?;
            let route_exists = routes.routes.iter().any(|item| item.id == id);
            if !route_exists && !ignore_missing {
                return Err(anyhow!("route '{}' not found", id));
            }
            let mut tunnel: TunnelStatusResponse =
                control_client.tunnel_status(PRIMARY_TUNNEL_ID).await?;
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
                let remove = control_client.delete_route(&id, ignore_missing).await?;
                if tunnel_stopped {
                    tunnel = control_client.stop_tunnel(PRIMARY_TUNNEL_ID).await?;
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
                    let routes: RoutesResponse = control_client.list_routes().await?;
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
                    control_client.create_route(&payload).await?;
                println!("{}", serde_json::to_string_pretty(&route)?);
            }
            RoutesCommand::Remove { id } => {
                let response: DeleteRouteResponse = control_client.delete_route(&id, false).await?;
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
            RoutesCommand::Match { path, host, table } => {
                let path = normalize_match_route_path(path)?;
                let payload: RouteMatchResponse =
                    control_client.match_route(&path, host.as_deref()).await?;
                if table {
                    println!("{}", format_route_match_table(&payload));
                } else {
                    println!("{}", serde_json::to_string_pretty(&payload)?);
                }
            }
            RoutesCommand::Export { id, out } => {
                let routes: RoutesResponse = control_client.list_routes().await?;
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
                let response: ApplyRoutesResponse = control_client.apply_routes(&payload).await?;
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
                let route: tunnelmux_core::RouteRule = control_client
                    .update_route_with_options(&id, &payload, upsert)
                    .await?;
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
                        control_client.upstreams_health().await?;
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
                    let response: HealthCheckSettingsResponse =
                        control_client.get_health_check_settings().await?;
                    println!("{}", serde_json::to_string_pretty(&response)?);
                } else {
                    let payload = UpdateHealthCheckSettingsRequest {
                        interval_ms,
                        timeout_ms,
                        path,
                    };
                    let response: HealthCheckSettingsResponse = control_client
                        .update_health_check_settings(&payload)
                        .await?;
                    println!("{}", serde_json::to_string_pretty(&response)?);
                }
            }
            SettingsCommand::Reload => {
                let response: ReloadSettingsResponse = control_client.reload_settings().await?;
                println!("{}", serde_json::to_string_pretty(&response)?);
            }
        },
    }

    Ok(())
}
