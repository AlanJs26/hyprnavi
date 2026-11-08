use std::env;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceBasic {
    pub id: i32,
    pub name: String,
}

impl<'de> Deserialize<'de> for WorkspaceBasic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawWorkspace {
            #[serde(default)]
            id: Option<i32>,
            #[serde(default)]
            address: Option<String>,
            #[serde(default)]
            name: Option<String>,
        }

        let raw = RawWorkspace::deserialize(deserializer)?;
        let name = raw.name.unwrap_or_default();
        let id = raw.id.unwrap_or_else(|| {
            raw.address
                .as_deref()
                .and_then(|a| a.parse::<i32>().ok())
                .unwrap_or(0)
        });

        Ok(WorkspaceBasic { id, name })
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Client {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub at: (i32, i32),
    #[serde(default)]
    pub size: (i32, i32),
    #[serde(default)]
    pub workspace: WorkspaceBasic,
    #[serde(default)]
    pub floating: bool,
}

impl Client {
    pub fn get_active() -> anyhow::Result<Option<Client>> {
        let res = send_socket("j/activewindow")?;
        let trimmed = res.trim();
        if trimmed == "{}" || trimmed.is_empty() {
            return Ok(None);
        }
        let client: Client = serde_json::from_str(trimmed)?;
        Ok(Some(client))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Clients(pub Vec<Client>);

impl std::ops::Deref for Clients {
    type Target = Vec<Client>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Clients {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> IntoIterator for &'a Clients {
    type Item = &'a Client;
    type IntoIter = std::slice::Iter<'a, Client>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Clients {
    pub fn get() -> anyhow::Result<Self> {
        let res = send_socket("j/clients")?;
        let list: Vec<Client> = serde_json::from_str(res.trim())?;
        Ok(Clients(list))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Monitor {
    #[serde(default)]
    pub id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
    #[serde(default)]
    pub width: i32,
    #[serde(default)]
    pub height: i32,
    #[serde(default)]
    pub reserved: (i32, i32, i32, i32),
    #[serde(default, rename = "activeWorkspace")]
    pub active_workspace: WorkspaceBasic,
    #[serde(default)]
    pub focused: bool,
}

impl Monitor {
    pub fn get_active() -> anyhow::Result<Monitor> {
        let monitors = Monitors::get()?;
        monitors
            .0
            .into_iter()
            .find(|m| m.focused)
            .ok_or_else(|| anyhow::anyhow!("No active monitor found"))
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Monitors(pub Vec<Monitor>);

impl std::ops::Deref for Monitors {
    type Target = Vec<Monitor>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Monitors {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> IntoIterator for &'a Monitors {
    type Item = &'a Monitor;
    type IntoIter = std::slice::Iter<'a, Monitor>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl Monitors {
    pub fn get() -> anyhow::Result<Self> {
        let res = send_socket("j/monitors")?;
        let list: Vec<Monitor> = serde_json::from_str(res.trim())?;
        Ok(Monitors(list))
    }
}

pub fn get_gaps_out() -> i32 {
    let Ok(res) = send_socket("j/getoption general:gaps_out") else {
        return 0;
    };
    #[derive(Deserialize)]
    struct OptionRaw {
        #[serde(default)]
        int: Option<i64>,
        #[serde(default)]
        float: Option<f64>,
        #[serde(default)]
        css: Option<String>,
    }
    let Ok(opt) = serde_json::from_str::<OptionRaw>(res.trim()) else {
        return 0;
    };
    if let Some(css) = opt.css {
        if let Some(first) = css.split_whitespace().next() {
            if let Ok(val) = first.parse::<i32>() {
                return val;
            }
        }
    }
    if let Some(i) = opt.int {
        return i as i32;
    }
    if let Some(f) = opt.float {
        return f as i32;
    }
    0
}

pub fn dispatch(cmd: &str, args: &str) -> anyhow::Result<()> {
    let full_cmd = if args.is_empty() {
        format!("/dispatch {}", cmd)
    } else {
        format!("/dispatch {} {}", cmd, args)
    };
    let res = send_socket(&full_cmd)?;
    let trimmed = res.trim();
    if trimmed == "ok" || trimmed.starts_with("info:") {
        Ok(())
    } else {
        anyhow::bail!("Hyprland dispatch failed: {}", trimmed)
    }
}

fn get_socket_path() -> anyhow::Result<PathBuf> {
    let his = env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .map_err(|_| anyhow::anyhow!("HYPRLAND_INSTANCE_SIGNATURE not set! Is Hyprland running?"))?;

    if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR") {
        let p = PathBuf::from(runtime_dir)
            .join("hypr")
            .join(&his)
            .join(".socket.sock");
        if p.exists() {
            return Ok(p);
        }
    }

    // Fallback: search in /run/user/*/hypr/$HIS/.socket.sock
    if let Ok(entries) = std::fs::read_dir("/run/user") {
        for entry in entries.flatten() {
            let p = entry.path().join("hypr").join(&his).join(".socket.sock");
            if p.exists() {
                return Ok(p);
            }
        }
    }

    anyhow::bail!("Could not find Hyprland socket for instance {his}");
}

fn send_socket(cmd: &str) -> anyhow::Result<String> {
    let path = get_socket_path()?;
    let mut stream = UnixStream::connect(path)?;
    stream.write_all(cmd.as_bytes())?;

    let mut response = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        response.extend_from_slice(&buf[..n]);
        if n < buf.len() {
            break;
        }
    }

    Ok(String::from_utf8(response)?)
}
