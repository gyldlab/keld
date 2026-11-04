//! Owned E5 fixtures only. Product assertions remain in the parent scenario.
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) const PACKAGE: &str = "kel75-e5-worker4-fixture";
pub(super) const MARKER: &str = "CACHE_PACKAGE_EVALUATED";
const INTEGRITY: &str = "sha512-hVvvC+Jp9wusoyUBy7bFOR3AAR84QoPQo/qkITJQauGj6APMCs7eHyk8KJOSNhZdqNqi9YXCz/SNQS2QUdK/Sg==";
const SHASUM: &str = "f6990465136403560ade669d6697ed240e4b3246";
const TARBALL_HEX: &str = concat!(
    "1f8b08000000000002ffedd3414fc2301407f09efd14a6274ddce87483044fcd5c30d18307e04a1a7890b9ad5dba4d31c6ef6e07181238",
    "781122e1ffbbbcf6a5e93bb4ff524d33b5a04eb9aefe6b6534fb63c2e986e1aa3abb550477dbf5aa1f04dd5bc12e053b82a6aa9575e3d9",
    "79fae45a15c4fb3ca3bc17791479efc66664436f9e2eebc612bfe16f64abd4687728f0852f5ca75069bb4df58c96eecbb84efd51b6b714",
    "66d6e4c4bf2e189c844dee3b3f4f798819bfe55f447bf96f0bf27f0453a32b93939f9bc5158f65fc984c5e64fc2407c92419cbe7911c26",
    "0ffcfa1e7906000000000000000000000000000000f8a7be01d584a53800280000",
);

