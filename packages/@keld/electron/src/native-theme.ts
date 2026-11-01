/** GH481 S1: actual native effective-appearance bool already applied to mirror. */
import {
  getDesktopSnapshot,
  isDesktopS1Selected,
} from "@keld/api/internal/app";
import {
  method,
  property,
  refuse,
  unsupportedSurface,
  writableProperty,
} from "./unsupported.ts";

const remaining = unsupportedSurface("nativeTheme", {
  themeSource: writableProperty("GH-558"),
  shouldUseHighContrastColors: property("GH-558"),
  shouldUseDarkColorsForSystemIntegratedUI: property("GH-558"),
  shouldUseInvertedColorScheme: property("GH-558"),
  inForcedColorsMode: property("GH-558"),
  prefersReducedTransparency: property("GH-558"),
  shouldDifferentiateWithoutColor: property("GH-558"),
  on: method("GH-558"),
  once: method("GH-558"),
  off: method("GH-558"),
  addListener: method("GH-558"),
  removeListener: method("GH-558"),
  removeAllListeners: method("GH-558"),
});

const surface = {
  get shouldUseDarkColors(): boolean {
    if (!isDesktopS1Selected())
      return refuse("property", "nativeTheme.shouldUseDarkColors", "GH-481");
    return getDesktopSnapshot().should_use_dark_colors;
  },
};
// Copy descriptors, not values: copying a GH558 getter would synchronously
// throw during import and prevent the original application's first window.
Object.defineProperties(surface, Object.getOwnPropertyDescriptors(remaining));
export const nativeTheme = Object.freeze(surface) as typeof surface &
  typeof remaining;
