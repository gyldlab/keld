# KEL-53 updater fuzzers

Run any bounded raw-byte target from this directory:

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
Its retained corpus includes one canonical record for each transaction phase so
mutations begin from every valid phase-specific shape.

`host_identity` feeds raw bytes to `keld_pack::read_host_identity_bytes`, the
KEL-19 expected-identity container reader that A3 boot uses on the verified host
handle. Run it with a fixed allocation ceiling, so any single allocation of 1 MiB or
more is reported as a crash:

```sh
cargo +nightly fuzz run host_identity -- -max_total_time=60 -max_len=8192 -malloc_limit_mb=1
```

The reader keeps every buffer on the stack at a size fixed by the format, so its only
heap allocation is the decoded payload (at most 400 bytes). Its retained corpus holds
three writer outputs for the keld-pack synthetic fixtures: the two-section host, the
host with a trailing uninitialized section, and the two-section host with a 16-byte
certificate table at its raw end. On Windows MSVC the ASan runtime DLL
(`clang_rt.asan_dynamic-x86_64.dll` from the Visual Studio C++ tools) must be on
`PATH`.

Every crash must retain its minimized corpus input and become a deterministic
regression in `crates/keld-update/src/tests.rs` (or, for `host_identity`, in
`crates/keld-pack/src/host_identity/tests/`) before its disposition is recorded.
