// Windows filesystem tests are grouped by their real OS/security contract.
// One test function per original selector; no include-based pseudo-extraction.

mod creation;
mod deletion;
mod enumeration;
mod retained_parent;
mod support;
mod volume;
