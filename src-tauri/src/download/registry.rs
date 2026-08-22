use super::{aria2::Aria2Client, provider::DownloadProvider};

pub struct Aria2DownloadProvider;

impl DownloadProvider for Aria2DownloadProvider {
    fn id(&self) -> &'static str {
        "aria2"
    }

    fn start(
        &self,
        request: super::provider::DownloadRequest,
    ) -> Result<super::provider::DownloadTask, String> {
        tauri::async_runtime::block_on(Aria2Client::add_uri(request))
    }

    fn pause(&self, task_id: String) -> Result<super::provider::DownloadTask, String> {
        tauri::async_runtime::block_on(Aria2Client::pause(task_id))
    }

    fn resume(&self, task_id: String) -> Result<super::provider::DownloadTask, String> {
        tauri::async_runtime::block_on(Aria2Client::resume(task_id))
    }

    fn cancel(&self, task_id: String) -> Result<super::provider::DownloadTask, String> {
        tauri::async_runtime::block_on(Aria2Client::cancel(task_id))
    }

    fn status(&self, task_id: String) -> Result<super::provider::DownloadTask, String> {
        tauri::async_runtime::block_on(Aria2Client::tell_status(task_id))
    }

    fn list(&self) -> Result<Vec<super::provider::DownloadTask>, String> {
        tauri::async_runtime::block_on(Aria2Client::list_active())
    }
}

pub fn list_download_providers() -> Vec<&'static str> {
    vec!["aria2", "bittorrent", "ftp", "cloud-drive", "browser"]
}

pub fn resolve_default_download_provider() -> Option<Box<dyn DownloadProvider>> {
    resolve_download_provider("aria2")
}

pub fn resolve_download_provider(id: &str) -> Option<Box<dyn DownloadProvider>> {
    match id {
        "aria2" => Some(Box::new(Aria2DownloadProvider)),

        _ => None,
    }
}
