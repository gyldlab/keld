/** Synchronous Bun-owned Menu objects and one guarded native installation call. */
import { invokeMenuInstall } from "@keld/api/internal/app";
import { stateWeakMap } from "@keld/kipc";
import {
  MENU_ROLES,
  MENU_FAULTS,
  encodeMenuRequest,
  type MenuRoleName,
  type MenuNodeV1,
  type MenuRequestV1,
} from "@keld/api/internal/menu-codec";
import { readSessionFact } from "./app-session-facts.ts";
import { refuse } from "./unsupported.ts";
import type { BrowserWindow } from "./browser-window.ts";

export type MenuItemType = "normal" | "separator" | "submenu" | "checkbox";
export type MenuItemClick = (
  item: MenuItem,
  window: BrowserWindow | undefined,
  event: unknown,
) => void;
/** The deliberately bounded macOS first-proof construction grammar. */
export interface MenuItemConstructorOptions {
  readonly type?: MenuItemType;
  readonly label?: string;
  readonly role?: string;
  readonly enabled?: boolean;
  readonly checked?: boolean;
  readonly click?: MenuItemClick;
  readonly submenu?: readonly (MenuItemConstructorOptions | MenuItem)[] | Menu;
}
interface ItemState {
  type: MenuItemType;
  label: string;
  role: MenuRoleName | undefined;
  enabled: boolean;
  checked: boolean;
  click: MenuItemClick | undefined;
  submenu: Menu | undefined;
  accelerator: string | null;
}
const menuItems = stateWeakMap<Menu, readonly MenuItem[]>();
const itemState = stateWeakMap<MenuItem, ItemState>();
const FIELDS = new Set([
  "type",
  "label",
  "role",
  "enabled",
  "checked",
  "click",
  "submenu",
]);

