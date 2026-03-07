use super::*;

pub(super) async fn persist_from_runtime(state: &Arc<AppState>) -> Result<(), ApiError> {
    let snapshot = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.clone()
    };

    save_state_file(&state.data_file, &snapshot)
        .await
        .map_err(|err| ApiError::internal(format!("failed to persist state: {err}")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ExitAction {
    NoRestart,
    Restart {
        next_restart_count: u32,
        backoff: Duration,
    },
    Exhausted,
}

pub(super) fn determine_exit_action(
    auto_restart: bool,
    restart_count: u32,
    max_auto_restarts: u32,
) -> ExitAction {
    if !auto_restart {
        return ExitAction::NoRestart;
    }

    if max_auto_restarts == 0 || restart_count >= max_auto_restarts {
        return ExitAction::Exhausted;
    }

    let next_restart_count = restart_count.saturating_add(1);
    ExitAction::Restart {
        next_restart_count,
        backoff: restart_backoff(next_restart_count),
    }
}

pub(super) fn restart_backoff(attempt: u32) -> Duration {
    let exponent = attempt.saturating_sub(1).min(5);
    Duration::from_secs(1_u64 << exponent)
}

pub(super) async fn stop_running_process(
    state: &Arc<AppState>,
    tunnel_id: &str,
) -> anyhow::Result<bool> {
    let (running, pending_cleared) = {
        let mut runtime = state.runtime.lock().await;
        (
            runtime.running_tunnels.remove(tunnel_id),
            runtime.pending_restarts.remove(tunnel_id).is_some(),
        )
    };

    if let Some(mut running) = running {
        terminate_child(&mut running.child).await?;
        return Ok(true);
    }

    Ok(pending_cleared)
}

pub(super) async fn monitor_runtime_state(state: Arc<AppState>) {
    loop {
        if let Err(err) = reconcile_runtime_and_maybe_restart(&state).await {
            warn!("runtime reconcile failed: {}", err.message);
        }
        sleep(Duration::from_secs(1)).await;
    }
}

pub(super) async fn monitor_upstream_health(state: Arc<AppState>) {
    loop {
        let settings = {
            let current = state.health_check_settings.read().await;
            current.clone()
        };

        if let Err(err) = refresh_upstream_health(&state, &settings).await {
            warn!("upstream health check failed: {err}");
        }
        sleep(Duration::from_millis(settings.interval_ms)).await;
    }
}

pub(super) fn hash_declarative_config(config: &DeclarativeConfigFile) -> anyhow::Result<u64> {
    use std::hash::{Hash, Hasher};

    let raw = serde_json::to_vec(config)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    raw.hash(&mut hasher);
    Ok(hasher.finish())
}

async fn apply_declarative_config(
    state: &Arc<AppState>,
    config: DeclarativeConfigFile,
    digest: u64,
) -> anyhow::Result<()> {
    let next_health_check = {
        let current = state.health_check_settings.read().await;
        resolve_initial_health_check_settings(current.clone(), config.health_check.clone())
    };

    {
        let mut current = state.health_check_settings.write().await;
        *current = next_health_check.clone();
    }

    {
        let mut runtime = state.runtime.lock().await;
        runtime.persisted.routes = config.routes;
        runtime.persisted.health_check = Some(next_health_check);
    }

    {
        let mut status = state.config_reload_status.lock().await;
        status.last_digest = Some(digest);
        status.last_config_reload_at = Some(now_iso());
        status.last_config_reload_error = None;
    }

    persist_from_runtime(state)
        .await
        .map_err(|err| anyhow!(err.message))?;
    Ok(())
}

pub(super) async fn reload_config_file(state: &Arc<AppState>, force: bool) -> anyhow::Result<bool> {
    let config = match load_config_file(&state.config_file).await {
        Ok(Some(config)) => config,
        Ok(None) => return Ok(false),
        Err(err) => {
            let message = err.to_string();
            let mut status = state.config_reload_status.lock().await;
            status.last_config_reload_error = Some(message.clone());
            return Err(anyhow!(message));
        }
    };

    let digest = hash_declarative_config(&config)?;
    {
        let status = state.config_reload_status.lock().await;
        if !force && status.last_digest == Some(digest) {
            return Ok(false);
        }
    }

    apply_declarative_config(state, config, digest).await?;
    Ok(true)
}

pub(super) async fn monitor_config_file(state: Arc<AppState>) {
    loop {
        if let Err(err) = reload_config_file(&state, false).await {
            warn!("config reload failed: {err}");
        }
        let interval_ms = {
            let status = state.config_reload_status.lock().await;
            status.interval_ms
        };
        sleep(Duration::from_millis(interval_ms)).await;
    }
}

pub(super) async fn refresh_upstream_health(
    state: &Arc<AppState>,
    settings: &HealthCheckSettings,
) -> anyhow::Result<()> {
    let routes = {
        let runtime = state.runtime.lock().await;
        runtime.persisted.routes.clone()
    };

    let mut upstreams = HashSet::new();
    for route in &routes {
        let route_health_check_path = effective_route_health_check_path(route, &settings.path);
        upstreams.insert(upstream_health_key(
            &route.upstream_url,
            &route_health_check_path,
        ));
        if let Some(fallback) = route.fallback_upstream_url.as_ref() {
            upstreams.insert(upstream_health_key(fallback, &route_health_check_path));
        }
    }

    let mut latest = HashMap::new();
    for upstream_key in upstreams {
        let checked_at = now_iso();
        let check_url = match build_health_check_url(
            &upstream_key.upstream_url,
            &upstream_key.health_check_path,
        ) {
            Ok(url) => url,
            Err(err) => {
                latest.insert(
                    upstream_key,
                    UpstreamHealth {
                        healthy: false,
                        last_checked_at: checked_at,
                        last_error: Some(err.to_string()),
                    },
                );
                continue;
            }
        };
        let check_result = state
            .proxy_client
            .get(check_url)
            .timeout(Duration::from_millis(settings.timeout_ms))
            .send()
            .await;

        let health = match check_result {
            Ok(response) if response.status().is_success() => UpstreamHealth {
                healthy: true,
                last_checked_at: checked_at.clone(),
                last_error: None,
            },
            Ok(response) => UpstreamHealth {
                healthy: false,
                last_checked_at: checked_at.clone(),
                last_error: Some(format!("status {}", response.status())),
            },
            Err(err) => UpstreamHealth {
                healthy: false,
                last_checked_at: checked_at,
                last_error: Some(err.to_string()),
            },
        };
        latest.insert(upstream_key, health);
    }

    let mut health_map = state.upstream_health.lock().await;
    *health_map = latest;
    Ok(())
}

pub(super) async fn reconcile_runtime_and_maybe_restart(
    state: &Arc<AppState>,
) -> Result<(), ApiError> {
    let mut changed = {
        let mut runtime = state.runtime.lock().await;
        reconcile_runtime_tunnel_state(&mut runtime, state.max_auto_restarts)?
    };

    changed |= process_pending_restart(state).await?;

    if changed {
        persist_from_runtime(state).await?;
    }
    Ok(())
}

pub(super) fn reconcile_runtime_tunnel_state(
    runtime: &mut RuntimeState,
    max_auto_restarts: u32,
) -> Result<bool, ApiError> {
    let tunnel_ids = runtime.running_tunnels.keys().cloned().collect::<Vec<_>>();
    let mut changed = false;
    for tunnel_id in tunnel_ids {
        changed |= reconcile_single_runtime_tunnel(runtime, &tunnel_id, max_auto_restarts)?;
    }

    Ok(changed)
}

fn reconcile_single_runtime_tunnel(
    runtime: &mut RuntimeState,
    tunnel_id: &str,
    max_auto_restarts: u32,
) -> Result<bool, ApiError> {
    enum Outcome {
        Exited {
            provider: TunnelProvider,
            target_url: String,
            metadata: Option<HashMap<String, String>>,
            public_base_url: Option<String>,
            started_at: String,
            auto_restart: bool,
            restart_count: u32,
            exit_reason: String,
        },
        Alive {
            provider: TunnelProvider,
            target_url: String,
            public_base_url: Option<String>,
            started_at: String,
            process_id: Option<u32>,
            auto_restart: bool,
            restart_count: u32,
        },
        InspectError {
            provider: TunnelProvider,
            target_url: String,
            message: String,
        },
    }

    let Some(running) = runtime.running_tunnels.get_mut(tunnel_id) else {
        return Ok(false);
    };

    let outcome = match running.child.try_wait() {
        Ok(Some(status)) => {
            let exit_reason = format!("provider process exited unexpectedly with status: {status}");
            warn!(
                "provider process exited unexpectedly: tunnel_id={}, provider={:?}, status={status}",
                tunnel_id, running.provider
            );
            Outcome::Exited {
                provider: running.provider.clone(),
                target_url: running.target_url.clone(),
                metadata: running.metadata.clone(),
                public_base_url: running.public_base_url.clone(),
                started_at: running.started_at.clone(),
                auto_restart: running.auto_restart,
                restart_count: running.restart_count,
                exit_reason,
            }
        }
        Ok(None) => Outcome::Alive {
            provider: running.provider.clone(),
            target_url: running.target_url.clone(),
            public_base_url: running.public_base_url.clone(),
            started_at: running.started_at.clone(),
            process_id: running.process_id,
            auto_restart: running.auto_restart,
            restart_count: running.restart_count,
        },
        Err(err) => Outcome::InspectError {
            provider: running.provider.clone(),
            target_url: running.target_url.clone(),
            message: format!("failed to inspect provider process state: {err}"),
        },
    };

    match outcome {
        Outcome::Exited {
            provider,
            target_url,
            metadata,
            public_base_url,
            started_at,
            auto_restart,
            restart_count,
            exit_reason,
        } => {
            runtime.running_tunnels.remove(tunnel_id);
            match determine_exit_action(auto_restart, restart_count, max_auto_restarts) {
                ExitAction::NoRestart => {
                    runtime.pending_restarts.remove(tunnel_id);
                    let tunnel = runtime.persisted.ensure_tunnel_status_mut(tunnel_id);
                    *tunnel = default_tunnel_status(TunnelState::Stopped);
                    tunnel.provider = Some(provider);
                    tunnel.target_url = Some(target_url);
                    tunnel.public_base_url = public_base_url;
                    tunnel.started_at = Some(started_at);
                    tunnel.auto_restart = auto_restart;
                    tunnel.restart_count = restart_count;
                    tunnel.last_error = Some(exit_reason);
                }
                ExitAction::Restart {
                    next_restart_count,
                    backoff,
                } => {
                    runtime.pending_restarts.insert(
                        tunnel_id.to_string(),
                        PendingRestart {
                            provider: provider.clone(),
                            target_url: target_url.clone(),
                            metadata,
                            auto_restart,
                            restart_count: next_restart_count,
                            started_at: started_at.clone(),
                            next_attempt_at: Instant::now() + backoff,
                            reason: exit_reason.clone(),
                        },
                    );
                    let tunnel = runtime.persisted.ensure_tunnel_status_mut(tunnel_id);
                    *tunnel = TunnelStatus {
                        state: TunnelState::Starting,
                        provider: Some(provider),
                        target_url: Some(target_url),
                        public_base_url: None,
                        started_at: Some(started_at),
                        updated_at: now_iso(),
                        process_id: None,
                        auto_restart,
                        restart_count: next_restart_count,
                        last_error: Some(format!(
                            "{exit_reason}; scheduling auto restart attempt {} in {}s",
                            next_restart_count,
                            backoff.as_secs()
                        )),
                    };
                }
                ExitAction::Exhausted => {
                    runtime.pending_restarts.remove(tunnel_id);
                    let tunnel = runtime.persisted.ensure_tunnel_status_mut(tunnel_id);
                    *tunnel = default_tunnel_status(TunnelState::Error);
                    tunnel.provider = Some(provider);
                    tunnel.target_url = Some(target_url);
                    tunnel.public_base_url = public_base_url;
                    tunnel.started_at = Some(started_at);
                    tunnel.auto_restart = auto_restart;
                    tunnel.restart_count = restart_count;
                    tunnel.last_error = Some(format!(
                        "{exit_reason}; auto restart limit reached ({max_auto_restarts})"
                    ));
                }
            }
            Ok(true)
        }
        Outcome::Alive {
            provider,
            target_url,
            public_base_url,
            started_at,
            process_id,
            auto_restart,
            restart_count,
        } => {
            let current = runtime
                .persisted
                .tunnel_status(tunnel_id)
                .cloned()
                .unwrap_or_else(|| default_tunnel_status(TunnelState::Idle));
            let should_update = current.state != TunnelState::Running
                || current.provider != Some(provider.clone())
                || current.target_url != Some(target_url.clone())
                || current.public_base_url != public_base_url
                || current.started_at != Some(started_at.clone())
                || current.process_id != process_id
                || current.auto_restart != auto_restart
                || current.restart_count != restart_count
                || current.last_error.is_some();
            if should_update {
                *runtime.persisted.ensure_tunnel_status_mut(tunnel_id) = TunnelStatus {
                    state: TunnelState::Running,
                    provider: Some(provider),
                    target_url: Some(target_url),
                    public_base_url,
                    started_at: Some(started_at),
                    updated_at: now_iso(),
                    process_id,
                    auto_restart,
                    restart_count,
                    last_error: None,
                };
                return Ok(true);
            }
            Ok(false)
        }
        Outcome::InspectError {
            provider,
            target_url,
            message,
        } => {
            runtime.running_tunnels.remove(tunnel_id);
            runtime.pending_restarts.remove(tunnel_id);
            let tunnel = runtime.persisted.ensure_tunnel_status_mut(tunnel_id);
            *tunnel = default_tunnel_status(TunnelState::Error);
            tunnel.provider = Some(provider);
            tunnel.target_url = Some(target_url);
            tunnel.last_error = Some(message);
            Ok(true)
        }
    }
}

pub(super) async fn process_pending_restart(state: &Arc<AppState>) -> Result<bool, ApiError> {
    let due_tunnels = {
        let runtime = state.runtime.lock().await;
        runtime
            .pending_restarts
            .iter()
            .filter(|(_, pending)| Instant::now() >= pending.next_attempt_at)
            .map(|(tunnel_id, _)| tunnel_id.clone())
            .collect::<Vec<_>>()
    };

    if due_tunnels.is_empty() {
        return Ok(false);
    }

    let mut changed = false;
    for tunnel_id in due_tunnels {
        let pending = {
            let mut runtime = state.runtime.lock().await;
            let Some(pending) = runtime.pending_restarts.remove(&tunnel_id) else {
                continue;
            };
            let tunnel = runtime.persisted.ensure_tunnel_status_mut(&tunnel_id);
            tunnel.state = TunnelState::Starting;
            tunnel.updated_at = now_iso();
            pending
        };

        let request = TunnelStartRequest {
            tunnel_id: tunnel_id.clone(),
            provider: pending.provider.clone(),
            target_url: pending.target_url.clone(),
            auto_restart: Some(pending.auto_restart),
            metadata: pending.metadata.clone(),
        };
        let attempt_no = pending.restart_count;

        match spawn_provider_process(state, &request).await {
            Ok(spawned) => {
                let status = TunnelStatus {
                    state: TunnelState::Running,
                    provider: Some(pending.provider.clone()),
                    target_url: Some(pending.target_url.clone()),
                    public_base_url: spawned.public_url.clone(),
                    started_at: Some(pending.started_at.clone()),
                    updated_at: now_iso(),
                    process_id: spawned.process_id,
                    auto_restart: pending.auto_restart,
                    restart_count: pending.restart_count,
                    last_error: None,
                };
                let mut runtime = state.runtime.lock().await;
                runtime.running_tunnels.insert(
                    tunnel_id.clone(),
                    RunningTunnel {
                        child: spawned.child,
                        provider: pending.provider,
                        target_url: pending.target_url,
                        metadata: pending.metadata,
                        auto_restart: pending.auto_restart,
                        restart_count: pending.restart_count,
                        started_at: pending.started_at,
                        public_base_url: spawned.public_url,
                        process_id: spawned.process_id,
                    },
                );
                *runtime.persisted.ensure_tunnel_status_mut(&tunnel_id) = status;
                changed = true;
            }
            Err(err) => {
                let action = determine_exit_action(
                    pending.auto_restart,
                    pending.restart_count,
                    state.max_auto_restarts,
                );
                let mut runtime = state.runtime.lock().await;
                match action {
                    ExitAction::Restart {
                        next_restart_count,
                        backoff,
                    } => {
                        runtime.pending_restarts.insert(
                            tunnel_id.clone(),
                            PendingRestart {
                                provider: pending.provider.clone(),
                                target_url: pending.target_url.clone(),
                                metadata: pending.metadata.clone(),
                                auto_restart: pending.auto_restart,
                                restart_count: next_restart_count,
                                started_at: pending.started_at.clone(),
                                next_attempt_at: Instant::now() + backoff,
                                reason: pending.reason.clone(),
                            },
                        );
                        *runtime.persisted.ensure_tunnel_status_mut(&tunnel_id) = TunnelStatus {
                            state: TunnelState::Starting,
                            provider: Some(pending.provider),
                            target_url: Some(pending.target_url),
                            public_base_url: None,
                            started_at: Some(pending.started_at),
                            updated_at: now_iso(),
                            process_id: None,
                            auto_restart: pending.auto_restart,
                            restart_count: next_restart_count,
                            last_error: Some(format!(
                                "auto restart attempt {} failed: {err}; retrying in {}s",
                                attempt_no,
                                backoff.as_secs()
                            )),
                        };
                    }
                    ExitAction::NoRestart | ExitAction::Exhausted => {
                        let tunnel = runtime.persisted.ensure_tunnel_status_mut(&tunnel_id);
                        *tunnel = default_tunnel_status(TunnelState::Error);
                        tunnel.provider = Some(pending.provider);
                        tunnel.target_url = Some(pending.target_url);
                        tunnel.started_at = Some(pending.started_at);
                        tunnel.auto_restart = pending.auto_restart;
                        tunnel.restart_count = pending.restart_count;
                        tunnel.last_error = Some(format!(
                            "auto restart attempt {} failed and no more retries are available: {err}",
                            attempt_no
                        ));
                    }
                }
                changed = true;
            }
        }
    }

    Ok(changed)
}

pub(super) async fn spawn_provider_process(
    state: &Arc<AppState>,
    request: &TunnelStartRequest,
) -> anyhow::Result<SpawnedTunnel> {
    let provider_binary = match request.provider {
        TunnelProvider::Cloudflared => state.cloudflared_bin.as_str(),
        TunnelProvider::Ngrok => state.ngrok_bin.as_str(),
    };

    let mut command = build_provider_command(&state.cloudflared_bin, &state.ngrok_bin, request)?;

    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .map_err(|error| provider_spawn_error(&request.provider, provider_binary, error))?;
    let process_id = child.id();
    let public_url = wait_for_provider_startup(
        &mut child,
        request,
        Duration::from_millis(state.ready_timeout_ms),
        state.provider_log_file.clone(),
    )
    .await
    .inspect_err(|err| warn!("provider startup failed: {err}"))?;

    Ok(SpawnedTunnel {
        child,
        public_url,
        process_id,
    })
}

fn build_provider_command(
    cloudflared_bin: &str,
    ngrok_bin: &str,
    request: &TunnelStartRequest,
) -> anyhow::Result<Command> {
    let mut command = match request.provider {
        TunnelProvider::Cloudflared => {
            let mut cmd = Command::new(cloudflared_bin);
            if let Some(token) = request
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("cloudflaredTunnelToken"))
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
            {
                cmd.args([
                    "tunnel",
                    "--no-autoupdate",
                    "run",
                    "--token",
                    token,
                    "--url",
                    request.target_url.as_str(),
                ]);
            } else {
                cmd.args([
                    "tunnel",
                    "--no-autoupdate",
                    "--url",
                    request.target_url.as_str(),
                ]);
            }
            cmd
        }
        TunnelProvider::Ngrok => {
            let mut cmd = Command::new(ngrok_bin);
            cmd.args([
                "http",
                request.target_url.as_str(),
                "--log",
                "stdout",
                "--log-format",
                "json",
            ]);

            if let Some(metadata) = request.metadata.as_ref() {
                if let Some(domain) = metadata
                    .get("ngrokDomain")
                    .or_else(|| metadata.get("domain"))
                    .map(|item| item.trim())
                    .filter(|item| !item.is_empty())
                {
                    cmd.arg("--domain").arg(domain);
                }

                if let Some(authtoken) = metadata
                    .get("ngrokAuthtoken")
                    .or_else(|| metadata.get("authtoken"))
                    .map(|item| item.trim())
                    .filter(|item| !item.is_empty())
                {
                    cmd.env("NGROK_AUTHTOKEN", authtoken);
                }
            }

            cmd
        }
    };

    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);

    Ok(command)
}

