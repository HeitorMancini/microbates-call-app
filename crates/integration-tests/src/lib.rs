//! Helpers for running a real `livekit-server` and an in-process token-server in tests.

use std::net::{SocketAddr, TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use tokio::sync::oneshot;

pub const API_KEY: &str = "devkey";
pub const API_SECRET: &str = "devsecretdevsecretdevsecretdevsecret";

fn exe_name() -> &'static str {
    if cfg!(windows) {
        "livekit-server.exe"
    } else {
        "livekit-server"
    }
}

/// Looks for `livekit-server` in `LIVEKIT_SERVER_BIN`, `PATH` and `~/.local/bin`.
pub fn find_livekit_server() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("LIVEKIT_SERVER_BIN").map(PathBuf::from) {
        return path.is_file().then_some(path);
    }
    let from_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|dir| dir.join(exe_name()));
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let from_home = home.map(|h| Path::new(&h).join(".local").join("bin").join(exe_name()));
    from_path.chain(from_home).find(|p| p.is_file())
}

pub fn free_tcp_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

pub fn free_udp_port() -> std::io::Result<u16> {
    Ok(UdpSocket::bind("127.0.0.1:0")?.local_addr()?.port())
}

/// A `livekit-server` child process bound to localhost; killed on drop.
pub struct LiveKitServer {
    child: Child,
    pub port: u16,
}

impl LiveKitServer {
    pub async fn start(bin: &Path, log_file: &Path) -> std::io::Result<Self> {
        let port = free_tcp_port()?;
        let config = format!(
            "port: {port}\n\
             bind_addresses: [\"127.0.0.1\"]\n\
             rtc:\n  tcp_port: {tcp}\n  udp_port: {udp}\n  use_external_ip: false\n\
             keys:\n  {API_KEY}: {API_SECRET}\n\
             logging:\n  level: info\n",
            tcp = free_tcp_port()?,
            udp = free_udp_port()?,
        );
        let log = std::fs::File::create(log_file)?;
        let child = Command::new(bin)
            .arg("--config-body")
            .arg(&config)
            .arg("--node-ip")
            .arg("127.0.0.1")
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        let mut server = Self { child, port };
        server.wait_ready(Duration::from_secs(30)).await?;
        Ok(server)
    }

    async fn wait_ready(&mut self, limit: Duration) -> std::io::Result<()> {
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait()? {
                return Err(std::io::Error::other(format!(
                    "livekit-server exited early: {status}"
                )));
            }
            if tokio::net::TcpStream::connect(("127.0.0.1", self.port))
                .await
                .is_ok()
            {
                return Ok(());
            }
            if tokio::time::Instant::now() > deadline {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "livekit-server did not start listening",
                ));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}

impl Drop for LiveKitServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// An in-process token-server; shuts down on drop.
pub struct TokenServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl TokenServer {
    pub async fn start(livekit_url: &str, api_secret: &str) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let config = token_server::Config {
            api_key: API_KEY.to_owned(),
            api_secret: api_secret.to_owned(),
            livekit_url: livekit_url.to_owned(),
            bind: addr,
            token_ttl: Duration::from_secs(600),
            access_password: None,
        };
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(token_server::serve(listener, config, async {
            let _ = rx.await;
        }));
        Ok(Self {
            addr,
            shutdown: Some(tx),
        })
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for TokenServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}
