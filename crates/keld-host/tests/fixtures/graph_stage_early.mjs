// Runs before the first application import of electron. Canonical KIPC must
// already have captured its own native primitives through Guardian's preload.
const Native = DataView;
globalThis.__graphProbe = { armed: false, hits: 0 };
function UntrustedDataView(...args) {
  if (globalThis.__graphProbe.armed) globalThis.__graphProbe.hits += 1;
  return new Native(...args);
}
UntrustedDataView.prototype = Native.prototype;
Object.setPrototypeOf(UntrustedDataView, Native);
globalThis.DataView = UntrustedDataView;
