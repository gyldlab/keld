/**
 * App main process — supervised Bun child (KEL-29 hello template).
 * Full typed bridge codegen lands in @keld/api; this slice uses the CLI client.
 */

const link = process.env.KELD_APP_LINK;
if (!link) {
  console.error(
    "KELD-CLI-010: KELD_APP_LINK is unset — run the app with `keld dev`, not `bun` directly.",
  );
  process.exit(1);
}

const keld = process.env.KELD_BIN ?? "keld";
const proc = Bun.spawn([keld, "ipc-client", "echo", "--link", link], {
  stdout: "inherit",
  stderr: "inherit",
  env: process.env,
});

const code = await proc.exited;
if (code !== 0) {
  process.exit(code);
}

console.log("{{name}}: main process ready (IPC echo ok)");
