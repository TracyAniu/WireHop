//! `send` and `receive` subcommands: a real protocol peer on a real socket.
//!
//! These exist so the C++/Qt suite can run a live session against the Rust
//! core (`tests/tst_interop.cpp`). Output is line-oriented and machine-read by
//! that test — keep it stable, and keep diagnostics on stderr so stdout stays
//! parseable.

use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::ExitCode;

use wirehop_core::session::{self, SendOutcome};

/// Minimal `--flag value` parsing. A real CLI would take a dependency for
/// this; the interop harness needs four flags and no help text.
struct Args {
    flags: Vec<(String, String)>,
    positional: Vec<String>,
}

impl Args {
    fn parse(raw: &[String]) -> Self {
        let mut flags = Vec::new();
        let mut positional = Vec::new();
        let mut i = 0;
        while i < raw.len() {
            if let Some(name) = raw[i].strip_prefix("--") {
                if i + 1 < raw.len() && !raw[i + 1].starts_with("--") {
                    flags.push((name.to_string(), raw[i + 1].clone()));
                    i += 2;
                    continue;
                }
                flags.push((name.to_string(), String::new()));
            } else {
                positional.push(raw[i].clone());
            }
            i += 1;
        }
        Self { flags, positional }
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }
}

pub fn send(raw: &[String]) -> ExitCode {
    let args = Args::parse(raw);
    let host = args.get("host").unwrap_or("127.0.0.1").to_string();
    let Some(port) = args.get("port").and_then(|p| p.parse::<u16>().ok()) else {
        eprintln!("send: --port is required");
        return ExitCode::from(2);
    };
    let name = args.get("name").unwrap_or("rust-core").to_string();
    let paths: Vec<PathBuf> = args.positional.iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        eprintln!("send: at least one file is required");
        return ExitCode::from(2);
    }

    let mut stream = match TcpStream::connect((host.as_str(), port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("send: connect failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    match session::send_files(&mut stream, &name, &paths) {
        Ok(outcome) => {
            println!(
                "outcome {}",
                match outcome {
                    SendOutcome::Confirmed => "confirmed",
                    SendOutcome::SentUnconfirmed => "unconfirmed",
                    SendOutcome::Rejected => "rejected",
                }
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("send: {e}");
            ExitCode::FAILURE
        }
    }
}

pub fn receive(raw: &[String]) -> ExitCode {
    let args = Args::parse(raw);
    let port: u16 = args.get("port").and_then(|p| p.parse().ok()).unwrap_or(0);
    let Some(dir) = args.get("dir").map(PathBuf::from) else {
        eprintln!("receive: --dir is required");
        return ExitCode::from(2);
    };
    let accept = !args.has("reject");

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("receive: bind failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    // Announce the bound port before blocking, so a caller that passed port 0
    // can connect. Flushing matters: the caller is reading this line.
    match listener.local_addr() {
        Ok(addr) => {
            println!("listening {}", addr.port());
            use std::io::Write;
            let _ = std::io::stdout().flush();
        }
        Err(e) => {
            eprintln!("receive: local_addr failed: {e}");
            return ExitCode::FAILURE;
        }
    }

    let mut stream = match listener.accept() {
        Ok((s, _)) => s,
        Err(e) => {
            eprintln!("receive: accept failed: {e}");
            return ExitCode::FAILURE;
        }
    };

    match session::receive_files(&mut stream, &dir, accept) {
        Ok(outcome) => {
            println!("device {}", outcome.device_name);
            println!("code {}", outcome.session_code);
            println!("version {}", outcome.peer.version);
            println!(
                "caps {}",
                outcome
                    .peer
                    .caps
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            );
            for path in &outcome.files {
                println!("file {}", path.display());
            }
            println!(
                "outcome {}",
                if outcome.accepted {
                    "accepted"
                } else {
                    "rejected"
                }
            );
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("receive: {e}");
            ExitCode::FAILURE
        }
    }
}
