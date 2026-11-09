---
name: "2026-08-16-process-wsl-version-prove-compositor-exists"
description: "WSL version does not prove a compositor exists; failed GTK initialization can identify an unavailable display."
area: "process"
tags: ["process", "wsl", "version", "prove", "compositor", "linux"]
platform: "linux"
recorded: "2026-08-16"
status: "historical"
verified: "unknown"
source: "https://github.com/gyldlab/keld/blob/244fb285125c2ee9e14c81a1c7b02cd357c7f5b1/docs/agents/learnings.md#L61"
related: ["docs/agents/learnings/wv/2026-08-16-wv-wry-linux-backend-gtk3-webkit2gtk.md"]
---

## Historical source

> - 2026-08-16 [process] WSL reporting `WSLg version: 1.0.66` via `wsl --version` does not mean a display is reachable: `/tmp/.X11-unix` and `/mnt/wslg/.X11-unix` can stay empty and `$DISPLAY`/`$WAYLAND_DISPLAY` unset even after `wsl --shutdown` + restart, as the default non-root user (per `/etc/wsl.conf` `[user] default=`). `gtk::init()` then fails with "Failed to initialize GTK" — a clean, unambiguous signal the sandbox has no compositor, not a code defect to chase. (evidence: KEL-28, `wsl --version`, tao-0.35.3 `platform_impl/linux/event_loop.rs` panic text)
