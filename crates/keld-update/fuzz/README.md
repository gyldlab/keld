# KEL-53 canonical archive fuzzer

Run from this directory with:

```sh
cargo +nightly fuzz run canonical_archive -- -max_total_time=60
```

The target feeds raw bytes into the production ustar preflight parser with a
verified-content receipt and the guard-owned lexical Windows component check.
Windows NLS normalization and ordinal path-set comparison are tested separately
on native Windows; this fuzz target does not claim those APIs ran under Linux.
Every crash must retain its minimized corpus input and become a deterministic
regression in `crates/keld-update/src/tests.rs` before its disposition is recorded.
