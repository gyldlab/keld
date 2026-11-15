//! Isolated process-main regression for a destroyed pending initial WK view.
#[cfg(target_os = "macos")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use keld_wv::wkwebview::WkWebViewEngine;
    use keld_wv::{AppWindowCommand, AppWindowEvent, NavTarget, WebEngine, WebviewSpec, WvError};
    use std::io::{Read, Write};
    use std::sync::mpsc;

    // The parent owns a bound loopback endpoint and withholds its HTTP response.
    let url = std::env::args()
        .nth(1)
        .ok_or("pending loopback URL required")?;
    let mut engine = WkWebViewEngine::new_dev_ephemeral()?;
    let (events, observed_events) = mpsc::channel();
    let id = engine.create_app(
        &WebviewSpec {
            title: String::from("KEL132 pending navigation early destroy"),
            initial: NavTarget::Url(url),
            ..WebviewSpec::default()
        },
        events.clone(),
    )?;
    assert!(
        observed_events.try_recv().is_err(),
        "initial navigation must remain pending"
    );
    engine.destroy(id)?;
    assert!(
        matches!(
            engine.navigate(id, NavTarget::Html(String::new())),
            Err(WvError::UnknownWebview { .. })
        ),
        "real native view must be removed"
    );
    assert!(
        observed_events.try_recv().is_err(),
        "destroyed pending view cannot become ready"
    );
    let (commands, received_commands) = mpsc::channel();
    let control = std::thread::spawn(move || -> std::io::Result<()> {
        let mut byte = [0];
        std::io::stdin().read_exact(&mut byte)?;
        if byte != *b"Q" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "parent must release explicit Quit with Q",
            ));
        }
        commands.send(AppWindowCommand::Quit).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                format!("live loop did not accept Quit: {error}"),
            )
        })?;
        Ok(())
    });
    println!(
        "KEL132_ZERO_VIEW_BEFORE_LOOP pid={} stale_id={} navigation_ready=false",
        std::process::id(),
        id.0
    );
    std::io::stdout().flush()?;
    let result = engine.run_app_until_quit(received_commands, events);
    // Publish the production result before joining a controller still held by the parent.
    println!("KEL132_ZERO_VIEW_LOOP_RETURN result={result:?}");
    std::io::stdout().flush()?;
    result?;
    let queued_events: Vec<_> = observed_events.try_iter().collect();
    assert!(
        !queued_events.contains(&AppWindowEvent::NavigationReady),
        "a late navigation completion must not mask the zero-view wait: {queued_events:?}"
    );
    println!("KEL132_ZERO_VIEW_POST_RUN events={queued_events:?} navigation_ready=false");
    control
        .join()
        .map_err(|_| std::io::Error::other("Quit controller panicked"))??;
    drop(engine);
    println!("KEL132_ZERO_VIEW_DONE explicit_quit=true engine_dropped=true");
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This real-OS regression requires macOS");
    std::process::exit(77);
}
