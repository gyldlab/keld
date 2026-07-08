//! keld — the developer CLI.
//!
//! Verbs and contracts: `docs/architecture/06-runtime-and-tooling.md` §2.
//! Distributed via `@keld/cli` npm platform packages; async Rust is permitted
//! here (cold tooling) per AGENTS.md.

fn main() {
    let verb = std::env::args().nth(1);
    match verb.as_deref() {
        Some("--version" | "-V") => println!("keld {}", env!("CARGO_PKG_VERSION")),
        Some(other) => {
            eprintln!("keld: `{other}` is not implemented yet (pre-alpha). See ROADMAP.md.");
            std::process::exit(1);
        }
        None => {
            println!("keld — the desktop framework (pre-alpha)");
            println!("planned verbs: create · dev · build · migrate · doctor · gen · ext");
        }
    }
}
