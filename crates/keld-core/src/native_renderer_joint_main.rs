//! Opt-in process-main acceptance executable; production owns every actual facility.
#[cfg(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions))]
pub mod app_session;

#[cfg(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions))]
mod macos_profile_identity;

// The owning source's Linux-only documentation points to this existing public primitive.
#[cfg(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions))]
pub use keld_core::prepare_webview_process;

const NATIVE_RENDERER_CASE: &str = "actual_renderer_native_held_same_document_recovers_g2";

fn select_native_renderer_case(args: &[String]) -> Result<bool, &'static str> {
    if args.iter().any(|arg| arg == "--list") {
        println!("{NATIVE_RENDERER_CASE}: test");
        let terse = args.iter().any(|arg| arg == "--format=terse")
            || args.windows(2).any(|pair| pair == ["--format", "terse"]);
        if !terse {
            println!("\n1 test, 0 benchmarks");
        }
        return Ok(false);
    }
    let exact = args.iter().any(|arg| arg == "--exact");
    let ignored = args.iter().any(|arg| arg == "--ignored");
    let selected = args.iter().enumerate().any(|(index, arg)| {
        arg == NATIVE_RENDERER_CASE
            && (index == 0
                || !matches!(
                    args[index - 1].as_str(),
                    "--skip" | "--format" | "--color" | "--test-threads"
                ))
    });
    if ignored && exact && !selected {
        return Err("physical joint execution requires the literal case selector");
    }
    let skip_matches = |pattern: &str| {
        if exact {
            pattern == NATIVE_RENDERER_CASE
        } else {
            NATIVE_RENDERER_CASE.contains(pattern)
        }
    };
    let skipped = args
        .windows(2)
        .any(|pair| pair[0] == "--skip" && skip_matches(pair[1].as_str()))
        || args
            .iter()
            .any(|arg| arg.strip_prefix("--skip=").is_some_and(skip_matches));
    if skipped || !selected || !ignored {
        println!(
            "{NATIVE_RENDERER_CASE} ... ignored, explicit physical joint lane\ntest result: ok. 0 passed; 0 failed; 1 ignored;"
        );
        return Ok(false);
    }
    if !exact
        || !args.iter().all(|arg| {
            matches!(
                arg.as_str(),
                NATIVE_RENDERER_CASE | "--exact" | "--ignored" | "--nocapture" | "--test-threads=1"
            )
        })
    {
        return Err(
            "physical joint execution requires the exact ignored selector and known arguments",
        );
    }
    Ok(true)
}

fn main() -> std::process::ExitCode {
    // `std::env::args()` panics on non-Unicode argv; refuse it through the
    // shared KELD-CLI-051 decoder with the same exit 2 as the `keld` and
    // `keld-host` binaries.
    let args = match keld_core::unicode_args(std::env::args_os()) {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::from(2);
        }
    };
    match select_native_renderer_case(args.get(1..).unwrap_or_default()) {
        Ok(false) => return std::process::ExitCode::SUCCESS,
        Ok(true) => {}
        Err(detail) => {
            eprintln!("KELD-TEST-MAIN: {detail}");
            return std::process::ExitCode::FAILURE;
        }
    }
    #[cfg(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions))]
    {
        app_session::native_renderer_main_thread_entry();
        std::process::ExitCode::SUCCESS
    }
    #[cfg(not(all(keld_core_native_renderer_main, target_os = "macos", debug_assertions)))]
    {
        eprintln!(
            "KELD-TEST-MAIN: missing checked macOS debug acceptance-target identity. Build the exact opt-in test with --cfg keld_core_native_renderer_main and its registered check-cfg; ordinary Cargo/libtest is not the main-thread runner."
        );
        std::process::ExitCode::FAILURE
    }
}
