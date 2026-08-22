//! Drop-order contract shared by wry-backed [`View`] structs.
//!
//! Platform bindings require the webview to release before the host window
//! closes. Rust drops struct fields in declaration order, so `webview` MUST be
//! declared before `window` in every `View` struct.

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::mem::offset_of;
    use std::rc::Rc;

    struct DropTag(&'static str, Rc<RefCell<Vec<&'static str>>>);

    impl Drop for DropTag {
        fn drop(&mut self) {
            self.1.borrow_mut().push(self.0);
        }
    }

    /// Mirrors field order required by `wkwebview::View` and `webkitgtk::View`.
    struct MirrorView {
        webview: DropTag,
        window: DropTag,
    }

    #[test]
    fn wry_view_drops_webview_before_window() {
        let log = Rc::new(RefCell::new(Vec::new()));
        {
            let _view = MirrorView {
                webview: DropTag("webview", log.clone()),
                window: DropTag("window", log.clone()),
            };
        }
        assert_eq!(&*log.borrow(), &["webview", "window"]);
    }

    #[test]
    fn mirror_field_order_matches_documented_contract() {
        assert!(offset_of!(MirrorView, webview) < offset_of!(MirrorView, window));
    }
}
