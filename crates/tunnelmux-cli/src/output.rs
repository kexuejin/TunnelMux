use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SseFrame {
    pub(super) event: String,
    pub(super) data: String,
}

#[derive(Debug, Default)]
pub(super) struct SseFrameBuilder {
    event: Option<String>,
    data_lines: Vec<String>,
}

impl SseFrameBuilder {
    pub(super) fn push_line(&mut self, line: &str) -> Option<SseFrame> {
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

pub(super) fn trim_sse_field_value(value: &str) -> &str {
    value.strip_prefix(' ').unwrap_or(value)
}

pub(super) fn take_next_sse_line(buffer: &mut String) -> Option<String> {
    let index = buffer.find('\n')?;
    let mut line = buffer[..index].to_string();
    buffer.drain(..=index);
    if line.ends_with('\r') {
        line.pop();
    }
    Some(line)
}

pub(super) fn format_status_output(
    health: &HealthResponse,
    tunnel: &TunnelStatusResponse,
) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(&json!({
        "health": health,
        "tunnel": tunnel.tunnel,
    }))?)
}

pub(super) fn render_status_stream_frame(
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

pub(super) fn render_logs_stream_frame(frame: &SseFrame) -> anyhow::Result<()> {
    match frame.event.as_str() {
        "line" | "message" => println!("{}", frame.data),
        "error" => {
            eprintln!("logs stream error: {}", frame.data);
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn render_metrics_stream_frame(
    frame: &SseFrame,
    interval_ms: u64,
) -> anyhow::Result<()> {
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

pub(super) fn render_dashboard_stream_frame(
    frame: &SseFrame,
    interval_ms: u64,
) -> anyhow::Result<()> {
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

pub(super) fn render_upstreams_stream_frame(
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

pub(super) fn render_routes_stream_frame(
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

pub(super) fn format_routes(
    response: &RoutesResponse,
    format: RoutesOutputFormat,
) -> anyhow::Result<String> {
    match format {
        RoutesOutputFormat::Json => Ok(serde_json::to_string_pretty(response)?),
        RoutesOutputFormat::Table => Ok(render_routes_table(response)),
    }
}

pub(super) fn render_routes_table(response: &RoutesResponse) -> String {
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

pub(super) fn format_routes_table_separator(widths: &[usize; 7]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

pub(super) fn format_routes_table_row(values: &[&str; 7], widths: &[usize; 7]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
}

pub(super) fn format_route_match_table(response: &RouteMatchResponse) -> String {
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

pub(super) fn format_kv_table_separator(widths: &[usize; 2]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

pub(super) fn format_kv_table_row(values: &[&str; 2], widths: &[usize; 2]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
}

pub(super) fn format_targets_table_separator(widths: &[usize; 4]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

pub(super) fn format_targets_table_row(values: &[&str; 4], widths: &[usize; 4]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
}

pub(super) fn format_upstreams_health(
    response: &UpstreamsHealthResponse,
    format: UpstreamsOutputFormat,
) -> anyhow::Result<String> {
    match format {
        UpstreamsOutputFormat::Json => Ok(serde_json::to_string_pretty(response)?),
        UpstreamsOutputFormat::Table => Ok(render_upstreams_health_table(response)),
    }
}

pub(super) fn render_upstreams_health_table(response: &UpstreamsHealthResponse) -> String {
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

pub(super) fn format_table_separator(widths: &[usize; 5]) -> String {
    let mut line = String::from("+");
    for width in widths {
        line.push_str(&"-".repeat(width + 2));
        line.push('+');
    }
    line
}

pub(super) fn format_table_row(values: &[&str; 5], widths: &[usize; 5]) -> String {
    let mut line = String::from("|");
    for (index, value) in values.iter().enumerate() {
        line.push(' ');
        line.push_str(value);
        line.push_str(&" ".repeat(widths[index].saturating_sub(value.len()) + 1));
        line.push('|');
    }
    line
}

pub(super) fn upstream_health_label(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "healthy",
        Some(false) => "unhealthy",
        None => "unknown",
    }
}

pub(super) fn truncate_cell(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let keep = max_len.saturating_sub(3);
    format!("{}...", &value[..keep])
}

pub(super) fn write_output_or_stdout(output: &str, out: Option<&Path>) -> anyhow::Result<()> {
    if let Some(path) = out {
        fs::write(path, output)
            .with_context(|| format!("failed to write export output: {}", path.display()))?;
        return Ok(());
    }

    println!("{output}");
    Ok(())
}
