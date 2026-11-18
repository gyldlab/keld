/** An entry dependency reads process facts before any facade import. */
console.log("GH1404_DEPENDENCY_REACHED");
export const argvReference = process.argv;
export const early = {
  type: Reflect.get(process, "type") ?? null,
  defaultApp: Reflect.get(process, "defaultApp") ?? null,
  resourcesPath: Reflect.get(process, "resourcesPath") ?? null,
  execPath: process.execPath,
  argv: [...process.argv],
  defaultDescriptor:
    Object.getOwnPropertyDescriptor(process, "defaultApp") ?? null,
  resourcesDescriptor:
    Object.getOwnPropertyDescriptor(process, "resourcesPath") ?? null,
};
