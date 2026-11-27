# Spec: guarded macOS application menu and host-native roles (GH-480)

Status: draft
Tracker: GH-480
Owner: GYLDLAB
Updated: 2026-11-17
Pin: ec29b7ce2b155a8c98f96036cbb9c54488b5c302
## 1. Goal & non-goals

Let the **unmodified first-proof draw.io macOS application** build a bounded
Electron-compatible `Menu`/`MenuItem` tree **synchronously in Bun**, including
the real update-preference checkbox and its initially checked state, then
request installation of the projected native macOS application menu by a
**single guarded app-to-Host CALL**. Once installed, recognized host-native
Edit/App roles and their **built-in default keyboard equivalents** execute
on the current macOS first responder without a Host-to-Bun action frame.
A `role: 'close'`, whether activated by mouse or Cmd+W, enters the existing GH-450
**vetoable Host CloseRequested → facade close/preventDefault path**, never a
raw native destroy. A `role: 'quit'` remains a **nonterminating** diagnosed
compatibility gap until GH-563/GH-451 owns real cancelable Quit; native Cmd+Q
must not bypass dirty-document protection. Every native effect, callback and
command identity stays with the current authenticated app generation and
AppKit/UI owner. Missing exact `app.system` literal `menu` creates **zero**
native menu installation effects. The checkbox and non-role click actions
remain explicitly classified as unsupported **interaction** until GH-556 owns
activation delivery; construction and installation must still succeed.

Non-goals:
- Implementing the canonical `keld-guard` system-literal parser a second time,
  creating a second window registry, app link, transport Worker or UI event loop.
- Sending non-role click callbacks or activated MenuItem state back to Bun;
  menu-click EVENTs and callback mirror belong to GH-556 (F06-T9). In the
  first proof, unsupported clicks issue a Host-owned typed diagnostic;
  checkbox selection leaves its projected checked state unchanged, and
  must not masquerade as a successful preference toggle.
- Actually terminating on `role:'quit'` or Cmd+Q. GH-563 owns the separate
  F01-T3 GH-451 cancelable Host Quit path; until then it must be safely inert
  and report that remaining owner.
- Implementing Menu.popup, menu-will-show/menu-will-close, Tray, menu
  accelerators beyond documented built-in roles, getApplicationMenu, dynamic
  item mutation, default menus, MenuItem sharing or Windows/Linux native
  menu bars (the neighboring GH-556/GH-557 tracks own them).
- Implicit `legacy` activation, bare native Win32/GTK/ObjC permissions,
  unrestricted `node:fs`, broad wildcard menu grants, or test-score
  claims based on a successful TypeScript export alone.
- Changing Host/Guardian/kipc deadlines, GH-531 identity, GH-527 ring,
  or the existing native close/quit policy.
## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [refs.md](refs.md) | Section 2. | Reviewing pinned source and upstream contracts. |
| [contract.md](contract.md) | Section 3. | Reviewing acceptance and effect-free Guard behavior. |
| [design.md](design.md) | Section 4. | Reviewing trust, native roles, diagnostics and lifecycle. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Planning implementation, tests, blockers and evidence. |

## Current blockers

- B-1
- B-2
- B-3
- B-4
- B-5
## 8. Review gates triggered

- **Permission model:** yes — one system-literal `menu` grant, verified
  app RoleInstance, immutable policy digest and zero native effects on
  denial; reuse GH478's shared evaluator, never broader Wildcard.
- **Public API:** yes — synchronous Menu/MenuItem shapes, role priority,
  checkbox `checked`, lowercase role semantics, built-in defaults,
  bounded options, `void` installation and typed stderr observation.
- **Wire protocol:** yes — new append-only GH508 guarded menu channel,
  typed immutable tree/receipt with `Checkbox`/`UnsupportedClick`
  discriminants and boolean checked, hostile codec tests, no
  activation EVENTs.
- **Unsafe:** conditional and **blocking** — current
  `keld-native/AGENTS.md` production unsafe allowance covers specific
  Auth operations only; any new NSMenu/ObjC unsafe needs a documented
  narrow allowlist/source proof and independent review first.
- **Dependency:** none planned; pinned objc2 crate features may need
  explicit reviewed additions at implementation, not silently inferred.
- **Platform:** real macOS menu/checkbox, focused mouse + native keyboard
  responder, Cmd+W close-veto, Cmd+Q suppression, stderr diagnostics
  and physical cleanup; no Windows/Linux claims.