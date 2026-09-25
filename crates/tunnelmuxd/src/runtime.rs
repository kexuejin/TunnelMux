use super::*;
use std::path::Path;
use tokio::task::JoinSet;

pub(super) async fn persist_from_runtime(state: &Arc<AppState>) -> Result<(), ApiError> {
    let _persist_guard = state.persist_lock.lock().await;
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
    let (running, pending_cleared, start_cancelled) = {
        let mut runtime = state.runtime.lock().await;
        let start_cancelled = runtime.in_flight_starts.contains_key(tunnel_id);
        runtime.cancel_start(tunnel_id);
        (
            runtime.running_tunnels.remove(tunnel_id),
            runtime.pending_restarts.remove(tunnel_id).is_some(),
            start_cancelled,
        )
    };

    if let Some(mut running) = running {
        terminate_child(&mut running.child).await?;
        return Ok(true);
    }

    Ok(pending_cleared || start_cancelled)
}

/// How the startup reclaim pass is configured.
pub(super) struct LeftoverProviderOptions<'a> {
    pub cloudflared_bin: &'a str,
    pub ngrok_bin: &'a str,
    /// When false the pass still reconciles statuses but signals nothing.
    pub terminate: bool,
}

/// What the process table can say about one pid.
///
/// Three states, not two: "not running" and "could not be read" must not be
/// conflated. A read failure treated as "gone" would clear the record of a
/// process that is still holding the tunnel's connector — silently, and exactly
/// when the operator is least able to notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProcessState {
    /// The table answered: this pid is not running.
    Gone,
    /// The table answered: it is running, with this command line.
    Running(String),
    /// The table could not be read, so nothing follows from it.
    Unknown,
}

/// The two process-table operations the reclaim pass needs.
///
/// Injected rather than called inline so the decisions — which pid is
/// signalled, and what the rewritten status says — are testable without a real
/// process table. The system implementation reads `ps`; a test that cannot
/// reach `ps` (a sandboxed one, say) can still exercise everything above it.
///
/// `Send + Sync` because the daemon's own startup future is held by callers
/// that require it to be `Send` — the desktop app runs the daemon inside a
/// Tauri command.
pub(super) trait ProcessTable: Send + Sync {
    fn process_state(&self, pid: u32) -> ProcessState;
    /// Deliver a signal. `true` when the delivery itself succeeded.
    fn signal(&self, pid: u32, signal: &str) -> bool;
}

/// The real process table, via `ps` and `kill`.
pub(super) struct SystemProcessTable;

impl ProcessTable for SystemProcessTable {
    fn process_state(&self, pid: u32) -> ProcessState {
        // `std::process` explicitly: `tokio::process::Command` is what is in
        // scope here, and this is a short, startup-only, blocking read whose
        // output is only ever compared against strings this daemon composed.
        let output = match std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "args="])
            .output()
        {
            Ok(output) => output,
            // `ps` itself could not be run. That is not evidence about the pid.
            Err(_) => return ProcessState::Unknown,
        };
        let command_line = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if output.status.success() {
            return if command_line.is_empty() {
                ProcessState::Unknown
            } else {
                ProcessState::Running(command_line)
            };
        }
        // The documented "no such process" answer: a failure with nothing on
        // stdout. Anything else (a policy denial, a usage error) leaves the
        // question open.
        if command_line.is_empty() {
            ProcessState::Gone
        } else {
            ProcessState::Unknown
        }
    }

    fn signal(&self, pid: u32, signal: &str) -> bool {
        std::process::Command::new("kill")
            .arg(format!("-{signal}"))
            .arg(pid.to_string())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

/// What a startup reclaim pass found, so the caller can log it.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct LeftoverProviderReport {
    /// `(tunnel_id, pid)` terminated for real.
    pub terminated: Vec<(String, u32)>,
    /// `(tunnel_id, pid)` whose process had already exited.
    pub already_gone: Vec<(String, u32)>,
    /// `(tunnel_id, pid)` left running — either the operator asked for that, or
    /// the pid is not the provider this daemon would have started.
    pub left_alone: Vec<(String, u32)>,
    /// `(tunnel_id, pid)` whose liveness could not be established at all.
    pub undetermined: Vec<(String, u32)>,
    /// Tunnel ids whose persisted status was reset.
    pub cleared: Vec<String>,
}

impl LeftoverProviderReport {
    pub(super) fn is_empty(&self) -> bool {
        self.terminated.is_empty()
            && self.already_gone.is_empty()
            && self.left_alone.is_empty()
            && self.undetermined.is_empty()
            && self.cleared.is_empty()
    }
}