/** Registry-owned invalid bounded menu grammar, before any native operation. */
export class MenuTemplateError extends TypeError {
  readonly code = MENU_FAULTS.InvalidProjection.code;
  constructor(detail: string) {
    const fault = MENU_FAULTS.InvalidProjection;
    super(`${fault.code}: ${fault.message}: ${detail}. Fix: ${fault.fix}.`);
    this.name = "MenuTemplateError";
  }
}
function invalid(detail: string): never {
  throw new MenuTemplateError(detail);
}
function state(item: MenuItem): ItemState {
  return itemState.get(item) ?? invalid("MenuItem is not owned by this facade");
}
function children(menu: Menu): readonly MenuItem[] {
  return menuItems.get(menu) ?? invalid("Menu is not owned by this facade");
}
function labelValue(value: unknown): string {
  if (
    typeof value !== "string" ||
    !value.isWellFormed() ||
    value.includes("\0") ||
    Buffer.byteLength(value, "utf8") > 4096
  )
    return invalid("menu label is invalid");
  return value;
}
function booleanValue(value: unknown, field: string): boolean {
  return typeof value === "boolean"
    ? value
    : invalid(`menu ${field} must be boolean`);
}
interface BuildState {
  seen: Set<object>;
  nodes: number;
}
function existing(item: MenuItem, build: BuildState, depth: number): MenuItem {
  if (++build.nodes > 256 || depth > 8)
    return invalid("menu exceeds node/depth bounds");
  if (build.seen.has(item))
    return invalid("MenuItem sharing or cycles are unsupported");
  build.seen.add(item);
  const data = state(item);
  if (data.submenu) {
    if (build.seen.has(data.submenu))
      return invalid("Menu sharing or cycles are unsupported");
    build.seen.add(data.submenu);
    for (const child of children(data.submenu))
      existing(child, build, depth + 1);
  }
  return item;
}
function buildMenu(input: unknown, build: BuildState, depth: number): Menu {
  if (depth > 8) return invalid("menu exceeds depth bound");
  const output = Object.create(Menu.prototype) as Menu;
  if (
    input !== null &&
    typeof input === "object" &&
    menuItems.has(input as Menu)
  ) {
    if (build.seen.has(input))
      return invalid("Menu sharing or cycles are unsupported");
    build.seen.add(input);
    for (const item of children(input as Menu)) existing(item, build, depth);
    return input as Menu;
  }
  let descriptors: PropertyDescriptorMap;
  try {
    if (
      !Array.isArray(input) ||
      Object.getPrototypeOf(input) !== Array.prototype
    )
      return invalid("menu template must be a plain array");
    if (build.seen.has(input))
      return invalid("menu array sharing or cycles are unsupported");
    build.seen.add(input);
    descriptors = Object.getOwnPropertyDescriptors(
      input,
    ) as unknown as PropertyDescriptorMap;
  } catch (error) {
    if (error instanceof MenuTemplateError) throw error;
    return invalid("menu array cannot be inspected");
  }
  const length: unknown = descriptors.length?.value;
  if (
    typeof length !== "number" ||
    !Number.isSafeInteger(length) ||
    length < 0 ||
    length > 256
  )
    return invalid("menu exceeds node bound");
  for (const key of Reflect.ownKeys(descriptors)) {
    if (
      key !== "length" &&
      (typeof key !== "string" ||
        !/^(0|[1-9]\d*)$/.test(key) ||
        Number(key) >= length)
    )
      return invalid("menu array has an unsupported own key");
  }
  const result: MenuItem[] = [];
  for (let index = 0; index < length; index++) {
    const descriptor = descriptors[String(index)];
    if (!descriptor || !("value" in descriptor) || !descriptor.enumerable)
      return invalid("menu array has a hole or accessor");
    const value: unknown = descriptor.value;
    if (
      value !== null &&
      typeof value === "object" &&
      itemState.has(value as MenuItem)
    ) {
      result.push(existing(value as MenuItem, build, depth));
      continue;
    }
    if (++build.nodes > 256) return invalid("menu exceeds node bound");
    if (value === null || typeof value !== "object")
      return invalid("menu item must be a plain options object");
    let fields: PropertyDescriptorMap;
    try {
      const prototype: unknown = Object.getPrototypeOf(value);
      if (
        Array.isArray(value) ||
        (prototype !== Object.prototype && prototype !== null)
      )
        return invalid("menu item must be a plain options object");
      if (build.seen.has(value))
        return invalid("menu item sharing or cycles are unsupported");
      build.seen.add(value);
      fields = Object.getOwnPropertyDescriptors(value);
    } catch (error) {
      if (error instanceof MenuTemplateError) throw error;
      return invalid("menu item cannot be inspected");
    }
    for (const key of Reflect.ownKeys(fields)) {
      if (typeof key !== "string" || !FIELDS.has(key))
        return invalid("menu item has an unsupported own key");
      if (!("value" in fields[key]!))
        return invalid("menu item accessors are unsupported");
    }
    const field = (name: string): unknown =>
      Object.hasOwn(fields, name) ? fields[name]?.value : undefined;
    const hasSubmenu = field("submenu") !== undefined;
    const type =
      field("type") === undefined
        ? hasSubmenu
          ? "submenu"
          : "normal"
        : field("type");
    if (
      type !== "normal" &&
      type !== "separator" &&
      type !== "submenu" &&
      type !== "checkbox"
    )
      return invalid("menu item type is unsupported");
    if ((type === "submenu") !== hasSubmenu)
      return invalid("menu submenu/type combination is invalid");
    let role: MenuRoleName | undefined;
    if (field("role") !== undefined) {
      const raw = field("role");
      if (
        typeof raw !== "string" ||
        !/^[A-Za-z]+$/.test(raw) ||
        !Object.hasOwn(MENU_ROLES, raw.toLowerCase())
      )
        return invalid("menu role is unsupported");
      role = raw.toLowerCase() as MenuRoleName;
      if (type !== "normal")
        return invalid("native roles require a normal item");
    }
    const click = field("click");
    if (click !== undefined && typeof click !== "function")
      return invalid("menu click must be a function");
    if ((type === "separator" || type === "submenu") && click !== undefined)
      return invalid("structural menu items cannot have a click action");
    if (field("checked") !== undefined && type !== "checkbox")
      return invalid("checked is only supported for checkbox items");
    const metadata = role === undefined ? undefined : MENU_ROLES[role];
    let label = field("label");
    if (label === undefined) {
      label = metadata?.label ?? "";
      if ((label as string).includes("{app}"))
        label = (label as string).replace(
          "{app}",
          readSessionFact("name", "MenuItem role default label"),
        );
    }
    const item = Object.create(MenuItem.prototype) as MenuItem;
    itemState.set(item, {
      type,
      role,
      label: labelValue(label),
      enabled:
        field("enabled") === undefined
          ? true
          : booleanValue(field("enabled"), "enabled"),
      checked:
        field("checked") === undefined
          ? false
          : booleanValue(field("checked"), "checked"),
      click: click as MenuItemClick | undefined,
      submenu: hasSubmenu
        ? buildMenu(field("submenu"), build, depth + 1)
        : undefined,
      accelerator: metadata?.accelerator ?? null,
    });
    result.push(item);
  }
  menuItems.set(output, Object.freeze(result));
  return output;
}

