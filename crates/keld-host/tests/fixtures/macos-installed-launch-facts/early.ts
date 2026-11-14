/**
 * First entry dependency for the installed Host acceptance fixture.
 * This module imports no Keld code and reads facts before the facade evaluates.
 */
export const initialArgvObject = process.argv;
export const early = Object.freeze({
  pid: process.pid,
  ppid: process.ppid,
  type: Reflect.get(process, "type") ?? null,
  defaultApp: Reflect.get(process, "defaultApp") ?? null,
  resourcesPath: Reflect.get(process, "resourcesPath") ?? null,
  electronVersion: process.versions.electron ?? null,
  execPath: process.execPath,
  argv: [...process.argv],
  defaultDescriptor:
    Object.getOwnPropertyDescriptor(process, "defaultApp") ?? null,
  resourcesDescriptor:
    Object.getOwnPropertyDescriptor(process, "resourcesPath") ?? null,
});
console.log("GH1404_NATIVE_FACTS_FIRST_DEPENDENCY " + JSON.stringify(early));
