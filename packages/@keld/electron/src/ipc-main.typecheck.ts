/**
 * Compile-only GH-466 registration contract, included by the package typecheck.
 * The public ipcMain surface is `any`-free: listener arguments are `unknown`,
 * channel names are strings, `IpcMainEvent` and `IpcMainInvokeEvent` are types
 * without a value binding, and the members other tickets own (`returnValue`
 * GH-548, `ports` GH-468) are absent from the type until they land. Removing a
 * strict signature leaves an unused @ts-expect-error directive.
 * Never execute this function: it registers listeners on the shared ipcMain.
 */
import {
  IpcMainEvent,
  type IpcMain,
  type IpcMainInvokeEvent,
  type IpcMainListener,
  type WebFrameMain,
  ipcMain,
} from "./index";

export function checkIpcMainRegistrationTypes(main: IpcMain): void {
  const listener: IpcMainListener = (event, ...args) => {
    const frame: WebFrameMain | null = event.senderFrame;
    const url: string | undefined = frame?.url;
    const origin: string | undefined = frame?.origin;
    const ids: [number, number] = [event.processId, event.frameId];
    const kind: "frame" = event.type;
    void url;
    void origin;
    void ids;
    void kind;
    void args;
    event.reply("mainResp", { ok: true });
    // @ts-expect-error returnValue is owned by GH-548 and is not on the type yet.
    event.returnValue = 1;
    // @ts-expect-error ports are owned by GH-468 and are not on the type yet.
    void event.ports;
    // @ts-expect-error host-minted frame facts are read-only.
    event.senderFrame = null;
  };
  main
    .on("rendererReq", listener)
    .once("export", listener)
    .off("export", listener)
    .addListener("app-load-finished", listener)
    .removeListener("app-load-finished", listener)
    .removeAllListeners("rendererReq")
    .removeAllListeners();
  main.handle(
    "invoke",
    (event: IpcMainInvokeEvent, ...args: unknown[]) =>
      args.length + event.frameId,
  );
  main.handleOnce("invoke-once", async () => undefined);
  main.removeHandler("invoke");
  const same: IpcMain = ipcMain;
  void same;
  // @ts-expect-error channel names are strings.
  main.on(1, listener);
  // @ts-expect-error a listener is a function.
  main.on("x", 42);
  // @ts-expect-error message arguments are unknown, never a narrower type.
  main.on("x", (_event, _arg: string) => {});
  // @ts-expect-error an invoke handler is a function.
  main.handle("x", "nope");
  // @ts-expect-error removeHandler takes a channel string.
  main.removeHandler();
  // @ts-expect-error IpcMainEvent is a type only; no value binding exists to construct.
  new IpcMainEvent();
}
