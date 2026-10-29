/**
 * Compile-only GH531 §4.f contract, included by the real package typecheck.
 * Complements existing window.close-event.prevent-default and
 * window.destroy.closed-without-close corpus cells without adding oracle cells.
 * Removing strict signatures leaves unused @ts-expect-error directives.
 * Never execute this function without an authenticated Host window.
 */
import type {
  BrowserWindow,
  BrowserWindowEventMap,
  CloseEvent,
  WebContents,
  WebContentsEventMap,
} from "./index";

export function checkWindowListenerTypes(window: BrowserWindow): void {
  const close = (event: CloseEvent): void => {
    event.preventDefault();
    const prevented: boolean = event.defaultPrevented;
    void prevented;
  };
  const closeArgs: BrowserWindowEventMap["close"] = [
    {
      preventDefault() {},
      defaultPrevented: false,
    },
  ];
  const destroyedArgs: WebContentsEventMap["destroyed"] = [];
  void closeArgs;
  void destroyedArgs;
  window.on("close", close).once("close", close).off("close", close);
  window
    .on("closed", () => {})
    .once("focus", () => {})
    .off("blur", () => {});
  window.on("close", (event) => {
    // @ts-expect-error CloseEvent rejects callback member typos.
    event.preventDefualt();
    // @ts-expect-error Only the Host close verdict owns this field.
    event.defaultPrevented = true;
  });
  window.once("close", (event) => {
    // @ts-expect-error once contextually types the close callback.
    event.preventDefualt();
  });
  window.off("close", (event) => {
    // @ts-expect-error off contextually types the close callback.
    event.preventDefualt();
  });
  // @ts-expect-error Unknown BrowserWindow event.
  window.on("clsoe", () => {});
  // @ts-expect-error Unknown BrowserWindow event.
  window.once("clsoe", () => {});
  // @ts-expect-error Unknown BrowserWindow event.
  window.off("clsoe", () => {});
  // @ts-expect-error close emits exactly one CloseEvent.
  window.on("close", (_event, _extra: string) => {});
  // @ts-expect-error closed emits no callback arguments.
  window.once("closed", (_extra: string) => {});
  // @ts-expect-error off accepts the same callback tuple as on.
  window.off("close", (_event, _extra: string) => {});

  const contents: WebContents = window.webContents;
  contents
    .on("destroyed", () => {})
    .once("destroyed", () => {})
    .off("destroyed", () => {});
  // Probe the actual getter surface as well as its exported interface.
  // @ts-expect-error WebContents has no closed event.
  window.webContents.on("closed", () => {});
  // @ts-expect-error WebContents has no closed event.
  window.webContents.once("closed", () => {});
  // @ts-expect-error WebContents has no closed event.
  window.webContents.off("closed", () => {});
  // @ts-expect-error destroyed supplies no callback argument.
  window.webContents.on("destroyed", (_event: string) => {});
  // @ts-expect-error destroyed supplies no callback argument.
  window.webContents.once("destroyed", (_event: string) => {});
  // @ts-expect-error destroyed supplies no callback argument.
  window.webContents.off("destroyed", (_event: string) => {});
  // @ts-expect-error The exported interface rejects unknown names too.
  contents.on("closed", () => {});
  // @ts-expect-error The exported interface rejects extra arguments too.
  contents.once("destroyed", (_event: string) => {});
  // @ts-expect-error The exported interface rejects wrong callbacks on off.
  contents.off("destroyed", (_event: string) => {});
}
