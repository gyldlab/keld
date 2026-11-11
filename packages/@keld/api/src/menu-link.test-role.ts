import { invokeMenuInstall } from "./app.ts";
const { Menu } = await import(
  new URL("../../electron/src/menu.ts", import.meta.url).href
);
if (process.argv[2] === "setter") {
  const menu = Menu.buildFromTemplate([
    {
      label: "X",
      role: "copy",
      click() {
        throw new Error("role invoked click");
      },
    },
  ]);
  console.log("returned=" + String(Menu.setApplicationMenu(menu)));
  menu.items[0]!.label = "mutated after capture";
  await new Promise<void>((resolve) => {
    process.stdin.once("data", () => resolve());
    process.stdin.resume();
  });
  console.log("setter-done");
  process.exit(0);
}
try {
  const receipt = await invokeMenuInstall({
    nodes: [
      {
        depth: 0,
        kind: "Normal",
        label: "X",
        enabled: true,
        checked: null,
        action: { variant: "NativeRole", role: "copy" },
      },
    ],
  });
  console.log("installed-generation=" + receipt.generation);
} catch (error) {
  if (
    process.argv[2] === "denied" &&
    !(
      error !== null &&
      typeof error === "object" &&
      "origin" in error &&
      error.origin === "peer"
    )
  ) {
    throw new Error("matching peer ERR lost its diagnostic provenance");
  }
  if (
    process.argv[2] === "malformed" &&
    error !== null &&
    typeof error === "object" &&
    "origin" in error
  ) {
    throw new Error(
      "local malformed receipt was incorrectly labelled a peer ERR",
    );
  }
  console.log(
    "refused=" +
      (error !== null && typeof error === "object" && "code" in error
        ? String(error.code)
        : "untyped"),
  );
}
process.exit(0);
