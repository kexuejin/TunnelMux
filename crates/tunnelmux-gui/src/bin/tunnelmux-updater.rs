use std::env;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const TARGET_BINARIES: &[&str] = &["tunnelmux-gui.exe", "tunnelmuxd.exe", "tunnelmux-cli.exe"];

fn main() {
    if let Err(error) = run() {
        eprintln!("tunnelmux updater helper failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 6
        || args[0] != "--pid"
        || args[2] != "--payload-dir"
        || args[4] != "--install-dir"
    {
        return Err(
            "usage: tunnelmux-updater --pid <pid> --payload-dir <dir> --install-dir <dir>"
                .to_string(),
        );
    }
    let pid = args[1]
        .parse::<u32>()
        .map_err(|error| format!("invalid parent pid: {error}"))?;
    let payload_dir = PathBuf::from(&args[3]);
    let install_dir = PathBuf::from(&args[5]);
    wait_for_process_exit(pid)?;

    for name in TARGET_BINARIES {
        replace_file(&payload_dir.join(name), &install_dir.join(name))?;
    }
    Ok(())
}

fn wait_for_process_exit(pid: u32) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(120);
    while process_exists(pid) {
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for process {pid} to exit"));
        }
        thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

#[cfg(windows)]
fn process_exists(pid: u32) -> bool {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output();
    output
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|text| text.contains(&pid.to_string()))
}

#[cfg(not(windows))]
fn process_exists(_pid: u32) -> bool {
    false
}

fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    let temporary = destination.with_extension("new");
    let backup = destination.with_extension("old");
    let _ = fs::remove_file(&temporary);
    let _ = fs::remove_file(&backup);
    fs::copy(source, &temporary).map_err(|error| error.to_string())?;
    set_executable_permissions(&temporary)?;

    if destination.exists() {
        fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&temporary, destination) {
        let _ = fs::remove_file(&temporary);
        if backup.exists() {
            let _ = fs::rename(&backup, destination);
        }
        return Err(error.to_string());
    }
    let _ = fs::remove_file(&backup);
    Ok(())
}

#[cfg(unix)]
fn set_executable_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}
