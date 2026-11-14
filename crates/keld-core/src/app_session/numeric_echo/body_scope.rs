//! Field order is the actual read-return barrier: both canonical body backings
//! end before the metadata guard may publish `ReadEnded`, including unwind.
pub(super) struct ReadScope<R, D, G> {
    pub(super) raw: R,
    pub(super) decoded: D,
    pub(super) guard: G,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Buffer(Rc<Cell<u8>>);
    impl Drop for Buffer {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct ReadEnd {
        disposed: Rc<Cell<u8>>,
        observed: Rc<Cell<Option<u8>>>,
    }
    impl Drop for ReadEnd {
        fn drop(&mut self) {
            self.observed.set(Some(self.disposed.get()));
        }
    }

    fn run(panic: bool) {
        let disposed = Rc::new(Cell::new(0));
        let observed = Rc::new(Cell::new(None));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _scope = ReadScope {
                raw: Buffer(Rc::clone(&disposed)),
                decoded: Buffer(Rc::clone(&disposed)),
                guard: ReadEnd {
                    disposed: Rc::clone(&disposed),
                    observed: Rc::clone(&observed),
                },
            };
            assert!(!panic, "actual scope unwind");
            Err::<(), _>("canonical decode refusal")
        }));
        assert_eq!(
            observed.get(),
            Some(2),
            "ReadEnded cannot precede either body disposal"
        );
        assert_eq!(disposed.get(), 2);
        assert_eq!(result.is_err(), panic);
    }

    #[test]
    fn raw_and_decoded_backing_end_before_guard_on_return() {
        run(false);
    }
    #[test]
    fn raw_and_decoded_backing_end_before_guard_on_unwind() {
        run(true);
    }
}