struct Registry {
    address: SocketAddr,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl Registry {
    fn local_only() -> Result<Self, String> {
        let (pairs, remainder) = TARBALL_HEX.as_bytes().as_chunks::<2>();
        if !remainder.is_empty() {
            return Err("fixture tarball has an odd hex width".into());
        }
        let tarball: Vec<u8> = pairs
            .iter()
            .map(|pair| {
                let text = std::str::from_utf8(pair).map_err(|e| e.to_string())?;
                u8::from_str_radix(text, 16).map_err(|e| e.to_string())
            })
            .collect::<Result<_, String>>()?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        if !address.ip().is_loopback() {
            return Err("registry is not loopback-only".into());
        }
        let metadata = serde_json::json!({
            "name": PACKAGE, "dist-tags": {"latest": "1.0.0"},
            "versions": {"1.0.0": {"name": PACKAGE, "version": "1.0.0", "main": "index.js", "type": "module",
                "dist": {"tarball": format!("http://{address}/tarball.tgz"), "integrity": INTEGRITY, "shasum": SHASUM}}}
        }).to_string().into_bytes();
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let observed = Arc::clone(&requests);
        let stopping = Arc::clone(&stop);
        let worker = std::thread::spawn(move || {
            for connection in listener.incoming() {
                let stream = connection.map_err(|e| e.to_string())?;
                if stopping.load(Ordering::SeqCst) {
                    break;
                }
                serve(stream, &metadata, &tarball, &observed)?;
            }
            Ok(())
        });
        Ok(Self {
            address,
            requests,
            stop,
            worker: Some(worker),
        })
    }

    fn finish(mut self) -> Result<(), String> {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(3));
        match self.worker.take() {
            Some(worker) => worker
                .join()
                .map_err(|_| "registry worker panicked".to_owned())?,
            None => Ok(()),
        }
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_secs(3));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn serve(
    mut stream: TcpStream,
    metadata: &[u8],
    tarball: &[u8],
    requests: &AtomicUsize,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    let mut buffer = [0_u8; 2048];
    let mut header = Vec::new();
    while header.len() < 8192 && !header.windows(4).any(|part| part == b"\r\n\r\n") {
        match stream.read(&mut buffer).map_err(|e| e.to_string())? {
            0 => return Err("incomplete local registry request header".into()),
            count => header.extend_from_slice(&buffer[..count]),
        }
    }
    if !header.windows(4).any(|part| part == b"\r\n\r\n") {
        return Err("incomplete local registry request header".into());
    }
    let header = String::from_utf8_lossy(&header);
    let mut tokens = header.split_whitespace();
    let method = tokens.next().unwrap_or("");
    let target = tokens.next().unwrap_or("");
    requests.fetch_add(1, Ordering::SeqCst);
    let (status, content_type, body): (&str, &str, &[u8]) = match target {
        "/kel75-e5-worker4-fixture" | "/kel75-e5-worker4-fixture/latest" => {
            ("200 OK", "application/json", metadata)
        }
        "/tarball.tgz" => ("200 OK", "application/octet-stream", tarball),
        _ => ("404 Not Found", "text/plain", b"not found"),
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .map_err(|e| e.to_string())?;
    if method != "HEAD" {
        stream.write_all(body).map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) struct Fixture {
    // Registry teardown precedes temporary-path cleanup even during unwind.
    registry: Registry,
    root: tempfile::TempDir,
    seed: PathBuf,
    pub(super) stage: PathBuf,
    pub(super) environment: Vec<(OsString, OsString)>,
}

impl Fixture {
    pub(super) fn prepare() -> Result<Self, String> {
        let root = tempfile::tempdir().map_err(|e| e.to_string())?;
        let seed = root.path().join("seed");
        let stage = root.path().join("stage");
        for path in [&seed, &stage] {
            fs::create_dir(path).map_err(|e| e.to_string())?;
        }
        require_clean_ancestors(&stage)?;
        if stage.starts_with(&seed) {
            return Err("seed is an ancestor of stage".into());
        }
        fs::write(
            seed.join("package.json"),
            format!(
                r#"{{"name":"e5-seed","private":true,"dependencies":{{"{PACKAGE}":"1.0.0"}}}}"#
            ),
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            stage.join("entry.ts"),
            format!("import \"{PACKAGE}\";\nconsole.log(\"ENTRY_RAN\");\n"),
        )
        .map_err(|e| e.to_string())?;
        for name in ["xdg", "tmp", "cache"] {
            fs::create_dir(root.path().join(name)).map_err(|e| e.to_string())?;
        }
        let registry = Registry::local_only()?;
        let mut environment = super::isolated_environment(root.path())?;
        let registry_url = format!("http://{}", registry.address);
        for (name, value) in [
            ("BUN_CONFIG_REGISTRY", registry_url.as_str()),
            ("NPM_CONFIG_REGISTRY", registry_url.as_str()),
            ("npm_config_registry", registry_url.as_str()),
            ("NO_PROXY", "127.0.0.1,localhost"),
            ("LANG", "C"),
            ("LC_ALL", "C"),
        ] {
            environment.push((name.into(), value.into()));
        }
        for (name, path) in [
            ("XDG_CONFIG_HOME", root.path().join("xdg")),
            ("TMPDIR", root.path().join("tmp")),
        ] {
            environment.push((name.into(), path.into_os_string()));
        }
        let fixture = Self {
            registry,
            root,
            seed,
            stage,
            environment,
        };
        fixture.seed_cache()?;
        Ok(fixture)
    }

    fn seed_cache(&self) -> Result<(), String> {
        let mut version = Command::new("bun");
        version
            .arg("--version")
            .env_clear()
            .envs(self.environment.iter().cloned());
        let version = self.run("version", version)?;
        if !version.status.success() || version.stdout != b"1.4.4\n" {
            return Err(format!("unqualified Bun version: {version:?}"));
        }
        let mut command = Command::new("bun");
        command
            .args(["install", "--ignore-scripts"])
            .current_dir(&self.seed)
            .env_clear()
            .envs(self.environment.iter().cloned());
        let installed = self.run("seed", command)?;
        if !installed.status.success() {
            return Err(format!("local cache seeding failed: {installed:?}"));
        }
        if installed
            .stdout
            .windows(MARKER.len())
            .any(|s| s == MARKER.as_bytes())
            || installed
                .stderr
                .windows(MARKER.len())
                .any(|s| s == MARKER.as_bytes())
        {
            return Err("package code executed while only seeding cache".into());
        }
        if self.request_count() == 0 {
            return Err("no local registry seed occurred".into());
        }
        let source = fs::read_to_string(
            self.seed
                .join("node_modules")
                .join(PACKAGE)
                .join("index.js"),
        )
        .map_err(|e| e.to_string())?;
        if !source.contains(MARKER) {
            return Err("real installed bytes lack fixture marker".into());
        }
        let mut cache = fs::read_dir(self.root.path().join("cache")).map_err(|e| e.to_string())?;
        if cache
            .next()
            .transpose()
            .map_err(|e| e.to_string())?
            .is_none()
        {
            return Err("real Bun cache stayed empty".into());
        }
        if self.stage.join("node_modules").exists() {
            return Err("seed install entered the stage".into());
        }
        Ok(())
    }

    pub(super) fn request_count(&self) -> usize {
        self.registry.requests.load(Ordering::SeqCst)
    }

    pub(super) fn run(&self, label: &str, mut command: Command) -> Result<Output, String> {
        let stdout = self.root.path().join(format!("{label}.stdout"));
        let stderr = self.root.path().join(format!("{label}.stderr"));
        let out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stdout)
            .map_err(|e| e.to_string())?;
        let err = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stderr)
            .map_err(|e| e.to_string())?;
        let mut child = command
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(18);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => std::thread::yield_now(),
                Ok(None) => {
                    break Err("fixture Bun exceeded 18s; this is not a denial pass".to_owned());
                }
                Err(error) => break Err(error.to_string()),
            }
        };
        let status = match status {
            Ok(status) => status,
            Err(cause) => {
                let kill_error = child.kill().err();
                let cleanup_deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    match child.try_wait() {
                        Ok(Some(_)) => {
                            return Err(format!(
                                "{cause}; fixture child reaped; kill error: {kill_error:?}"
                            ));
                        }
                        Ok(None) | Err(_) if Instant::now() < cleanup_deadline => {
                            std::thread::yield_now();
                        }
                        observed => {
                            return Err(format!(
                                "{cause}; fixture child {} reaping unconfirmed: {observed:?}; kill error: {kill_error:?}",
                                child.id()
                            ));
                        }
                    }
                }
            }
        };
        // Only an independently observed exited child reaches this wait.
        child.wait().map_err(|e| e.to_string())?;
        Ok(Output {
            status,
            stdout: fs::read(stdout).map_err(|e| e.to_string())?,
            stderr: fs::read(stderr).map_err(|e| e.to_string())?,
        })
    }

    pub(super) fn materialize_installed(&self) -> Result<PathBuf, String> {
        let installed = self.root.path().join("healthy");
        let target = installed.join("node_modules").join(PACKAGE);
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        fs::copy(self.stage.join("entry.ts"), installed.join("entry.ts"))
            .map_err(|e| e.to_string())?;
        for name in ["package.json", "index.js"] {
            fs::copy(
                self.seed.join("node_modules").join(PACKAGE).join(name),
                target.join(name),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(installed)
    }

    pub(super) fn finish(self) -> Result<(), String> {
        self.registry.finish()
    }
}

fn require_clean_ancestors(stage: &Path) -> Result<(), String> {
    for ancestor in stage.ancestors() {
        for name in [
            "node_modules",
            "package.json",
            "tsconfig.json",
            "jsconfig.json",
            "bunfig.toml",
            ".bunfig.toml",
            ".env",
        ] {
            let path = ancestor.join(name);
            match fs::symlink_metadata(&path) {
                Ok(_) => return Err(format!("Bun fixture ancestry contains {}", path.display())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("cannot inspect fixture ancestry: {error}")),
            }
        }
    }
    Ok(())
}

pub(super) fn without_install_flag(original: &Command) -> Result<Command, String> {
    let arguments: Vec<OsString> = original
        .get_args()
        .map(std::ffi::OsStr::to_os_string)
        .collect();
    let retained: Vec<_> = arguments
        .iter()
        .filter(|arg| *arg != "--no-install")
        .collect();
    if arguments.len() != retained.len() + 1 {
        return Err("counterweight must remove exactly one flag".into());
    }
    let mut altered = Command::new(original.get_program());
    altered
        .args(retained)
        .current_dir(original.get_current_dir().ok_or("factory has no cwd")?)
        .env_clear();
    for (name, value) in original.get_envs() {
        if let Some(value) = value {
            altered.env(name, value);
        } else {
            altered.env_remove(name);
        }
    }
    if altered.get_program() != original.get_program()
        || altered.get_current_dir() != original.get_current_dir()
        || altered.get_envs().collect::<Vec<_>>() != original.get_envs().collect::<Vec<_>>()
    {
        return Err("one-flag counterweight changed executable/cwd/environment".into());
    }
    Ok(altered)
}
