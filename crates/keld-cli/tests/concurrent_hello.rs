//! KEL-30: host-owned concurrent hello app-link (Bun + echo live together).

#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use keld_cli::create::create_project;
use keld_cli::dev::start_dev_session;

#[test]
fn host_owned_session_keeps_bun_alive_and_second_echo_after_window_phase() {
    let dir = tempfile::tempdir().expect("tempdir");
    let name = format!("c{}", std::process::id());
    let root = create_project(dir.path(), &name).expect("create");
    let marker = root.join("echo-again");
    let marker_lit = marker.display().to_string();

    fs::write(
        root.join("src/main.ts"),
        format!(
            r#"
import {{ AppLinkSession }} from "./kipc";
import {{ watchFile, existsSync }} from "node:fs";

const link = process.env.KELD_APP_LINK;
if (!link) {{
  console.error("KELD-CLI-010: KELD_APP_LINK is unset");
  process.exit(1);
}}
const session = await AppLinkSession.connect(link);
try {{
  const response = await session.echo({{ message: "keld", count: 1 }});
  console.log(`ipc-echo ok: message=${{JSON.stringify(response.message)}} count=${{response.count}}`);
  console.log("concurrent-ready");
  const again = {marker_lit:?};
  await new Promise<void>((resolve, reject) => {{
    const tryEcho = async () => {{
      if (!existsSync(again)) return;
      try {{
        const second = await session.echo({{ message: "after-window", count: 2 }});
        console.log(
          `ipc-echo ok: message=${{JSON.stringify(second.message)}} count=${{second.count}}`,
        );
        resolve();
      }} catch (e) {{
        reject(e);
      }}
    }};
    watchFile(again, {{ interval: 20 }}, () => {{
      void tryEcho();
    }});
    void tryEcho();
  }});
  await new Promise(() => {{}});
}} finally {{
  session.close();
}}
"#
        ),
    )
    .expect("overwrite main");

    let session = start_dev_session(&root).expect("start host-owned session");
    session
        .wait_until_output_contains("concurrent-ready", Duration::from_secs(30))
        .expect("first HELLO+CALL ready");
    let pid = session
        .current_pid()
        .expect("Bun must still be live after the first echo");
    assert!(
        process_is_alive(pid),
        "supervised Bun pid {pid} must be alive before the window-phase marker"
    );

    // Window-phase stand-in: shipping `run_dev` calls `run_hello_window_html` here.
    fs::write(&marker, "1").expect("window-phase marker");
    session
        .wait_until_output_contains(
            "ipc-echo ok: message=\"after-window\" count=2",
            Duration::from_secs(30),
        )
        .expect("second CALL/REPLY after window ownership began");
    assert!(
        process_is_alive(pid),
        "Bun must stay alive across the window-phase wire proof"
    );
    assert!(
        session
            .output()
            .stdout
            .contains("ipc-echo ok: message=\"keld\" count=1"),
        "first echo missing: {}",
        session.output().stdout
    );

    let link = session.link().to_owned();
    session.shutdown().expect("shutdown");
    assert!(
        !process_is_alive(pid),
        "shutdown must reap Bun; pid {pid} still live"
    );
    #[cfg(unix)]
    {
        let endpoint = link.split('#').next().expect("endpoint");
        let socket = Path::new(endpoint);
        assert!(
            !socket.exists(),
            "echo socket must be removed: {}",
            socket.display()
        );
    }
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .creation_flags(0x0800_0000)
        .output();
    match output {
        Ok(out) => String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()),
        Err(_) => false,
    }
}
