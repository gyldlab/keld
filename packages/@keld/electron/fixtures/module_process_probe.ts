/** GH-569 direct facade observation, not a supervised role or installed-app run. */
const facade = await import(process.argv[2] ?? "../src/index.ts");
const namespace = Object.keys(facade)
  .filter((key) => key !== "default")
  .sort();
const defaultKeys = Object.keys(facade.default).sort();
console.log(
  JSON.stringify({
    namespace,
    defaultKeys,
    electronVersion: process.versions.electron,
  }),
);
