const earlyType = require("early");
const { app } = require("electron");
app.setPath("userData", process.env.HOME + "/userdata");
const facts = {};
for (const key of [
  "defaultApp",
  "mas",
  "windowsStore",
  "sandboxed",
  "contextIsolated",
  "resourcesPath",
]) {
  const value = process[key];
  facts[key] =
    value === undefined ? { kind: "undefined" } : { kind: typeof value, value };
}
console.log(
  JSON.stringify({
    electron: process.versions.electron,
    earlyType,
    userData: app.getPath("userData"),
    argv: process.argv,
    userArgs: process.argv.slice(process.defaultApp ? 2 : 1),
    facts,
  }),
);
app.quit();
