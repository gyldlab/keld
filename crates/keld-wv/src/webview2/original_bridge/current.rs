//! The existing one current publication/input position. Taking it for a native
//! turn retains the whole owner locally; the position remains visibly occupied.
use super::control::ControlWork;
use super::intake::PendingSendInput;
use super::numeric::NumericWork;
use super::{Cell, OriginalBridge, WvError, failure};

#[expect(
    clippy::large_enum_variant,
    reason = "One prepaid current slot owns its complete operation; boxing would add an unaccounted hot-path allocation."
)]
pub(super) enum CurrentOperation {
    Control(ControlWork),
    Numeric(NumericWork),
    Send(PendingSendInput),
}
pub(super) type CurrentSlot = NativePosition<CurrentOperation>;

enum Custody<T> {
    Empty,
    Retained(T),
    NativeEntry,
}
#[derive(Clone, Copy)]
pub(super) enum Action {
    Keep,
    Dispose,
}

pub(super) struct NativePosition<T> {
    custody: Cell<Custody<T>>,
}
impl<T> NativePosition<T> {
    pub(super) const fn new() -> Self {
        Self {
            custody: Cell::new(Custody::Empty),
        }
    }
    pub(super) fn is_empty(&self) -> bool {
        let value = self.custody.replace(Custody::NativeEntry);
        let empty = matches!(&value, Custody::Empty);
        self.custody.set(value);
        empty
    }
    /// Scoped read of this same retained operation. Nothing can borrow its body
    /// after the closure; an in-call position is never observed as empty.
    pub(super) fn observe<R>(&self, read: impl FnOnce(Option<&T>) -> R) -> Result<R, WvError> {
        let owned = self.custody.replace(Custody::NativeEntry);
        if matches!(&owned, Custody::NativeEntry) {
            return Err(failure("current observation met its actual native entry"));
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            read(match &owned {
                Custody::Empty => None,
                Custody::Retained(value) => Some(value),
                Custody::NativeEntry => unreachable!(),
            })
        }));
        self.custody.set(owned);
        result.map_err(|_| failure("current observation panicked with custody retained"))
    }
    /// Conversion is pure and infallible after borrowed prevalidation. Reentry
    /// sees `NativeEntry` and cannot replace the original operation or take input.
    pub(super) fn attach<I>(&self, input: &mut Option<I>, convert: impl FnOnce(I) -> T) -> bool {
        let old = self.custody.replace(Custody::NativeEntry);
        match old {
            Custody::Empty => {
                if let Some(value) = input.take() {
                    self.custody.set(Custody::Retained(convert(value)));
                    true
                } else {
                    self.custody.set(Custody::Empty);
                    false
                }
            }
            retained => {
                self.custody.set(retained);
                false
            }
        }
    }

    /// Fallible borrowed conversion preserves input on refusal and attaches the
    /// resulting owner before the occupied position is made visible again.
    pub(super) fn attach_checked<I>(
        &self,
        input: &mut Option<I>,
        convert: impl FnOnce(&mut Option<I>) -> Result<Option<T>, WvError>,
    ) -> Result<bool, WvError> {
        let old = self.custody.replace(Custody::NativeEntry);
        match old {
            Custody::Empty => match convert(input) {
                Ok(Some(value)) => {
                    self.custody.set(Custody::Retained(value));
                    Ok(true)
                }
                Ok(None) => {
                    self.custody.set(Custody::Empty);
                    Ok(false)
                }
                Err(error) => {
                    self.custody.set(Custody::Empty);
                    Err(error)
                }
            },
            retained => {
                self.custody.set(retained);
                Ok(false)
            }
        }
    }

    /// The real T is outside catch's capture, so every error/unwind restores the
    /// same owner. No `RefCell` guard crosses the operation's native entry.
    pub(super) fn drive(
        &self,
        run: impl FnOnce(&mut T) -> Result<Action, WvError>,
    ) -> Result<bool, WvError> {
        let old = self.custody.replace(Custody::NativeEntry);
        let mut owned = match old {
            Custody::Empty => {
                self.custody.set(Custody::Empty);
                return Ok(false);
            }
            Custody::NativeEntry => return Err(failure("native current position was reentered")),
            Custody::Retained(value) => value,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut owned)));
        match result {
            Ok(Ok(Action::Dispose)) => {
                // A disposal panic cannot make this position appear empty.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(owned))).is_err() {
                    return Err(failure("native current disposal lost its return proof"));
                }
                self.custody.set(Custody::Empty);
                Ok(true)
            }
            result => {
                self.custody.set(Custody::Retained(owned));
                match result {
                    Ok(Ok(Action::Keep)) => Ok(false),
                    Ok(Err(error)) => Err(error),
                    _ => Err(failure("native current turn panicked with owner retained")),
                }
            }
        }
    }
}
impl CurrentOperation {
    pub(super) fn drive(&mut self, bridge: &OriginalBridge) -> Result<Action, WvError> {
        match self {
            Self::Control(work) => work.drive(bridge),
            Self::Numeric(work) => work.drive(bridge),
            Self::Send(work) => work.drive(bridge),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;
    #[test]
    fn reentry_and_failure_keep_the_exact_current_owner() {
        struct Payload(Rc<Cell<u32>>);
        impl Drop for Payload {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let slot = NativePosition::new();
        let drops = Rc::new(Cell::new(0));
        let mut first = Some(Payload(drops.clone()));
        assert!(slot.attach(&mut first, |v| v));
        assert!(
            slot.drive(|_| {
                assert!(!slot.is_empty());
                assert!(slot.observe(|_| ()).is_err());
                let mut second = Some(Payload(drops.clone()));
                assert!(!slot.attach(&mut second, |v| v));
                assert!(second.is_some());
                drop(second);
                Err(failure("modeled failure"))
            })
            .is_err()
        );
        assert!(!slot.is_empty());
        assert_eq!(drops.get(), 1);
        assert!(slot.observe(|value| value.is_some()).unwrap());
        assert!(
            slot.observe::<()>(|_| panic!("modeled observation unwind"))
                .is_err()
        );
        assert!(!slot.is_empty());
        assert_eq!(drops.get(), 1);
        assert!(slot.drive(|_| panic!("modeled native unwind")).is_err());
        assert!(!slot.is_empty());
        assert_eq!(drops.get(), 1);
        assert!(slot.drive(|_| Ok(Action::Dispose)).unwrap());
        assert!(slot.is_empty());
        assert_eq!(drops.get(), 2);
    }
}