/// Take ownership of — or clean up after — a provider left by a previous run.
///
/// `stop_running_process` can only kill a child *this* process spawned, and the
/// runtime reconciler only inspects tunnels present in `running_tunnels`. A
/// provider that outlived a daemon restart is therefore invisible to both: it
/// keeps holding the Cloudflare/ngrok connector while the daemon, which reads
/// its status from disk, reports a tunnel nobody is managing. Depending on what
/// was written last, that reads as "running" (with a pid that is not the
/// daemon's) or as "stopped" while the public hostname still answers. Neither
/// is true, and both hide an exposed endpoint from whoever asked for it to be
/// off.
///
/// So at startup, before serving: for every persisted status that still claims
/// a running tunnel, prove the recorded pid really is that tunnel's provider
/// and terminate it. The status is reset either way — after this point the
/// daemon owns no child for that tunnel, and saying otherwise is the bug.
pub(super) async fn reclaim_leftover_providers(
    persisted: &mut PersistedState,
    options: &LeftoverProviderOptions<'_>,
    process_table: &dyn ProcessTable,
) -> LeftoverProviderReport {
    let mut report = LeftoverProviderReport::default();

    // Snapshot first: the loop mutates the statuses it is reading.
    let leftovers: Vec<(String, TunnelStatus)> = persisted
        .tunnels
        .iter()
        .filter(|tunnel| status_claims_a_running_provider(&tunnel.status))
        .map(|tunnel| (tunnel.id.clone(), tunnel.status.clone()))
        .collect();

    for (tunnel_id, status) in leftovers {
        let provider = status.provider.clone();
        let target_url = status.target_url.clone();
        let mut outcome = LeftoverOutcome::NoProcessRecorded;

        if let Some(pid) = status.process_id {
            match process_table.process_state(pid) {
                ProcessState::Unknown => {
                    warn!(
                        "leftover tunnel {tunnel_id}: the process table could not be read for pid \
                         {pid}; not signalling it"
                    );
                    report.undetermined.push((tunnel_id.clone(), pid));
                    outcome = LeftoverOutcome::Undetermined { pid };
                }
                ProcessState::Gone => {
                    report.already_gone.push((tunnel_id.clone(), pid));
                    outcome = LeftoverOutcome::AlreadyGone { pid };
                }
                ProcessState::Running(command_line) => {
                    let is_ours = leftover_command_line_matches(
                        &command_line,
                        provider.as_ref(),
                        target_url.as_deref(),
                        options,
                    );
                    if !is_ours {
                        // Pids are reused. Anything that is not provably the
                        // provider this tunnel recorded is never signalled.
                        warn!(
                            "leftover tunnel {tunnel_id}: pid {pid} is alive but is not this tunnel's \
                             provider (command line: {command_line}); leaving it alone"
                        );
                        report.left_alone.push((tunnel_id.clone(), pid));
                        outcome = LeftoverOutcome::NotThisProvider { pid };
                    } else if options.terminate {
                        let killed = terminate_process(pid, process_table).await;
                        warn!(
                            "leftover tunnel {tunnel_id}: terminated provider process {pid} \
                             (clean exit: {killed})"
                        );
                        report.terminated.push((tunnel_id.clone(), pid));
                        outcome = LeftoverOutcome::Terminated { pid };
                    } else {
                        warn!(
                            "leftover tunnel {tunnel_id}: provider process {pid} is still running and \
                             has been left alone on request"
                        );
                        report.left_alone.push((tunnel_id.clone(), pid));
                        outcome = LeftoverOutcome::KeptOnRequest { pid };
                    }
                }
            }
        }

        let tunnel = persisted.ensure_tunnel_status_mut(&tunnel_id);
        let last_error = outcome.message(&tunnel_id, provider.as_ref());
        *tunnel = default_tunnel_status(TunnelState::Stopped);
        tunnel.provider = provider;
        tunnel.target_url = target_url;
        tunnel.last_error = Some(last_error);
        report.cleared.push(tunnel_id);
    }

    if !report.is_empty() {
        info!(
            "startup reclaim: cleared {} stale status(es), terminated {}, already gone {}, left alone {}, \
             undetermined {}",
            report.cleared.len(),
            report.terminated.len(),
            report.already_gone.len(),
            report.left_alone.len(),
            report.undetermined.len()
        );
    }
    report
}

/// What happened to one leftover, so the reset status can say why.
enum LeftoverOutcome {
    /// The status claimed a running tunnel but recorded no pid.
    NoProcessRecorded,
    AlreadyGone {
        pid: u32,
    },
    NotThisProvider {
        pid: u32,
    },
    Terminated {
        pid: u32,
    },
    KeptOnRequest {
        pid: u32,
    },
    Undetermined {
        pid: u32,
    },
}

impl LeftoverOutcome {
    fn message(&self, tunnel_id: &str, provider: Option<&TunnelProvider>) -> String {
        let provider = provider_name(provider);
        match self {
            LeftoverOutcome::NoProcessRecorded => format!(
                "startup reclaim: tunnel {tunnel_id} was recorded as running but no provider process \
                 was recorded; nothing was running to reclaim"
            ),
            LeftoverOutcome::AlreadyGone { pid } => format!(
                "startup reclaim: the {provider} process {pid} recorded for tunnel {tunnel_id} had \
                 already exited"
            ),
            LeftoverOutcome::NotThisProvider { pid } => format!(
                "startup reclaim: pid {pid} recorded for tunnel {tunnel_id} is alive but is not this \
                 tunnel's {provider} provider, so it was not signalled; if a {provider} process is \
                 still serving this tunnel it must be stopped by hand"
            ),
            LeftoverOutcome::Terminated { pid } => format!(
                "startup reclaim: terminated orphaned {provider} process {pid} left by a previous \
                 daemon run"
            ),
            LeftoverOutcome::KeptOnRequest { pid } => format!(
                "startup reclaim: orphaned {provider} process {pid} from a previous daemon run is \
                 still running (--keep-leftover-providers); this daemon does not manage it, so the \
                 public hostname may still answer while this status reads stopped"
            ),
            LeftoverOutcome::Undetermined { pid } => format!(
                "startup reclaim: could not read the process table for pid {pid} recorded for tunnel \
                 {tunnel_id}, so it was not signalled — this is not evidence that it exited. If a \
                 {provider} process is still serving this tunnel it must be stopped by hand"
            ),
        }
    }
}

fn provider_name(provider: Option<&TunnelProvider>) -> &'static str {
    match provider {
        Some(TunnelProvider::Ngrok) => "ngrok",
        _ => "cloudflared",
    }
}

/// Whether a persisted status still claims a tunnel that something else started.
///
/// Any recorded pid counts, whatever the state: at startup this daemon has
/// spawned nothing, so a pid on disk can only belong to an earlier run.
fn status_claims_a_running_provider(status: &TunnelStatus) -> bool {
    status.process_id.is_some()
        || matches!(status.state, TunnelState::Running | TunnelState::Starting)
}

