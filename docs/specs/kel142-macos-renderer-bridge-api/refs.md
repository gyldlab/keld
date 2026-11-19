# Spec: KEL-142: spec refs

Part of: [index](index.md)
Owns: section 2, the authority and predecessors this spec consumes

## 2. Spec refs

This child consumes, rather than redefines:

- KEL-139 / PR #341, merged as
  `32093e1d789febda666b3fc04a5836999d429e8c`: approved minimum macOS
  product-spine contract.
- KEL-136: the canonical TypeScript app-link transport in
  `packages/@keld/kipc/src/transport.ts`.
- KEL-133: centralized receiver semantics, nonzero CALL correlation, absolute
  deadlines, and hostile transcript rules.
- KEL-75/T3 corrected lifecycle-routed virtual-port owner, including follow-up
  PR #59 merged as
  `756b6fb76030666f7b4ce56e423cdd9d1b32bff3`. Its already-published inline
  bound is `MAX_PORT_MESSAGE_LEN = 4096`.
- KEL-72: live `@keld/electron` lifecycle behavior that must be extracted,
  not copied.
- KEL-98: the bounded EchoRequest/EchoResponse declaration and codec fixture.
- KEL-80 comment "Product-spine task-level slice" as **scope/acceptance
  authority** for this first renderer slice.

KEL-80 is still Backlog and no separate passed `KEL-80/T*` execution artifact
exists. KEL-142 therefore owns the first implementation of this explicitly
authorized narrow slice. It must not claim that a nonexistent KEL-80 execution
artifact was consumed, and it must not mark broader KEL-80 acceptance complete.
