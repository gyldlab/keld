//! Host-created D16 native helper. The test owns termination through a retained handle.
fn main() {
    loop {
        std::thread::park();
    }
}
