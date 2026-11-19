import "@keld/kipc";
import { LifecycleLink } from "@keld/api/internal/link";

let connects = 0;
LifecycleLink.connect = (async () => {
  connects++;
  throw new Error("unexpected GH565 Host connection");
}) as typeof LifecycleLink.connect;

const ActualError = Error;
const ActualTypeError = TypeError;
const define = Object.defineProperty;
let hooks = 0;
const poison = () => {
  hooks++;
  throw new ActualError("unexpected late GH565 hook");
};
const hooksToReplace = [
  [Reflect, "ownKeys"],
  [Object, "entries"],
  [Object, "keys"],
  [Object, "getOwnPropertyDescriptor"],
  [Object, "defineProperty"],
  [Object, "freeze"],
  [Array, "isArray"],
  [Array.prototype, Symbol.iterator],
  [Promise, "reject"],
  [String.prototype, "slice"],
  [globalThis, "Error"],
  [globalThis, "TypeError"],
] as const;
const prior = hooksToReplace.map(([owner, key]) =>
  Object.getOwnPropertyDescriptor(owner, key)!,
);
const inheritedCode = Object.getOwnPropertyDescriptor(Object.prototype, "code");
let result: unknown;
try {
  define(Object.prototype, "code", { set: poison, configurable: true });
  for (let index = 0; index < hooksToReplace.length; index++) {
    const binding = hooksToReplace[index]!;
    define(binding[0], binding[1], { ...prior[index], value: poison });
  }
  const { shell, ClipboardItem, clipboard, nativeImage } =
    await import("../src/unsupported-bindings.ts");
  const { isUnsupportedMemberError } = await import("../src/unsupported.ts");
  const observe = (error: unknown) => {
    if (!isUnsupportedMemberError(error))
      throw new ActualError("expected canonical refusal");
    return [error.code, error.kind, error.member, error.ticket];
  };
  let reads = 0;
  const item = new ClipboardItem({
    get "text/plain"() {
      reads++;
      return "x";
    },
  });
  let invalid = false;
  try {
    new ClipboardItem({});
  } catch (error) {
    invalid = error instanceof ActualTypeError;
  }
  let goodType = false;
  try {
    item.types;
  } catch (error) {
    goodType = error instanceof ActualError;
  }
  let image: unknown;
  try {
    nativeImage.createFromDataURL("data:,");
  } catch (error) {
    image = observe(error);
  }
  const shellRefusal = await shell
    .openExternal("https://example.invalid")
    .catch(observe);
  const clipboardRefusal = await clipboard.write([item]).catch(observe);
  result = {
    connects,
    hooks,
    reads,
    linkKinds: [
      typeof shell,
      typeof ClipboardItem,
      typeof clipboard,
      typeof nativeImage,
    ],
    shell: shellRefusal,
    clipboard: clipboardRefusal,
    nativeImage: image,
    invalid,
    goodType,
  };
} finally {
  for (let index = 0; index < hooksToReplace.length; index++) {
    const binding = hooksToReplace[index]!;
    define(binding[0], binding[1], prior[index]!);
  }
  if (inheritedCode) define(Object.prototype, "code", inheritedCode);
  else Reflect.deleteProperty(Object.prototype, "code");
}
console.log(JSON.stringify(result));