pub(super) fn provider_spawn_error(
    provider: &TunnelProvider,
    binary: &str,
    error: std::io::Error,
) -> anyhow::Error {
    if error.kind() == std::io::ErrorKind::NotFound {
        return anyhow!(
            "provider executable not found: {} ({:?}); install it or configure the daemon binary path",
            binary,
            provider
        );
    }

    anyhow!(error).context(format!("failed to spawn provider command: {:?}", provider))
}

fn cloudflared_uses_named_tunnel_token(request: &TunnelStartRequest) -> bool {
    request.provider == TunnelProvider::Cloudflared
        && request
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("cloudflaredTunnelToken"))
            .map(|item| item.trim())
            .filter(|item| !item.is_empty())
            .is_some()
}

fn provider_requires_public_url(request: &TunnelStartRequest) -> bool {
    !cloudflared_uses_named_tunnel_token(request)
}

pub(super) async fn wait_for_provider_startup(
    child: &mut Child,
    request: &TunnelStartRequest,
    timeout_duration: Duration,
    provider_log_file: PathBuf,
) -> anyhow::Result<Option<String>> {
    let provider = request.provider.clone();
    let require_public_url = provider_requires_public_url(request);
    let ready_after = if require_public_url {
        timeout_duration
    } else {
        timeout_duration.min(Duration::from_secs(5))
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("failed to capture provider stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("failed to capture provider stderr"))?;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(pipe_reader_to_channel(
        stdout,
        tx.clone(),
        provider.clone(),
        "stdout",
        provider_log_file.clone(),
    ));
    tokio::spawn(pipe_reader_to_channel(
        stderr,
        tx,
        provider.clone(),
        "stderr",
        provider_log_file,
    ));

    let start = Instant::now();
    let mut discovered_url = None;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!(
                "{}: {status}",
                if require_public_url {
                    "provider exited before publishing public URL"
                } else {
                    "provider exited before startup completed"
                }
            ));
        }

        let elapsed = start.elapsed();
        if !require_public_url && elapsed >= ready_after {
            return Ok(discovered_url);
        }

        if elapsed >= timeout_duration {
            let _ = terminate_child(child).await;
            return Err(anyhow!(
                "{} within {} ms",
                if require_public_url {
                    "provider did not report public URL"
                } else {
                    "provider did not stay up long enough to confirm startup"
                },
                timeout_duration.as_millis()
            ));
        }

        let deadline = if require_public_url {
            timeout_duration
        } else {
            ready_after
        };
        let remaining = deadline.saturating_sub(elapsed);
        match timeout(remaining, rx.recv()).await {
            Ok(Some(line)) => {
                if let Some(url) = extract_public_url(&provider, &line) {
                    discovered_url = Some(url);
                    if require_public_url {
                        return Ok(discovered_url);
                    }
                }
            }
            Ok(None) => {
                return if require_public_url {
                    let _ = terminate_child(child).await;
                    Err(anyhow!("provider log stream closed before URL was discovered"))
                } else {
                    Ok(discovered_url)
                };
            }
            Err(_) => {
                return if require_public_url {
                    let _ = terminate_child(child).await;
                    Err(anyhow!(
                        "provider did not report public URL within {} ms",
                        timeout_duration.as_millis()
                    ))
                } else {
                    Ok(discovered_url)
                };
            }
        }
    }
}

