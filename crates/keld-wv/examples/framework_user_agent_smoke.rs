//! Dedicated debug child for the first-party macOS UA observation gate.
#[cfg(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions))]
fn main() {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut arguments = std::env::args();
        let _executable = arguments.next();
        let case = arguments
            .next()
            .ok_or("expected exactly one native fixture case")?;
        if arguments.next().is_some() {
            return Err("unexpected native fixture arguments".into());
        }
        let report = keld_wv::wkwebview::run_application_name_fixture_child(&case)?;
        println!("{}", serde_json::to_string(&report)?);
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("GH476 native UA fixture failed: {error}");
        std::process::exit(1);
    }
}
#[cfg(not(all(target_os = "macos", feature = "profile-test-hooks", debug_assertions)))]
fn main() {
    eprintln!(
        "GH476 native UA fixture requires a macOS debug build with profile-test-hooks; no native proof performed"
    );
    std::process::exit(2);
}