/** Real branded local MenuItem state; native commands never invoke retained callbacks. */
export class MenuItem {
  constructor(options: MenuItemConstructorOptions) {
    return children(buildMenu([options], { seen: new Set(), nodes: 0 }, 0))[0]!;
  }
  get type(): MenuItemType {
    return state(this).type;
  }
  get role(): MenuRoleName | undefined {
    return state(this).role;
  }
  get label(): string {
    return state(this).label;
  }
  set label(value: string) {
    state(this).label = labelValue(value);
  }
  get enabled(): boolean {
    return state(this).enabled;
  }
  set enabled(value: boolean) {
    state(this).enabled = booleanValue(value, "enabled");
  }
  get checked(): boolean {
    return state(this).checked;
  }
  set checked(value: boolean) {
    const data = state(this);
    if (data.type !== "checkbox")
      invalid("checked is only supported for checkbox items");
    data.checked = booleanValue(value, "checked");
  }
  get submenu(): Menu | undefined {
    return state(this).submenu;
  }
  get accelerator(): string | null {
    return state(this).accelerator;
  }
}

/** Capture current private state; subsequent local mutations cannot alter queued bytes. */
function installation(menu: Menu): MenuRequestV1 {
  const nodes: MenuNodeV1[] = [];
  function visit(current: Menu, depth: number): void {
    for (const item of children(current)) {
      const data = state(item);
      nodes.push(
        Object.freeze({
          depth,
          kind:
            data.type === "normal"
              ? "Normal"
              : data.type === "separator"
                ? "Separator"
                : data.type === "submenu"
                  ? "Submenu"
                  : "Checkbox",
          label: data.label,
          enabled: data.enabled,
          checked: data.type === "checkbox" ? data.checked : null,
          action: Object.freeze(
            data.role !== undefined
              ? { variant: "NativeRole", role: data.role }
              : data.click !== undefined
                ? { variant: "UnsupportedClick" }
                : { variant: "Inert" },
          ),
        }),
      );
      if (data.submenu) visit(data.submenu, depth + 1);
    }
  }
  visit(menu, 0);
  const request = Object.freeze({ nodes: Object.freeze(nodes) });
  try {
    encodeMenuRequest(request);
  } catch (error) {
    if (
      error !== null &&
      typeof error === "object" &&
      Object.hasOwn(error, "code") &&
      (error as { code?: unknown }).code === "KELD-IPC-003"
    )
      invalid(
        "menu projection exceeds the approved wire grammar or 128 KiB bound",
      );
    throw error;
  }
  return request;
}

function observeFailure(error: unknown): void {
  let code = "KELD-IPC-005";
  if (
    error !== null &&
    typeof error === "object" &&
    Object.hasOwn(error, "code") &&
    typeof (error as { code?: unknown }).code === "string"
  )
    code = (error as { code: string }).code;
  // Every received peer ERR has exactly one Host-owned correlated diagnostic.
  // This says who reported the outcome, never that its Host effect is known.
  if (
    error !== null &&
    typeof error === "object" &&
    Object.hasOwn(error, "origin") &&
    (error as { origin?: unknown }).origin === "peer"
  )
    return;
  try {
    process.stderr.write(
      JSON.stringify({
        code,
        phase: "menu-install",
        result: code === "KELD-IPC-007" ? "not-sent" : "effect-unknown",
        fix: "Inspect Host menu admission and link diagnostics; make a fresh deliberate install only after correcting the cause.",
      }) + "\n",
    );
  } catch {
    /* User-replaced stderr cannot escape the handled transport continuation. */
  }
}

/** The native setter returns void; its one real guarded CALL has an observed receipt. */
export class Menu {
  constructor() {
    refuse("constructor", "Menu", "GH-556");
  }
  static buildFromTemplate(
    template: readonly (MenuItemConstructorOptions | MenuItem)[],
  ): Menu {
    return buildMenu(template, { seen: new Set(), nodes: 0 }, 0);
  }
  static setApplicationMenu(menu: Menu | null): void {
    if (menu === null)
      return refuse("method", "Menu.setApplicationMenu(null)", "GH-556");
    const request = installation(menu);
    void invokeMenuInstall(request).then(() => undefined, observeFailure);
  }
  static getApplicationMenu(): never {
    return refuse("method", "Menu.getApplicationMenu", "GH-556");
  }
  static sendActionToFirstResponder(..._action: unknown[]): never {
    return refuse("method", "Menu.sendActionToFirstResponder", "GH-476");
  }
  get items(): readonly MenuItem[] {
    return children(this);
  }
}
