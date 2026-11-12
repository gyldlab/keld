import type { MenuItemConstructorOptions } from "./menu.ts";

/** Source-shaped original macOS electron.js:1949-2069; original is not evaluated. */
export function drawioMenuTemplate(
  disableUpdate: boolean,
  dontCheckUpdates: boolean | undefined,
): readonly MenuItemConstructorOptions[] {
  const click = () => {
    throw new Error("construction invoked an application click");
  };
  const application: MenuItemConstructorOptions[] = [
    { label: "About draw.io", click },
    { label: "Support", click },
    { label: "Check for updates", click },
    {
      label: "Check for Updates Automatically",
      type: "checkbox",
      checked: dontCheckUpdates !== true,
      click,
    },
    { label: "Set Update Check Interval...", click },
    { type: "separator" },
    { label: "Actual Size", click },
    { label: "Zoom In", click },
    { label: "Zoom Out", click },
    { type: "separator" },
    { role: "hide" },
    { role: "hideothers" },
    { role: "unhide" },
    { type: "separator" },
    { role: "quit" },
  ];
  if (disableUpdate) application.splice(2, 3);
  return [
    { label: "draw.io", submenu: application },
    { label: "File", submenu: [{ role: "close" }] },
    {
      label: "Edit",
      submenu: [
        { role: "undo" },
        { role: "redo" },
        { type: "separator" },
        { role: "cut" },
        { role: "copy" },
        { role: "paste" },
        { role: "pasteAndMatchStyle" },
        { role: "selectAll" },
      ],
    },
  ];
}