pub(super) async fn pipe_reader_to_channel<R>(
    reader: R,
    tx: mpsc::UnboundedSender<String>,
    provider: TunnelProvider,
    stream_name: &'static str,
    provider_log_file: PathBuf,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();
    let mut log_file = match open_provider_log_file(&provider_log_file).await {
        Ok(file) => Some(file),
        Err(err) => {
            warn!(
                "failed to open provider log file {}: {err}",
                provider_log_file.display()
            );
            None
        }
    };

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                if let Some(file) = log_file.as_mut() {
                    let formatted = format_provider_log_line(&provider, stream_name, &line);
                    if let Err(err) = file.write_all(formatted.as_bytes()).await {
                        warn!("failed to write provider logs: {err}");
                        log_file = None;
                    }
                }
                if tx.send(line.clone()).is_err() {
                    debug!(line = line, "provider-log");
                }
            }
            Ok(None) => break,
            Err(err) => {
                debug!("failed to read provider output: {err}");
                break;
            }
        }
    }
}

pub(super) async fn open_provider_log_file(path: &Path) -> anyhow::Result<fs::File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create provider log dir: {}", parent.display()))?;
    }

    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
        .with_context(|| format!("failed to open provider log file: {}", path.display()))
}

pub(super) fn format_provider_log_line(
    provider: &TunnelProvider,
    stream_name: &str,
    line: &str,
) -> String {
    let provider_name = match provider {
        TunnelProvider::Cloudflared => "cloudflared",
        TunnelProvider::Ngrok => "ngrok",
    };
    format!(
        "{} [{}:{}] {}\n",
        now_iso(),
        provider_name,
        stream_name,
        line
    )
}

