use crate::daemon_manager::DaemonRuntimeState;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone)]
pub struct GuiAppState {
    pub settings_dir_override: Option<PathBuf>,
    pub daemon_runtime: Arc<Mutex<DaemonRuntimeState>>,
}

impl Default for GuiAppState {
    fn default() -> Self {
        Self {
            settings_dir_override: None,
            daemon_runtime: Arc::new(Mutex::new(DaemonRuntimeState::default())),
        }
    }
}
