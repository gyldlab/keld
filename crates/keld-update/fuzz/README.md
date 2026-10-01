# KEL-53 updater fuzzers

Run either bounded raw-byte target from this directory:

```sh
cargo +nightly fuzz run canonical_archive -- -max_total_time=60
cargo +nightly fuzz run activation_journal -- -max_total_time=60 -max_len=65536
```

`canonical_archive` feeds raw bytes into the production ustar preflight parser with
a verified-content receipt and the guard-owned lexical Windows component check.
Windows NLS normalization and ordinal path-set comparison are tested separately on
native Windows; this target does not claim those APIs ran under Linux.

`activation_journal` feeds raw bytes to the production canonical journal decoder.
The decoder enforces the local-record size bound, canonical serialization, strict
schema and complete journal invariants. Fuzz success proves neither native storage
protection nor safe process-family recovery; those remain separate Windows gates.

Every crash must retain its minimized corpus input and become a deterministic
regression in `crates/keld-update/src/tests.rs` before its disposition is recorded.
