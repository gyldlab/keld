/** Negative: windowless-looking pre-Ready facade Quit must not kill recovery. */
if (process.env.KELD_NATIVE_WINDOW_FACADE !== "1") {
  throw new Error(
    "fixture requires native facade selection in the launch environment",
  );
}
delete process.env.KELD_APP_LINK;
const { app } = await import("../src/index.ts");

let code = "success";
try {
  await app.quit();
} catch (error) {
  code =
    error instanceof Error && error.message.startsWith("KELD-COMPAT-012:")
      ? "KELD-COMPAT-012"
      : "other";
}
if (code !== "KELD-COMPAT-012" || app.isReady()) {
  throw new Error(
    `facade Quit reached an unauthenticated Host or failed to deny: ${code}`,
  );
}
console.log("KELD_QUIT_BEFORE_NATIVE_READY_REFUSED");
