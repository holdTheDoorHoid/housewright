//! `housewright-bridge`: see `integrations/home-assistant/README.md`.

use std::io::{BufRead, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use clap::{Parser, Subcommand};
use housewright_bridge::http::{Shared, router, stored_token, sync_loop};
use housewright_bridge::{Bridge, PairIn};

/// Housewright bridge for Home Assistant: a household device with a small
/// local API.
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Where the bridge keeps its database, keys and token.
    #[arg(
        long,
        env = "HW_BRIDGE_DATA_DIR",
        default_value = "./housewright-bridge-data",
        global = true
    )]
    data_dir: PathBuf,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the local API (the default).
    Serve {
        /// Address for the local API. Keep it on 127.0.0.1 unless Home
        /// Assistant runs elsewhere (the add-on uses its own network).
        #[arg(long, env = "HW_BRIDGE_LISTEN", default_value = "127.0.0.1:8788")]
        listen: SocketAddr,
        /// The token Home Assistant must send. Default: made on first
        /// start and kept in the data folder as `api-token`.
        #[arg(long, env = "HW_BRIDGE_TOKEN", hide_env_values = true)]
        token: Option<String>,
        /// Seconds between syncs (changes made here sync within seconds).
        #[arg(long, env = "HW_BRIDGE_SYNC_EVERY", default_value_t = 60)]
        sync_every: u64,
    },
    /// Join a household: type the code the app shows, then compare the
    /// six-digit safety number.
    Join {
        /// The sync server's address (not needed if --code is the QR text).
        #[arg(long, default_value = "")]
        server: String,
        #[arg(long)]
        code: String,
        #[arg(long, default_value = housewright_bridge::DEFAULT_DEVICE_NAME)]
        name: String,
    },
    /// Print the local API token (for Home Assistant's setup).
    Token,
    /// Show whether the bridge has joined and when it last synced.
    Status,
    /// Make a sample household with no sync, to try the integration.
    /// Not for real use: nothing here reaches your other devices.
    Demo,
}

fn fail(e: impl std::fmt::Display) -> ! {
    eprintln!("housewright-bridge: {e}");
    std::process::exit(1);
}

fn main() {
    let mut a = Args::parse();
    if a.cmd.is_none() {
        // No subcommand: serve, with its options from the environment.
        a = Args::parse_from(std::env::args().chain(["serve".to_string()]));
    }
    match a.cmd.expect("a subcommand") {
        Cmd::Token => println!("{}", stored_token(&a.data_dir).unwrap_or_else(|e| fail(e))),
        Cmd::Status => {
            let mut b = Bridge::open(&a.data_dir).unwrap_or_else(|e| fail(e));
            let s = b.status().unwrap_or_else(|e| fail(e));
            println!("{}", serde_json::to_string_pretty(&s).expect("json"));
        }
        Cmd::Demo => {
            let mut b = Bridge::open(&a.data_dir).unwrap_or_else(|e| fail(e));
            b.seed_demo().unwrap_or_else(|e| fail(e));
            eprintln!(
                "housewright-bridge: made a demo household in {}",
                a.data_dir.display()
            );
        }
        Cmd::Join { server, code, name } => join(&a.data_dir, server, code, name),
        Cmd::Serve {
            listen,
            token,
            sync_every,
        } => serve(&a.data_dir, listen, token, sync_every),
    }
}

fn join(dir: &std::path::Path, server: String, code: String, name: String) {
    let mut b = Bridge::open(dir).unwrap_or_else(|e| fail(e));
    let mut state = b
        .pair_start(PairIn {
            server: Some(server),
            code,
            device_name: Some(name),
        })
        .unwrap_or_else(|e| fail(e));
    eprintln!("Waiting for the app...");
    for _ in 0..600 {
        match state["state"].as_str() {
            Some("compare") => {
                println!(
                    "Safety number: {}\nCheck that the Housewright app shows the same number.",
                    state["safety_number"].as_str().unwrap_or("?")
                );
                print!("Type yes if they match: ");
                let _ = std::io::stdout().flush();
                let mut line = String::new();
                let _ = std::io::stdin().lock().read_line(&mut line);
                let yes = line.trim().eq_ignore_ascii_case("yes");
                state = b.pair_confirm(yes).unwrap_or_else(|e| fail(e));
                if !yes {
                    fail("stopped: the numbers did not match");
                }
                eprintln!("Now tap \"They match\" in the app.");
            }
            Some("done") => {
                eprintln!("Joined. Start the bridge with `housewright-bridge serve`.");
                return;
            }
            Some("failed") => fail(state["reason"].as_str().unwrap_or("pairing failed")),
            _ => {
                std::thread::sleep(Duration::from_secs(1));
                state = b.pair_poll().unwrap_or_else(|e| fail(e));
            }
        }
    }
    fail("gave up waiting; start again with a new code");
}

#[tokio::main]
async fn serve(dir: &std::path::Path, listen: SocketAddr, token: Option<String>, every: u64) {
    let token = match token.filter(|t| !t.trim().is_empty()) {
        Some(t) if t.trim().len() < 24 => fail("the token must be at least 24 characters"),
        Some(t) => t.trim().to_string(),
        None => stored_token(dir).unwrap_or_else(|e| fail(e)),
    };
    let bridge = Bridge::open(dir).unwrap_or_else(|e| fail(e));
    if !bridge.is_paired() {
        eprintln!(
            "housewright-bridge: not in a household yet. Pair it from Home Assistant's setup, or run `housewright-bridge join`."
        );
    }
    if !listen.ip().is_loopback() {
        eprintln!(
            "housewright-bridge: warning: the local API listens on {listen}, not only this computer. Anyone who can reach it and has the token can read task names and mark tasks done."
        );
    }
    eprintln!(
        "housewright-bridge {} listening on http://{listen}, data in {}",
        env!("CARGO_PKG_VERSION"),
        dir.display()
    );
    let shared = Shared::new(bridge, &token);
    tokio::spawn(sync_loop(
        shared.clone(),
        Duration::from_secs(every.max(10)),
    ));
    let l = tokio::net::TcpListener::bind(listen)
        .await
        .unwrap_or_else(|e| fail(format!("listening on {listen}: {e}")));
    axum::serve(l, router(shared))
        .with_graceful_shutdown(async {
            let ctrl_c = async {
                let _ = tokio::signal::ctrl_c().await;
            };
            #[cfg(unix)]
            let term = async {
                if let Ok(mut s) =
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                {
                    s.recv().await;
                }
            };
            #[cfg(not(unix))]
            let term = std::future::pending::<()>();
            tokio::select! { _ = ctrl_c => {}, _ = term => {} }
        })
        .await
        .unwrap_or_else(|e| fail(e));
}
