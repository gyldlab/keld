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
use keld_cli::doctor::{all_ok, findings_all_ok, findings_json, run_checks, run_findings};
use keld_cli::echo_link::{EchoServer, echo_roundtrip};
use keld_cli::flags::{parse_create_args, parse_dev_flags, parse_doctor_flags, parse_hello_flags};
use keld_cli::mcp::run_mcp_serve;
use keld_cli::verb::{ReservedVerb, ReservedVerbError, UnknownVerbError};
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
        Some("hello") => match parse_hello_flags(&args[2..]) {
            Ok(()) => keld_core::prepare_webview_process()
                .and_then(|()| run_hello_window())
                .map_err(|e| e.to_string()),
            Err(err) => {
                eprintln!("{err}");
                process::exit(2);
            }
        },
        Some("ipc-echo") => run_ipc_echo_demo().map_err(|e| e.to_string()),
        Some("create") => match parse_create_args(&args[2..]) {
            Ok(name) => run_create(name.unwrap_or("")).map_err(|e| e.to_string()),
            Err(err) => {
                eprintln!("{err}");
                process::exit(2);
            }
        },
        Some("dev") => match parse_dev_flags(&args[2..]) {
            Ok(()) => match env::current_dir() {
                Ok(cwd) => find_project_root(&cwd)
                    .map_err(|error| error.to_string())
                    .and_then(|root| run_dev(&root.unwrap_or(cwd)).map_err(|e| e.to_string())),
                Err(e) => Err(e.to_string()),
            },
            Err(err) => {
                eprintln!("{err}");
                process::exit(2);
            }
        },
        Some("doctor") => match env::current_dir() {
            Ok(cwd) => find_project_root(&cwd)
                .map_err(|error| error.to_string())
                .map(|root| run_doctor_cmd(root.as_deref(), &args[2..])),
            Err(e) => Err(e.to_string()),
        },
        Some("mcp") => run_mcp_cmd(&args[2..]),
        Some("ipc-client") => run_ipc_client(&args[2..]),
        Some(other) => {
            if let Some(verb) = ReservedVerb::parse(other) {
                eprintln!("{}", ReservedVerbError { verb });
            } else {
                eprintln!(
                    "{}",
                    UnknownVerbError {
                        verb: other.to_owned(),
                    }
                );
            }
            print_usage();
            process::exit(2);
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
    eprintln!("  doctor [--json] Check local toolchain and project layout");
    eprintln!("  mcp serve       Speak MCP over stdio (doctor/docs/permissions)");
    eprintln!("  hello           Open the hello window (WKWebView on macOS, WebView2 on Windows)");
    eprintln!("  ipc-echo        Run the typed kipc echo round-trip demo");
    eprintln!("  ipc-client      Internal: kipc client helpers for templates");
    eprintln!("  --version       Print version");
    eprintln!("reserved (not live): build, migrate, gen, ext");
}

fn run_create(name: &str) -> Result<(), CreateError> {
    if name.is_empty() {
        return Err(CreateError::InvalidName {
            name: String::new(),
            reason: "name is empty",
        });
    }
    let cwd = env::current_dir()?;
    let root = create_project(&cwd, name)?;
    println!("Created keld project at {}", root.display());
    println!("Next: cd {name} && keld dev");
    Ok(())
}

fn print_mcp_usage() {
    eprintln!("usage: keld mcp serve");
    eprintln!("  serve    Start the stdio MCP server (MCP 2026-07-28)");
}

fn run_mcp_cmd(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("serve") => {
            let cwd = env::current_dir().map_err(|e| e.to_string())?;
            let root = find_project_root(&cwd).map_err(|error| error.to_string())?;
            run_mcp_serve(root.as_deref()).map_err(|e| e.to_string())
        }
        Some(other) => {
            eprintln!("keld mcp: unknown subcommand `{other}`");
            print_mcp_usage();
            process::exit(2);
        }
        None => {
            eprintln!("keld mcp: missing subcommand");
            print_mcp_usage();
            process::exit(2);
        }
    }
}

fn run_doctor_cmd(project_root: Option<&Path>, args: &[String]) {
    let json = match parse_doctor_flags(args) {
        Ok(json) => json,
        Err(err) => {
            eprintln!("{err}");
            process::exit(2);
        }
    };
    if json {
        match findings_json(project_root) {
            Ok(payload) => {
                println!("{payload}");
                if !findings_all_ok(&run_findings(project_root)) {
                    process::exit(1);
                }
            }
            Err(e) => {
                eprintln!(
                    "KELD-MCP020: failed to serialize doctor findings — {e}. \
                     Re-run `keld doctor` without --json or report a bug."
                );
                process::exit(1);
            }
        }
        return;
    }

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
            "--link" => {
                link = Some(iter.next().cloned().ok_or_else(|| {
                    "KELD-CLI-040: --link requires a value. Pass `--link <path>`.".to_owned()
                })?);
            }
            "--message" => {
                message = iter.next().cloned().ok_or_else(|| {
                    "KELD-CLI-041: --message requires a value. Pass `--message <text>`.".to_owned()
                })?;
            }
            "--count" => {
                let raw = iter.next().ok_or_else(|| {
                    "KELD-CLI-042: --count requires a value. Pass `--count <u32>`.".to_owned()
                })?;
                count = raw.parse().map_err(|_| {
                    format!(
                        "KELD-CLI-042: --count must be a u32, got `{raw}`. Pass `--count <u32>`."
                    )
                })?;
            }
            other => {
                return Err(format!(
                    "KELD-CLI-043: unknown ipc-client echo flag `{other}`. \
                     Use only `--link`, `--message`, and `--count`."
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

/// Runs echo server + client on the platform bootstrap app-link (KEL-30 slice).
fn run_ipc_echo_demo() -> Result<(), Box<dyn std::error::Error>> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let server = EchoServer::start(&ready_tx)?;
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
