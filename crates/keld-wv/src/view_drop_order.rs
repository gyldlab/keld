//! Drop-order contract shared by wry-backed [`View`] structs.
//!
//! Platform bindings require the webview to release before the host window
//! closes. Rust drops struct fields in declaration order, so `webview` MUST be
//! declared before `window` in every `View` struct.

/// Asserts that `$ty` declares `webview` before `window`.
///
/// Each wry-backed backend MUST call this from a `#[cfg(test)]` module against
/// its real [`View`] type so swapping fields fails CI on that platform.
macro_rules! assert_wry_view_field_order {
    ($ty:ty) => {
        assert!(
            std::mem::offset_of!($ty, webview) < std::mem::offset_of!($ty, window),
            "wry View must declare webview before window so drop releases webview first"
        );
    };
}
pub(crate) use assert_wry_view_field_order;

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    struct DropTag(&'static str, Rc<RefCell<Vec<&'static str>>>);

    impl Drop for DropTag {
        fn drop(&mut self) {
            self.1.borrow_mut().push(self.0);
        }
    }

    /// Stand-in for any two-field struct; proves Rust's drop-order rule that
    /// backend `View` layouts rely on — not a substitute for backend tests.
    #[expect(dead_code, reason = "fields exist only to observe Drop order")]
    struct TwoFieldStruct {
        first: DropTag,
        second: DropTag,
    }

    #[test]
    fn rust_drops_struct_fields_in_declaration_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        {
            let _value = TwoFieldStruct {
                first: DropTag("first", log.clone()),
                second: DropTag("second", log.clone()),
            };
        }
        assert_eq!(&*log.borrow(), &["first", "second"]);
    }
}
