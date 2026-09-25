#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    keld_update::fuzz_canonical_archive(data);
});
