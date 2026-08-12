//! keld — the developer CLI.
//!
//! Verbs and contracts: `docs/architecture/06-runtime-and-tooling.md` §2.
//! Distributed via `@keld/cli` npm platform packages; async Rust is permitted
//! here (cold tooling) per AGENTS.md.

use std::env;
use std::path::Path;
use std::process;
use std::sync::mpsc;

use keld_cli::create::{CreateError, create_project};
use keld_cli::dev::{find_project_root, run_dev};
use keld_cli::doctor::{all_ok, run_checks};
use keld_cli::echo_link::{EchoServer, echo_roundtrip};
use keld_core::run_hello_window;
use keld_ipc::EchoRequest;

fn main() {
    let args: Vec<String> = env::args().collect();
    let verb = args.get(1).map(String::as_str);

    let result = match verb {
        Some("--version" | "-V") => {
            println!("keld {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Some("hello") => run_hello_window().map_err(|e| e.to_string()),
        Some("ipc-echo") => run_ipc_echo_demo().map_err(|e| e.to_string()),
        Some("create") => run_create(&args[2..]).map_err(|e| e.to_string()),
        Some("dev") => match env::current_dir() {
            Ok(cwd) => {
                let root = find_project_root(&cwd).unwrap_or(cwd);
                run_dev(&root).map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        },
        Some("doctor") => match env::current_dir() {
            Ok(cwd) => {
                let root = find_project_root(&cwd);
                run_doctor_cmd(root.as_deref());
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        },
        Some("ipc-client") => run_ipc_client(&args[2..]),
        Some(other) => {
            eprintln!("keld: unknown command `{other}`");
            print_usage();
            process::exit(1);
        }
        None => {
            print_usage();
            Ok(())
        }
    };

    if let Err(err) = result {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn print_usage() {
    eprintln!("keld {} (pre-alpha)", env!("CARGO_PKG_VERSION"));
    eprintln!("commands:");
    eprintln!("  create <name>   Scaffold the hello-world template");
    eprintln!("  dev             Run the app (Bun main + IPC echo + window)");
    eprintln!("  doctor          Check local toolchain and project layout");
    eprintln!("  hello           Open the macOS WKWebView hello window");
    eprintln!("  ipc-echo        Run the typed kipc echo round-trip demo");
    eprintln!("  ipc-client      Internal: kipc client helpers for templates");
    eprintln!("  --version       Print version");
}

fn run_create(args: &[String]) -> Result<(), CreateError> {
    let Some(name) = args.first() else {
        return Err(CreateError::InvalidName {
            name: String::new(),
            reason: "name is empty",
        });
    };
    let cwd = env::current_dir()?;
    let root = create_project(&cwd, name)?;
    println!("Created keld project at {}", root.display());
    println!("Next: cd {name} && keld dev");
    Ok(())
}

fn run_doctor_cmd(project_root: Option<&Path>) {
    let checks = run_checks(project_root);
    for check in &checks {
        let mark = if check.ok { "ok" } else { "FAIL" };
        println!("[{mark}] {} — {}", check.label, check.detail);
    }
    if !all_ok(&checks) {
        process::exit(1);
    }
}

fn run_ipc_client(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("echo") => run_ipc_client_echo(&args[1..]),
        _ => {
            Err("usage: keld ipc-client echo --link <path> [--message TEXT] [--count N]".to_owned())
        }
    }
}

fn run_ipc_client_echo(args: &[String]) -> Result<(), String> {
    let mut link = None;
    let mut message = "keld".to_owned();
    let mut count = 1u32;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--link" => link = iter.next().cloned(),
            "--message" => {
                message = iter
                    .next()
                    .cloned()
                    .ok_or_else(|| "KELD-CLI-041: --message requires a value".to_owned())?;
            }
            "--count" => {
                let raw = iter
                    .next()
                    .ok_or_else(|| "KELD-CLI-042: --count requires a value".to_owned())?;
                count = raw
                    .parse()
                    .map_err(|_| format!("KELD-CLI-042: --count must be a u32, got `{raw}`"))?;
            }
            other => {
                return Err(format!(
                    "KELD-CLI-043: unknown ipc-client echo flag `{other}`"
                ));
            }
        }
    }
    let Some(link) = link else {
        return Err("KELD-CLI-040: missing --link (set KELD_APP_LINK from `keld dev`)".to_owned());
    };
    let response =
        echo_roundtrip(&link, &EchoRequest { message, count }).map_err(|e| e.to_string())?;
    println!(
        "ipc-echo ok: message={:?} count={}",
        response.message, response.count
    );
    Ok(())
}

/// Runs echo server + client on a loopback app-link (KEL-30 slice).
fn run_ipc_echo_demo() -> Result<(), Box<dyn std::error::Error>> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let server = EchoServer::start(ready_tx);
    ready_rx.recv()?;

    let link = server.link();
    let response = echo_roundtrip(
        &link,
        &EchoRequest {
            message: "keld".to_owned(),
            count: 1,
        },
    )?;

    server.join()?;

    println!(
        "ipc-echo ok: message={:?} count={}",
        response.message, response.count
    );
    Ok(())
}
