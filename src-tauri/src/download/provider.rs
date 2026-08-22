use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownloadProviderKind {
    Aria2,
    BitTorrent,
    Ftp,
    Thunder,
    CloudDrive,
    Browser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub source: String,

    pub destination: String,

    pub provider: Option<String>,

    pub credential_id: Option<String>,

    #[serde(default)]
    pub options: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadTask {
    pub task_id: String,

    pub provider: String,

    pub state: String,

    pub progress: Option<u64>,

    #[serde(default)]
    pub metadata: Value,

    pub output: Value,
}

pub trait DownloadProvider: Send + Sync {
    fn id(&self) -> &'static str;

    fn start(&self, request: DownloadRequest) -> Result<DownloadTask, String>;

    fn pause(&self, task_id: String) -> Result<DownloadTask, String>;

    fn resume(&self, task_id: String) -> Result<DownloadTask, String>;

    fn cancel(&self, task_id: String) -> Result<DownloadTask, String>;

    fn status(&self, task_id: String) -> Result<DownloadTask, String>;

    fn list(&self) -> Result<Vec<DownloadTask>, String>;
}
