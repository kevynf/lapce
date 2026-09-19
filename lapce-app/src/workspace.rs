use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{main_split::SplitInfo, panel::data::PanelInfo};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LapceWorkspace {
    pub path: Option<PathBuf>,
    pub last_open: u64,
}

impl LapceWorkspace {
    pub fn display(&self) -> Option<String> {
        let path = self.path.as_ref()?;
        let path = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy()
            .to_string();
        Some(path)
    }
}

impl Default for LapceWorkspace {
    fn default() -> Self {
        Self {
            path: None,
            last_open: 0,
        }
    }
}

impl std::fmt::Display for LapceWorkspace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.path.as_ref().and_then(|p| p.to_str()).unwrap_or("")
        )
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub split: SplitInfo,
    pub panel: PanelInfo,
}
