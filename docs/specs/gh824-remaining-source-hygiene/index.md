# Spec: Remaining source-hygiene ownership slices (GH824)

- Status: implementing
- Tracker: GH-824
- Owner: GYLDLAB (repository owner)
- Updated: 2026-11-17
- Pin: 6892ec6fd45271f538230998998a73906d0b3268

## 1. Goal & non-goals

Make two responsibilities discoverable without moving runtime lifecycle or security authority. The first owns bounded transcript storage in keld-runtime. The second owns local research-checkout operations now embedded in justfile. This is not a file-length policy. The work does not complete the whole GH-824 audit. PR #1178 formatting and navigation fixes are separate evidence.

Non-goals:

- No output-limit, supervisor, OS-handle, permission or wire-format change.
- No new dependency, public command name or reference-lock owner.
- No private research read or content migration.
- No CI-admission or numeric source-size policy change.
- No blanket tools/ move, transport split or other-owner branch edit.

## Parts

| Part | Owns | Load when |
| --- | --- | --- |
| [references.md](references.md) | Section 2. | Checking owners, source pins and existing contracts. |
| [acceptance.md](acceptance.md) | Section 3. | Reviewing binary acceptance and negative controls. |
| [design.md](design.md) | Section 4. | Reviewing capture, research helper, origin and router boundaries. |
| [delivery.md](delivery.md) | Sections 5–7, 9 and 10. | Planning tests, ownership, platform proof and blockers. |

## Current blockers

- B-1

## 8. Review gates triggered

- **Public API:** Review the unchanged root re-export and public type contract. No new public signature is proposed.
- **Unsafe and permissions:** None. The capture move adds no unsafe code or permission.
- **Wire and dependencies:** None. No wire or dependency change is proposed.
- **Owner merge gate:** GH-824 retains its whole-codebase audit hold. This spec does not release it.

T-1 and T-2 are recorded as completed source slices. T-3 remains open until exact-head review and required platform evidence are reconciled. The repository owner must release B-1 separately.
