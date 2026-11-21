// Windows filesystem tests are grouped by their real OS/security contract.
// One test function per original selector; no include-based pseudo-extraction.

mod creation;
mod deletion;
mod enumeration;
mod repository_record_boundary;
mod rename_binding;
mod retained_parent;
mod support;
mod volume;