/// Whether a live process is provably the provider this tunnel recorded.
///
/// Strict on purpose — a pid alone proves nothing, because pids are reused:
/// the command line has to name the provider binary *and* the tunnel's own
/// target URL, which is what `build_provider_command` puts there. A recorded
/// target is required, so a status without one is never grounds for a signal.
fn leftover_command_line_matches(
    command_line: &str,
    provider: Option<&TunnelProvider>,
    target_url: Option<&str>,
    options: &LeftoverProviderOptions<'_>,
) -> bool {
    let binary = match provider {
        Some(TunnelProvider::Ngrok) => options.ngrok_bin,
        _ => options.cloudflared_bin,
    };
    let binary_name = Path::new(binary)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(binary);
    if !command_line.contains(binary_name) {
        return false;
    }
    target_url
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .is_some_and(|url| command_line.contains(url))
}

/// SIGTERM, escalating to SIGKILL, with a bounded wait so a wedged orphan
/// cannot delay startup. Returns whether the process is gone afterwards.
async fn terminate_process(pid: u32, process_table: &dyn ProcessTable) -> bool {
    if !process_table.signal(pid, "TERM") {
        return false;
    }
    // cloudflared's default grace period is 30s, but an orphan has no client
    // pointed at it, so it exits promptly. The bound keeps a wedged process
    // from delaying startup.
    for _ in 0..30 {
        sleep(Duration::from_millis(100)).await;
        if process_table.process_state(pid) == ProcessState::Gone {
            return true;
        }
    }
    process_table.signal(pid, "KILL");
    for _ in 0..10 {
        sleep(Duration::from_millis(100)).await;
        if process_table.process_state(pid) == ProcessState::Gone {
            return true;
        }
    }
    false
}

pub(super) async fn monitor_runtime_state(state: Arc<AppState>) {
    loop {
        if state.is_shutting_down() {
            return;
        }
        if let Err(err) = reconcile_runtime_and_maybe_restart(&state).await {
            warn!("runtime reconcile failed: {}", err.message);
        }
        sleep(Duration::from_secs(1)).await;
    }
}

