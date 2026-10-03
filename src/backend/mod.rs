use crate::model::Snapshot;
use anyhow::Result;
use async_trait::async_trait;

pub mod linux;

#[async_trait]
pub trait NetworkBackend: Send + Sync {
    async fn snapshot(&self) -> Result<Snapshot>;
    fn platform(&self) -> &'static str;
}

pub fn system_backend() -> Result<Box<dyn NetworkBackend>> {
    if cfg!(target_os = "linux") {
        Ok(Box::new(linux::LinuxBackend))
    } else {
        anyhow::bail!("This release supports Linux. NetworkBackend is the extension point for other platforms.")
    }
}
