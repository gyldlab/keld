#[test]
#[ignore = "requires a coordinated native Windows test slot"]
fn desktop_capture_refusal_preserves_identity_history() {
    let worker = std::thread::spawn(|| {
        let mut sampler = DesktopSampler::default();
        for target_id in 1..=2 {
            assert_eq!(sampler.display_id(target(target_id)).unwrap(), target_id);
        }
        let before = history(&sampler);

        // This fresh thread creates no window and changes no DPI context.
        // On the qualified single-primary desktop, it fails the inherited-
        // awareness guard or enumerates no windows. Other failures stay explicit.
        // EnumThreadWindows returns FALSE when the thread has no windows.
        for attempt in 1..=2 {
            let error = match sampler.capture() {
                Ok(_) => panic!("a windowless thread must not publish desktop facts"),
                Err(error) => error,
            };
            assert!(
                matches!(
                    &error,
                    crate::WvError::Window(reason)
                        if matches!(
                            reason.as_str(),
                            "Windows desktop unavailable: UI thread must already be per-monitor V2 DPI aware"
                                | "Windows desktop unavailable: could not verify a consistent Host UI-thread monitor DPI"
                        )
                ),
                "unexpected capture refusal on attempt {attempt}: {error:?}"
            );
            assert_eq!(
                history(&sampler),
                before,
                "failed capture {attempt} must not change display identity history"
            );
        }

        for target_id in (1..=2).rev() {
            assert_eq!(sampler.display_id(target(target_id)).unwrap(), target_id);
        }
        assert_eq!(history(&sampler), before);
        assert_eq!(
            sampler.display_id(target(3)).unwrap(),
            3,
            "failed captures must not consume the next display ID"
        );
        assert_eq!(sampler.known.len(), 3);
    });
    if let Err(payload) = worker.join() {
        std::panic::resume_unwind(payload);
    }
}
