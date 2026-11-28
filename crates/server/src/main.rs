//! `housewright-server`: see `docs/SELF-HOSTING.md`.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use housewright_server::{AppState, Config, Listen, serve};

/// Housewright sync server. Stores and relays only encrypted data.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Address and port to listen on.
    #[arg(long, env = "HW_LISTEN", default_value = "127.0.0.1:8787")]
    listen: SocketAddr,
    /// Where the database and encrypted files are kept.
    #[arg(long, env = "HW_DATA_DIR", default_value = "./housewright-data")]
    data_dir: PathBuf,
    /// TLS certificate chain (PEM). With --tls-key, serve HTTPS directly.
    #[arg(long, env = "HW_TLS_CERT", requires = "tls_key")]
    tls_cert: Option<PathBuf>,
    /// TLS private key (PEM).
    #[arg(long, env = "HW_TLS_KEY", requires = "tls_cert")]
    tls_key: Option<PathBuf>,
    /// Plain HTTP behind a reverse proxy that does TLS (Caddy, nginx).
    /// Trusts the proxy's X-Forwarded-For for rate limiting.
    #[arg(long, env = "HW_BEHIND_PROXY")]
    behind_proxy: bool,
    /// Path prefix when the proxy does not strip one, e.g. /housewright.
    #[arg(long, env = "HW_BASE_PATH", default_value = "")]
    base_path: String,
    /// If set, a new household needs this token (typed once at setup).
    #[arg(long, env = "HW_REGISTRATION_TOKEN")]
    registration_token: Option<String>,
    /// Most households this server will hold.
    #[arg(long, env = "HW_MAX_HOUSEHOLDS", default_value_t = 20)]
    max_households: u64,
}

#[tokio::main]
async fn main() {
    let a = Args::parse();
    let mut cfg = Config::new(&a.data_dir);
    cfg.behind_proxy = a.behind_proxy;
    cfg.registration_token = a.registration_token.filter(|t| !t.is_empty());
    cfg.max_households = a.max_households;
    let state = match AppState::open(cfg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("housewright-server: {e}");
            std::process::exit(1);
        }
    };
    let listen = match (a.tls_cert, a.tls_key) {
        (Some(cert), Some(key)) => Listen::Tls {
            addr: a.listen,
            cert,
            key,
        },
        _ => {
            if !a.behind_proxy && !a.listen.ip().is_loopback() {
                eprintln!(
                    "housewright-server: warning: plain HTTP on {} without --behind-proxy. \
                     Data stays end-to-end encrypted, but use TLS or a proxy for anything \
                     beyond a home network.",
                    a.listen
                );
            }
            Listen::Plain(a.listen)
        }
    };
    eprintln!(
        "housewright-server {} listening on {} ({}), data in {}",
        env!("CARGO_PKG_VERSION"),
        a.listen,
        match &listen {
            Listen::Tls { .. } => "https",
            Listen::Plain(_) => "http",
        },
        a.data_dir.display()
    );
    if let Err(e) = serve(state, &a.base_path, listen).await {
        eprintln!("housewright-server: {e}");
        std::process::exit(1);
    }
}
