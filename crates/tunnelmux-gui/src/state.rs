use crate::daemon_manager::DaemonRuntimeState;
use crate::provider_installer::ProviderInstallStatus;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::AtomicBool,
        Arc, Mutex,
    },
};

#[derive(Debug, Clone)]
pub struct GuiAppState {
    pub settings_dir_override: Option<PathBuf>,
    pub daemon_runtime: Arc<Mutex<DaemonRuntimeState>>,
    pub provider_install_statuses: Arc<Mutex<HashMap<String, ProviderInstallStatus>>>,
    pub exit_requested: Arc<AtomicBool>,
}

impl Default for GuiAppState {
    fn default() -> Self {
        Self {
            settings_dir_override: None,
            daemon_runtime: Arc::new(Mutex::new(DaemonRuntimeState::default())),
            provider_install_statuses: Arc::new(Mutex::new(HashMap::new())),
            exit_requested: Arc::new(AtomicBool::new(false)),
        }
    }
}
