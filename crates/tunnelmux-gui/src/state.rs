use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct GuiAppState {
    pub settings_dir_override: Option<PathBuf>,
}