pub(super) fn extract_public_url(provider: &TunnelProvider, line: &str) -> Option<String> {
    match provider {
        TunnelProvider::Cloudflared => cloudflared_url_regex()
            .find(line)
            .map(|matched| matched.as_str().to_string()),
        TunnelProvider::Ngrok => extract_ngrok_url(line),
    }
}

pub(super) fn extract_ngrok_url(line: &str) -> Option<String> {
    if let Ok(parsed) = serde_json::from_str::<HashMap<String, serde_json::Value>>(line) {
        if let Some(url) = parsed
            .get("url")
            .and_then(|value| value.as_str())
            .filter(|value| value.starts_with("https://"))
        {
            return Some(url.to_string());
        }
    }

    ngrok_url_regex()
        .find(line)
        .map(|matched| matched.as_str().to_string())
}

pub(super) fn cloudflared_url_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"https://[a-z0-9-]+\.trycloudflare\.com\b")
            .expect("valid cloudflared URL regex")
    })
}

pub(super) fn ngrok_url_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"https://[a-z0-9.-]*ngrok(?:-free)?\.app\b").expect("valid ngrok URL regex")
    })
}

pub(super) async fn terminate_child(child: &mut Child) -> anyhow::Result<()> {
    if child.try_wait()?.is_some() {
        return Ok(());
    }

    let _ = child.start_kill();
    let _ = timeout(Duration::from_secs(5), child.wait()).await;
    Ok(())
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    #[test]
    fn provider_spawn_error_mentions_missing_executable_path() {
        let error = provider_spawn_error(
            &TunnelProvider::Cloudflared,
            "/missing/cloudflared",
            std::io::Error::from(std::io::ErrorKind::NotFound),
        );

        assert!(
            error
                .to_string()
                .contains("provider executable not found: /missing/cloudflared"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn cloudflared_named_tunnel_does_not_require_public_url() {
        let request = TunnelStartRequest {
            tunnel_id: "primary".to_string(),
            provider: TunnelProvider::Cloudflared,
            target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: Some(true),
            metadata: Some(HashMap::from([(
                "cloudflaredTunnelToken".to_string(),
                "cf-token".to_string(),
            )])),
        };

        assert!(!provider_requires_public_url(&request));
    }

    #[test]
    fn cloudflared_command_uses_named_tunnel_token_when_present() {
        let request = TunnelStartRequest {
            tunnel_id: "primary".to_string(),
            provider: TunnelProvider::Cloudflared,
            target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: Some(true),
            metadata: Some(HashMap::from([(
                "cloudflaredTunnelToken".to_string(),
                "cf-token".to_string(),
            )])),
        };

        let command = build_provider_command(
            "/opt/homebrew/bin/cloudflared",
            "/opt/homebrew/bin/ngrok",
            &request,
        )
        .expect("command should build");

        let args = command
            .as_std()
            .get_args()
            .map(|value| value.to_string_lossy().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "tunnel",
                "--no-autoupdate",
                "run",
                "--token",
                "cf-token",
                "--url",
                "http://127.0.0.1:48080",
            ]
        );
    }
}
