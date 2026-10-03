use crate::model::{NetworkEvent, Sample, ToolResult};
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub chart_renderer: String,
    pub refresh_seconds: u64,
    pub external_enabled: bool,
    pub monitoring_enabled: bool,
    pub targets: Vec<String>,
    pub public_ip_url: String,
    pub internet_url: String,
    pub dns_test_name: String,
    pub pihole_url: Option<String>,
    pub dot_server: Option<String>,
    pub dot_tls_name: Option<String>,
    pub retention_samples: usize,
    pub custom_colors: BTreeMap<String, String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            chart_renderer: "auto".into(),
            refresh_seconds: 2,
            external_enabled: false,
            monitoring_enabled: false,
            targets: vec!["1.1.1.1".into(), "8.8.8.8".into()],
            public_ip_url: "https://api.ipify.org".into(),
            internet_url: "https://example.com".into(),
            dns_test_name: "example.com".into(),
            pihole_url: None,
            dot_server: None,
            dot_tls_name: None,
            retention_samples: 1800,
            custom_colors: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SavedProfile {
    pub name: String,
    pub interface: String,
    pub dns: Vec<String>,
    pub mtu: Option<u32>,
    pub routes: Vec<crate::model::Route>,
    pub proxy: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct History {
    pub samples: Vec<Sample>,
    pub tests: Vec<ToolResult>,
    pub tool_results: Vec<ToolResult>,
    pub events: Vec<NetworkEvent>,
    pub profiles: Vec<SavedProfile>,
    pub known_devices: BTreeMap<String, String>,
    pub devices: BTreeMap<String, DeviceObservation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceObservation {
    pub first_seen: chrono::DateTime<chrono::Utc>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
    pub mac: String,
    pub state: String,
    pub vendor: String,
}

pub struct Storage {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}
impl Storage {
    pub fn new() -> Result<Self> {
        let dirs = ProjectDirs::from("dev", "nexus", "nexus-net")
            .context("Cannot determine config directory")?;
        Ok(Self {
            config_dir: dirs.config_dir().into(),
            data_dir: dirs.data_local_dir().into(),
        })
    }
    pub fn load_config(&self) -> Result<Config> {
        let p = self.config_dir.join("config.toml");
        if !p.exists() {
            return Ok(Config::default());
        }
        let mut c: Config =
            toml::from_str(&std::fs::read_to_string(p)?).context("Invalid config.toml")?;
        c.refresh_seconds = c.refresh_seconds.clamp(1, 60);
        c.retention_samples = c.retention_samples.clamp(60, 43200);
        Ok(c)
    }
    pub fn load_history(&self) -> Result<History> {
        let p = self.data_dir.join("history.json");
        if !p.exists() {
            return Ok(History::default());
        }
        if p.metadata()?.len() > 32 * 1024 * 1024 {
            anyhow::bail!("History file exceeds 32 MiB");
        }
        serde_json::from_str(&std::fs::read_to_string(p)?).context("Invalid history.json")
    }
    pub fn save_config(&self, c: &Config) -> Result<()> {
        private_atomic(
            &self.config_dir.join("config.toml"),
            toml::to_string_pretty(c)?.as_bytes(),
        )
    }
    pub fn save_history(&self, h: &History) -> Result<()> {
        private_atomic(&self.data_dir.join("history.json"), &serde_json::to_vec(h)?)
    }
}

pub fn private_atomic(path: &std::path::Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().context("Missing parent directory")?;
    let created = !parent.exists();
    std::fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        if created {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    let tmp = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