pub(super) async fn monitor_upstream_health(state: Arc<AppState>) {
    loop {
        if state.is_shutting_down() {
            return;
        }
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
        if state.is_shutting_down() {
            return;
        }
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
        if !route_health_check_enabled(route) {
            continue;
        }
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
    let mut tasks = JoinSet::new();
    for upstream_key in upstreams {
        let state = state.clone();
        let timeout_ms = settings.timeout_ms;
        tasks.spawn(async move {
            let checked_at = now_iso();
            let check_url = match build_health_check_url(
                &upstream_key.upstream_url,
                &upstream_key.health_check_path,
            ) {
                Ok(url) => url,
                Err(err) => {
                    return (
                        upstream_key,
                        UpstreamHealth {
                            healthy: false,
                            last_checked_at: checked_at,
                            last_error: Some(err.to_string()),
                        },
                    );
                }
            };
            let check_result = state
                .proxy_client
                .get(check_url)
                .timeout(Duration::from_millis(timeout_ms))
                .send()
                .await;
            let health = match check_result {
                Ok(response) if response.status().is_success() => UpstreamHealth {
                    healthy: true,
                    last_checked_at: checked_at,
                    last_error: None,
                },
                Ok(response) => UpstreamHealth {
                    healthy: false,
                    last_checked_at: checked_at,
                    last_error: Some(format!("status {}", response.status())),
                },
                Err(err) => UpstreamHealth {
                    healthy: false,
                    last_checked_at: checked_at,
                    last_error: Some(err.to_string()),
                },
            };
            (upstream_key, health)
        });
    }
    while let Some(result) = tasks.join_next().await {
        if let Ok((key, health)) = result {
            latest.insert(key, health);
        }
    }

    let mut health_map = state.upstream_health.lock().await;
    *health_map = latest;
    Ok(())
}

pub(super) async fn reconcile_runtime_and_maybe_restart(
    state: &Arc<AppState>,
) -> Result<(), ApiError> {
    // Never resurrect a tunnel while the daemon is being torn down.
    if state.is_shutting_down() {
        return Ok(());
    }

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
        let (pending, generation) = {
            let mut runtime = state.runtime.lock().await;
            let Some(pending) = runtime.pending_restarts.remove(&tunnel_id) else {
                continue;
            };
            let generation = runtime.begin_start(&tunnel_id);
            let tunnel = runtime.persisted.ensure_tunnel_status_mut(&tunnel_id);
            tunnel.state = TunnelState::Starting;
            tunnel.updated_at = now_iso();
            (pending, generation)
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
                let SpawnedTunnel {
                    child,
                    public_url,
                    process_id,
                } = spawned;
                let mut child = Some(child);
                let committed = {
                    let mut runtime = state.runtime.lock().await;
                    if state.is_shutting_down() || !runtime.start_is_current(&tunnel_id, generation)
                    {
                        false
                    } else {
                        runtime.in_flight_starts.remove(&tunnel_id);
                        let status = TunnelStatus {
                            state: TunnelState::Running,
                            provider: Some(pending.provider.clone()),
                            target_url: Some(pending.target_url.clone()),
                            public_base_url: public_url.clone(),
                            started_at: Some(pending.started_at.clone()),
                            updated_at: now_iso(),
                            process_id,
                            auto_restart: pending.auto_restart,
                            restart_count: pending.restart_count,
                            last_error: None,
                        };
                        runtime.running_tunnels.insert(
                            tunnel_id.clone(),
                            RunningTunnel {
                                child: child.take().expect("child is committed once"),
                                provider: pending.provider.clone(),
                                target_url: pending.target_url.clone(),
                                metadata: pending.metadata.clone(),
                                auto_restart: pending.auto_restart,
                                restart_count: pending.restart_count,
                                started_at: pending.started_at.clone(),
                                public_base_url: public_url.clone(),
                                process_id,
                            },
                        );
                        *runtime.persisted.ensure_tunnel_status_mut(&tunnel_id) = status;
                        true
                    }
                };
                if !committed {
                    terminate_child(
                        child
                            .as_mut()
                            .expect("cancelled child is still owned by restart"),
                    )
                    .await
                    .ok();
                    continue;
                }
                changed = true;
            }
            Err(err) => {
                let current = {
                    let mut runtime = state.runtime.lock().await;
                    let current = runtime.start_is_current(&tunnel_id, generation);
                    if current {
                        runtime.in_flight_starts.remove(&tunnel_id);
                    }
                    current
                };
                if !current {
                    continue;
                }
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
    ensure_tunnel_gateway_listener(state, &request.tunnel_id, &request.target_url).await?;
    let provider_binary =
        provider_binary_for_request(&state.cloudflared_bin, &state.ngrok_bin, request);

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
        state.provider_log.clone(),
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
    let provider_binary = provider_binary_for_request(cloudflared_bin, ngrok_bin, request);

    let mut command = match request.provider {
        TunnelProvider::Cloudflared => {
            let mut cmd = Command::new(provider_binary);
            if let Some(protocol) = cloudflared_protocol_for_request(request) {
                cmd.args(["tunnel", "--protocol", protocol]);
            } else {
                cmd.arg("tunnel");
            }
            if let Some(token) = request
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.get("cloudflaredTunnelToken"))
                .map(|item| item.trim())
                .filter(|item| !item.is_empty())
            {
                cmd.args([
                    "--no-autoupdate",
                    "run",
                    "--token",
                    token,
                    "--url",
                    request.target_url.as_str(),
                ]);
            } else {
                cmd.args(["--no-autoupdate", "--url", request.target_url.as_str()]);
            }
            cmd
        }
        TunnelProvider::Ngrok => {
            let mut cmd = Command::new(provider_binary);
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

fn cloudflared_protocol_for_request(request: &TunnelStartRequest) -> Option<&'static str> {
    request
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("cloudflaredProtocol"))
        .map(|item| item.trim().to_ascii_lowercase())
        .and_then(protocol_arg)
}

/// Map a requested protocol onto a `--protocol` argument.
///
/// The accepted set is cloudflared's own: `auto` (its default and the value it
/// recommends), `quic`, `http2`. Anything else yields `None`, which drops the
/// flag entirely and lets cloudflared apply its default — an unknown value must
/// never become a flag that turns a start into a hard failure.
///
/// `auto` is *not* translated away into `http2`: cloudflared probes both
/// transports and picks a reachable one, which is what keeps a start working on
/// a network where the pinned transport is blocked.
fn protocol_arg(protocol: String) -> Option<&'static str> {
    match protocol.as_str() {
        "auto" => Some("auto"),
        "http2" => Some("http2"),
        "quic" => Some("quic"),
        _ => None,
    }
}

fn provider_binary_for_request<'a>(
    cloudflared_bin: &'a str,
    ngrok_bin: &'a str,
    request: &'a TunnelStartRequest,
) -> &'a str {
    match request.provider {
        TunnelProvider::Cloudflared => cloudflared_bin,
        TunnelProvider::Ngrok => ngrok_bin,
    }
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
    provider_log: Arc<ProviderLogSink>,
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
        request.tunnel_id.clone(),
        provider.clone(),
        "stdout",
        provider_log.clone(),
    ));
    tokio::spawn(pipe_reader_to_channel(
        stderr,
        tx,
        request.tunnel_id.clone(),
        provider.clone(),
        "stderr",
        provider_log,
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
                    Err(anyhow!(
                        "provider log stream closed before URL was discovered"
                    ))
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
    tunnel_id: String,
    provider: TunnelProvider,
    stream_name: &'static str,
    provider_log: Arc<ProviderLogSink>,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut lines = BufReader::new(reader).lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let formatted = format_provider_log_line(&tunnel_id, &provider, stream_name, &line);
                provider_log.write_line(&formatted).await;
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

/// The single append-only provider log for this process, with size-based
/// rotation.
///
/// Every tunnel's stdout/stderr funnel through one shared instance so rotation
/// stays coherent. If each reader kept its own handle, the first one to rotate
/// would rename the file out from under the others, and their subsequent writes
/// would land in a file nobody ever reads again.
#[derive(Debug)]
pub(super) struct ProviderLogSink {
    path: PathBuf,
    max_bytes: u64,
    max_files: usize,
    state: Mutex<OpenLog>,
}

#[derive(Debug)]
struct OpenLog {
    file: Option<fs::File>,
    /// Bytes currently in the live log. Tracked here instead of re-`stat`ing per
    /// line, because right after this process wrote, a `stat` can still report
    /// the pre-write size — which silently skips rotations.
    written: u64,
}

impl ProviderLogSink {
    pub(super) fn new(path: PathBuf, max_bytes: u64, max_files: usize) -> Self {
        Self {
            path,
            max_bytes,
            max_files,
            state: Mutex::new(OpenLog {
                file: None,
                written: 0,
            }),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub(super) fn max_files(&self) -> usize {
        self.max_files
    }

    /// Append one already-formatted line, rotating the log first when it would
    /// grow past the configured size.
    ///
    /// Logging never fails the caller: if the file cannot be opened or written
    /// the line is dropped with a warning, because a tunnel must not die over
    /// its own diagnostics.
    pub(super) async fn write_line(&self, line: &str) {
        let mut state = self.state.lock().await;

        if state.file.is_none() {
            match open_provider_log_file(&self.path).await {
                Ok(handle) => {
                    // Start from whatever is already on disk, so a log left over
                    // from an earlier run still counts toward the cap.
                    state.written = handle
                        .metadata()
                        .await
                        .map(|metadata| metadata.len())
                        .unwrap_or(0);
                    state.file = Some(handle);
                }
                Err(err) => {
                    warn!(
                        "failed to open provider log file {}: {err}",
                        self.path.display()
                    );
                    return;
                }
            }
        }

        if self.should_rotate(state.written, line.len() as u64) {
            match rotate_provider_log(&self.path, self.max_files).await {
                Ok(()) => {
                    // Drop the handle to the renamed file before reopening the
                    // live path, otherwise writes would keep flowing into the
                    // backup that rotation just sealed.
                    state.file = None;
                    state.written = 0;
                    match open_provider_log_file(&self.path).await {
                        Ok(handle) => state.file = Some(handle),
                        Err(err) => {
                            warn!(
                                "failed to reopen provider log file {}: {err}",
                                self.path.display()
                            );
                            return;
                        }
                    }
                }
                Err(err) => warn!(
                    "failed to rotate provider log {}: {err}",
                    self.path.display()
                ),
            }
        }

        // Destructure up front: `state` is a guard, so touching two fields
        // through it borrows the whole guard twice.
        let OpenLog { file, written } = &mut *state;
        if let Some(handle) = file.as_mut() {
            match handle.write_all(line.as_bytes()).await {
                Ok(()) => {
                    *written += line.len() as u64;
                    // `write_all` only hands the bytes to tokio's background
                    // writer; without a flush the tail of the log can still be
                    // in flight when it is rotated or read back, which shows up
                    // as truncated output.
                    if let Err(err) = handle.flush().await {
                        warn!("failed to flush provider logs: {err}");
                        *file = None;
                        *written = 0;
                    }
                }
                Err(err) => {
                    warn!("failed to write provider logs: {err}");
                    *file = None;
                    *written = 0;
                }
            }
        }
    }

    /// Whether appending `incoming` more bytes would push the log past its cap.
    ///
    /// An empty live log is never rotated: that only happens when the cap is
    /// smaller than a single line, where rotating would just mint empty backups.
    fn should_rotate(&self, written: u64, incoming: u64) -> bool {
        self.max_bytes > 0 && written > 0 && written.saturating_add(incoming) > self.max_bytes
    }
}

/// Move `provider.log` to `provider.log.1`, shifting the older backups up and
/// discarding the oldest one. The live file is gone afterwards, so the caller
/// must reopen it.
async fn rotate_provider_log(path: &Path, max_files: usize) -> anyhow::Result<()> {
    if max_files == 0 {
        // No backups requested: keep the disk footprint at the cap by starting
        // the live file over.
        return fs::write(path, [])
            .await
            .with_context(|| format!("failed to truncate provider log {}", path.display()));
    }

    let _ = fs::remove_file(rotated_provider_log_path(path, max_files)).await;
    for index in (1..max_files).rev() {
        let from = rotated_provider_log_path(path, index);
        if fs::try_exists(&from).await.unwrap_or(false) {
            fs::rename(&from, rotated_provider_log_path(path, index + 1))
                .await
                .with_context(|| format!("failed to shift provider log {}", from.display()))?;
        }
    }

    fs::rename(path, rotated_provider_log_path(path, 1))
        .await
        .with_context(|| format!("failed to rotate provider log {}", path.display()))
}

fn rotated_provider_log_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{index}"));
    path.with_file_name(name)
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
    tunnel_id: &str,
    provider: &TunnelProvider,
    stream_name: &str,
    line: &str,
) -> String {
    let provider_name = match provider {
        TunnelProvider::Cloudflared => "cloudflared",
        TunnelProvider::Ngrok => "ngrok",
    };
    format!(
        "{} [{}:{}:{}] {}\n",
        now_iso(),
        tunnel_id,
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
    use std::sync::atomic::{AtomicU64, Ordering};

    // ── startup reclaim of a provider left behind by a previous run ─────────

    /// A process table that answers from memory, so the reclaim decisions can be
    /// tested without a real process table — and without `ps`, which not every
    /// environment a test runs in can reach.
    struct FakeProcessTable {
        command_lines: std::sync::Mutex<HashMap<u32, String>>,
        signals: std::sync::Mutex<Vec<String>>,
        /// A process that ignores SIGTERM is what forces the SIGKILL escalation.
        honours_sigterm: bool,
        /// Make every read fail, the way a table this process cannot reach does.
        unreadable: bool,
    }

    impl FakeProcessTable {
        fn with(entries: &[(u32, &str)]) -> Self {
            Self {
                command_lines: std::sync::Mutex::new(
                    entries
                        .iter()
                        .map(|(pid, line)| (*pid, (*line).to_string()))
                        .collect(),
                ),
                signals: std::sync::Mutex::new(Vec::new()),
                honours_sigterm: true,
                unreadable: false,
            }
        }

        fn unreadable() -> Self {
            Self {
                unreadable: true,
                ..Self::with(&[])
            }
        }

        fn signals(&self) -> Vec<String> {
            self.signals.lock().expect("signals lock").clone()
        }

        fn is_alive(&self, pid: u32) -> bool {
            self.command_lines
                .lock()
                .expect("command lines lock")
                .contains_key(&pid)
        }
    }

    impl ProcessTable for FakeProcessTable {
        fn process_state(&self, pid: u32) -> ProcessState {
            if self.unreadable {
                return ProcessState::Unknown;
            }
            match self
                .command_lines
                .lock()
                .expect("command lines lock")
                .get(&pid)
                .cloned()
            {
                Some(command_line) => ProcessState::Running(command_line),
                None => ProcessState::Gone,
            }
        }

        fn signal(&self, pid: u32, signal: &str) -> bool {
            self.signals
                .lock()
                .expect("signals lock")
                .push(format!("{signal}:{pid}"));
            if signal == "KILL" || self.honours_sigterm {
                self.command_lines
                    .lock()
                    .expect("command lines lock")
                    .remove(&pid);
            }
            true
        }
    }

    const CLOUDFLARED_BIN: &str = "/opt/homebrew/bin/cloudflared";
    const NGROK_BIN: &str = "/opt/homebrew/bin/ngrok";
    const GATEWAY_URL: &str = "http://127.0.0.1:48081";
    const LIVE_PROVIDER: &str = "/opt/homebrew/bin/cloudflared tunnel --protocol auto --no-autoupdate --url http://127.0.0.1:48081";

    fn reclaim_options(terminate: bool) -> LeftoverProviderOptions<'static> {
        LeftoverProviderOptions {
            cloudflared_bin: CLOUDFLARED_BIN,
            ngrok_bin: NGROK_BIN,
            terminate,
        }
    }

    /// A persisted state that says the primary tunnel is running with `pid`.
    fn persisted_running(pid: Option<u32>, target_url: Option<&str>) -> PersistedState {
        let mut persisted = PersistedState::default();
        let status = persisted.ensure_tunnel_status_mut("primary");
        status.state = TunnelState::Running;
        status.provider = Some(TunnelProvider::Cloudflared);
        status.target_url = target_url.map(str::to_string);
        status.process_id = pid;
        persisted
    }

    #[tokio::test]
    async fn reclaim_terminates_the_provider_a_previous_run_left_behind() {
        let table = FakeProcessTable::with(&[(4242, LIVE_PROVIDER)]);
        let mut persisted = persisted_running(Some(4242), Some(GATEWAY_URL));

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(true), &table).await;

        assert_eq!(report.terminated, vec![("primary".to_string(), 4242)]);
        assert_eq!(table.signals(), vec!["TERM:4242"]);
        assert!(
            !table.is_alive(4242),
            "the leftover has to actually be gone"
        );
        let status = persisted.tunnel_status("primary").expect("status");
        assert_eq!(status.state, TunnelState::Stopped);
        assert_eq!(status.process_id, None);
        // Only the process claim is dropped; the tunnel stays configured.
        assert_eq!(status.target_url.as_deref(), Some(GATEWAY_URL));
        assert!(
            status
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("terminated orphaned cloudflared process 4242"),
            "the reset status must say what happened: {:?}",
            status.last_error
        );
    }

    #[tokio::test]
    async fn reclaim_never_signals_a_pid_that_is_not_this_tunnels_provider() {
        // Pid reuse: the recorded pid is very much alive, but it is somebody else.
        let table =
            FakeProcessTable::with(&[(4242, "/Applications/Other.app/Contents/MacOS/Other")]);
        let mut persisted = persisted_running(Some(4242), Some(GATEWAY_URL));

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(true), &table).await;

        assert!(report.terminated.is_empty());
        assert_eq!(report.left_alone, vec![("primary".to_string(), 4242)]);
        assert!(
            table.signals().is_empty(),
            "a pid that is not provably ours must never be signalled"
        );
        assert!(table.is_alive(4242));
        let status = persisted.tunnel_status("primary").expect("status");
        assert_eq!(status.state, TunnelState::Stopped);
        assert!(
            status
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("is not this tunnel's cloudflared provider")
        );
    }

    #[tokio::test]
    async fn reclaim_clears_a_status_whose_process_is_already_gone() {
        let table = FakeProcessTable::with(&[]);
        let mut persisted = persisted_running(Some(4242), Some(GATEWAY_URL));

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(true), &table).await;

        assert_eq!(report.already_gone, vec![("primary".to_string(), 4242)]);
        assert!(table.signals().is_empty());
        assert_eq!(
            persisted.tunnel_status("primary").expect("status").state,
            TunnelState::Stopped
        );
    }

    #[tokio::test]
    async fn reclaim_clears_a_running_status_that_recorded_no_process() {
        let table = FakeProcessTable::with(&[]);
        let mut persisted = persisted_running(None, Some(GATEWAY_URL));

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(true), &table).await;

        assert_eq!(report.cleared, vec!["primary".to_string()]);
        assert!(report.terminated.is_empty() && report.already_gone.is_empty());
        let status = persisted.tunnel_status("primary").expect("status");
        assert_eq!(status.state, TunnelState::Stopped);
        assert!(
            status
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("no provider process was recorded")
        );
    }

    #[tokio::test]
    async fn reclaim_escalates_to_sigkill_when_the_orphan_ignores_sigterm() {
        let mut table = FakeProcessTable::with(&[(4242, LIVE_PROVIDER)]);
        table.honours_sigterm = false;
        let mut persisted = persisted_running(Some(4242), Some(GATEWAY_URL));

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(true), &table).await;

        assert_eq!(report.terminated, vec![("primary".to_string(), 4242)]);
        assert_eq!(table.signals(), vec!["TERM:4242", "KILL:4242"]);
        assert!(!table.is_alive(4242));
    }

    #[tokio::test]
    async fn reclaim_admits_it_could_not_check_rather_than_claiming_the_process_exited() {
        // A process table this daemon cannot read must not be reported as "the
        // process had already exited": that clears the record of something that
        // may still be holding the tunnel's connector.
        let table = FakeProcessTable::unreadable();
        let mut persisted = persisted_running(Some(4242), Some(GATEWAY_URL));

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(true), &table).await;

        assert_eq!(report.undetermined, vec![("primary".to_string(), 4242)]);
        assert!(
            report.already_gone.is_empty(),
            "an unreadable table is not evidence of exit"
        );
        assert!(
            table.signals().is_empty(),
            "an unreadable table means no signal"
        );
        let status = persisted.tunnel_status("primary").expect("status");
        assert!(
            status
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("could not read the process table"),
            "the status must admit the gap: {:?}",
            status.last_error
        );
    }

    #[tokio::test]
    async fn reclaim_leaves_the_process_alone_when_asked_to() {
        let table = FakeProcessTable::with(&[(4242, LIVE_PROVIDER)]);
        let mut persisted = persisted_running(Some(4242), Some(GATEWAY_URL));

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(false), &table).await;

        assert!(report.terminated.is_empty());
        assert!(
            table.signals().is_empty(),
            "--keep-leftover-providers means no signal at all"
        );
        assert!(table.is_alive(4242));
        let status = persisted.tunnel_status("primary").expect("status");
        assert_eq!(status.state, TunnelState::Stopped);
        assert!(
            status
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("still running"),
            "a kept orphan must still be admitted to: {:?}",
            status.last_error
        );
    }

    #[tokio::test]
    async fn reclaim_touches_a_tunnel_that_was_not_running() {
        let table = FakeProcessTable::with(&[]);
        let mut persisted = PersistedState::default();
        let status = persisted.ensure_tunnel_status_mut("primary");
        *status = default_tunnel_status(TunnelState::Stopped);
        status.provider = Some(TunnelProvider::Cloudflared);
        status.target_url = Some(GATEWAY_URL.to_string());

        let report =
            reclaim_leftover_providers(&mut persisted, &reclaim_options(true), &table).await;

        assert!(report.is_empty(), "a stopped tunnel is not a leftover");
    }

    #[test]
    fn a_recorded_pid_alone_is_a_leftover_whatever_the_state_says() {
        // At startup this daemon has spawned nothing, so a pid on disk can only
        // belong to an earlier run — even if the last write said "error".
        let mut status = default_tunnel_status(TunnelState::Error);
        status.process_id = Some(4242);
        assert!(status_claims_a_running_provider(&status));

        let mut running = default_tunnel_status(TunnelState::Running);
        running.process_id = None;
        assert!(status_claims_a_running_provider(&running));

        let stopped = default_tunnel_status(TunnelState::Stopped);
        assert!(!status_claims_a_running_provider(&stopped));
    }

    #[test]
    fn leftover_matching_requires_the_binary_and_the_tunnels_own_target() {
        let options = reclaim_options(true);
        let cloudflared = TunnelProvider::Cloudflared;

        assert!(leftover_command_line_matches(
            LIVE_PROVIDER,
            Some(&cloudflared),
            Some(GATEWAY_URL),
            &options,
        ));
        // Same binary, but a different tunnel's target.
        assert!(!leftover_command_line_matches(
            "/opt/homebrew/bin/cloudflared tunnel --url http://127.0.0.1:48080",
            Some(&cloudflared),
            Some(GATEWAY_URL),
            &options,
        ));
        // The right target, but not the provider that was recorded.
        assert!(!leftover_command_line_matches(
            "/opt/homebrew/bin/ngrok http http://127.0.0.1:48081",
            Some(&cloudflared),
            Some(GATEWAY_URL),
            &options,
        ));
        // No recorded target: never grounds for a signal.
        assert!(!leftover_command_line_matches(
            "/opt/homebrew/bin/cloudflared tunnel",
            Some(&cloudflared),
            None,
            &options,
        ));
    }

    /// The real table, exercised for real — but only where `ps` is reachable. A
    /// sandboxed test runner may deny it (setuid tool, restricted process
    /// table); skipping is honest, whereas failing would blame the code for the
    /// environment.
    #[tokio::test]
    async fn system_process_table_reads_and_signals_a_real_process() {
        if !matches!(
            SystemProcessTable.process_state(std::process::id()),
            ProcessState::Running(_)
        ) {
            eprintln!("skipping: the process table (`ps`) is not reachable from this test runner");
            return;
        }

        // A stand-in for a provider: the command line carries the target URL, so
        // it has the shape `build_provider_command` produces.
        let mut child = Command::new("sleep")
            .arg("300")
            .arg(GATEWAY_URL)
            .spawn()
            .expect("sleep should spawn");
        let pid = child.id().expect("a spawned child has a pid");

        let command_line = match SystemProcessTable.process_state(pid) {
            ProcessState::Running(command_line) => command_line,
            other => panic!("a live process should have a readable command line, got {other:?}"),
        };
        assert!(command_line.contains("sleep") && command_line.contains(GATEWAY_URL));

        // Reap concurrently: this child is *ours*, so without a waiter it would
        // sit as a zombie and never read as gone.
        let reaper = tokio::spawn(async move { child.wait().await });
        assert!(
            terminate_process(pid, &SystemProcessTable).await,
            "SIGTERM should end a plain sleep"
        );
        let _ = reaper.await;
        assert_eq!(SystemProcessTable.process_state(pid), ProcessState::Gone);
    }

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

    #[test]
    fn cloudflared_command_accepts_valid_protocol_metadata() {
        let request = TunnelStartRequest {
            tunnel_id: "primary".to_string(),
            provider: TunnelProvider::Cloudflared,
            target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: Some(true),
            metadata: Some(HashMap::from([
                ("cloudflaredTunnelToken".to_string(), "cf-token".to_string()),
                ("cloudflaredProtocol".to_string(), "http2".to_string()),
            ])),
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
                "--protocol",
                "http2",
                "--no-autoupdate",
                "run",
                "--token",
                "cf-token",
                "--url",
                "http://127.0.0.1:48080",
            ]
        );
    }

    #[test]
    fn cloudflared_command_ignores_invalid_protocol_metadata() {
        let request = TunnelStartRequest {
            tunnel_id: "primary".to_string(),
            provider: TunnelProvider::Cloudflared,
            target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: Some(true),
            metadata: Some(HashMap::from([(
                "cloudflaredProtocol".to_string(),
                "ftp".to_string(),
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
                "--url",
                "http://127.0.0.1:48080",
            ]
        );
    }

    #[test]
    fn protocol_arg_maps_cloudflared_protocol_names() {
        // The metadata path lowercases before calling, so the table is lowercase.
        assert_eq!(protocol_arg("auto".to_string()), Some("auto"));
        assert_eq!(protocol_arg("quic".to_string()), Some("quic"));
        assert_eq!(protocol_arg("http2".to_string()), Some("http2"));
        assert_eq!(protocol_arg("ftp".to_string()), None);
        assert_eq!(protocol_arg(String::new()), None);
    }

    #[test]
    fn cloudflared_command_accepts_auto_protocol_metadata() {
        let request = TunnelStartRequest {
            tunnel_id: "primary".to_string(),
            provider: TunnelProvider::Cloudflared,
            target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: Some(true),
            metadata: Some(HashMap::from([(
                "cloudflaredProtocol".to_string(),
                "auto".to_string(),
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
                "--protocol",
                "auto",
                "--no-autoupdate",
                "--url",
                "http://127.0.0.1:48080",
            ]
        );
    }

    #[test]
    fn provider_command_ignores_metadata_path_override() {
        let request = TunnelStartRequest {
            tunnel_id: "primary".to_string(),
            provider: TunnelProvider::Cloudflared,
            target_url: "http://127.0.0.1:48080".to_string(),
            auto_restart: Some(true),
            metadata: Some(HashMap::from([(
                "providerBinaryPath".to_string(),
                "/tmp/tools/bin/cloudflared".to_string(),
            )])),
        };

        let command = build_provider_command(
            "/opt/homebrew/bin/cloudflared",
            "/opt/homebrew/bin/ngrok",
            &request,
        )
        .expect("command should build");

        assert_eq!(
            command.as_std().get_program().to_string_lossy(),
            "/opt/homebrew/bin/cloudflared"
        );
    }

    /// A private directory for one log-rotation test, cleaned before use.
    ///
    /// The name carries the pid because a counter alone repeats across runs, and
    /// the sink appends rather than truncates: a run that failed mid-test would
    /// otherwise leave content behind that breaks the next one.
    async fn fresh_log_dir(name: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "tunnelmuxd-log-{}-{unique}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir).await;
        dir
    }

    #[tokio::test]
    async fn provider_log_sink_rotates_at_the_size_cap_and_drops_the_oldest() {
        let dir = fresh_log_dir("rotate").await;
        let path = dir.join("provider.log");
        // The first two 9-byte lines still fit under the 20-byte cap, so the
        // rotation lands on the third write and repeats from there.
        let sink = ProviderLogSink::new(path.clone(), 20, 2);

        for line in ["line-one\n", "line-two\n", "line-three\n", "line-four\n"] {
            sink.write_line(line).await;
        }

        assert_eq!(fs::read_to_string(&path).await.unwrap(), "line-four\n");
        assert_eq!(
            fs::read_to_string(path.with_file_name("provider.log.1"))
                .await
                .unwrap(),
            "line-three\n"
        );
        assert_eq!(
            fs::read_to_string(path.with_file_name("provider.log.2"))
                .await
                .unwrap(),
            "line-one\nline-two\n"
        );
        // Only `max_files` backups are kept, and the oldest content is the
        // first thing to go.
        assert!(
            !fs::try_exists(path.with_file_name("provider.log.3"))
                .await
                .unwrap()
        );

        let _ = fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn provider_log_sink_appends_without_rotating_when_the_cap_is_disabled() {
        let dir = fresh_log_dir("no-rotate").await;
        let path = dir.join("provider.log");
        let sink = ProviderLogSink::new(path.clone(), 0, 2);

        for line in ["line-one\n", "line-two\n", "line-three\n"] {
            sink.write_line(line).await;
        }

        assert_eq!(
            fs::read_to_string(&path).await.unwrap(),
            "line-one\nline-two\nline-three\n"
        );
        assert!(
            !fs::try_exists(path.with_file_name("provider.log.1"))
                .await
                .unwrap()
        );

        let _ = fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn provider_log_sink_truncates_in_place_when_no_backups_are_kept() {
        let dir = fresh_log_dir("truncate").await;
        let path = dir.join("provider.log");
        let sink = ProviderLogSink::new(path.clone(), 20, 0);

        for line in ["line-one\n", "line-two\n", "line-three\n"] {
            sink.write_line(line).await;
        }

        // The footprint stays at the cap: nothing is kept but the newest line.
        assert_eq!(fs::read_to_string(&path).await.unwrap(), "line-three\n");
        assert!(
            !fs::try_exists(path.with_file_name("provider.log.1"))
                .await
                .unwrap()
        );

        let _ = fs::remove_dir_all(&dir).await;
    }

    #[tokio::test]
    async fn provider_log_sink_creates_missing_parent_directories() {
        let dir = fresh_log_dir("nested").await;
        let path = dir.join("deep").join("provider.log");
        let sink = ProviderLogSink::new(path.clone(), 0, 1);

        sink.write_line("hello\n").await;

        assert_eq!(fs::read_to_string(&path).await.unwrap(), "hello\n");

        let _ = fs::remove_dir_all(&dir).await;
    }
}
