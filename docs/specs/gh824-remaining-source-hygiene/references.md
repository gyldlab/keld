# References and source baseline

Part of: [index](index.md)
Owns: section 2

## 2. Spec refs

**FACT:** [Runtime and tooling architecture](../../architecture/06-runtime-and-tooling.md) section 1 owns the supervisor contract. KEL-134 and KEL-225 own bounded capture and the literal host-retention ceiling.

**FACT:** [Runtime agent instructions](../../../crates/keld-runtime/AGENTS.md) retain process-family and unsafe ownership. [Test layout](../../../.agents/test-layout.md) owns test placement and discovery safeguards.

**FACT:** [Agent workflow](../../agents/workflow.md) and [coordination rules](../../../.agents/coordination.md) own claims, managed worktrees, reference-resource coordination and review. GH-824 retains the whole-codebase audit and merge hold.

**FACT:** The public root re-exports CapturedOutput and the five existing CAPTURE constants. Source: `crates/keld-runtime/src/lib.rs:266-267@6892ec6fd45271f538230998998a73906d0b3268`.

**FACT:** The private capture module owns the constants, retained-output structure and bounded append logic. Source: `crates/keld-runtime/src/captured_output.rs:9-56@6892ec6fd45271f538230998998a73906d0b3268` and `crates/keld-runtime/src/captured_output.rs:87-203@6892ec6fd45271f538230998998a73906d0b3268`.

**FACT:** The ten pure capture tests sit beside that module. Source: `crates/keld-runtime/src/captured_output/tests.rs:10-321@6892ec6fd45271f538230998998a73906d0b3268`.

**FACT:** The public research-sync and research-push recipes retain the reference-run wrapper. Their private recipes resolve the reference root before calling the helper. Source: `justfile:372-406@6892ec6fd45271f538230998998a73906d0b3268`.

**FACT:** The workspace tool owns reference-root and reference-run command dispatch. Source: `tools/workspace.py:691-729@6892ec6fd45271f538230998998a73906d0b3268`. The helper does not own the shared reference lock or root selection.

**FACT:** The helper contains one origin predicate and separate sync and push operations. Source: `tools/research_checkout.sh:20-150@6892ec6fd45271f538230998998a73906d0b3268`.

**FACT:** The research fixture uses local bare Git repositories and refuses accidental network fallback. Source: `tools/test_research_checkout.sh:20-401@6892ec6fd45271f538230998998a73906d0b3268`.

**FACT:** The CI input contract is stateless v2. It stores no reader-content digest.
It rejects retired digest keys. Source: `tools/ci_inputs.py:7-10@6892ec6fd45271f538230998998a73906d0b3268` and
`tools/ci_inputs.py:67-80@6892ec6fd45271f538230998998a73906d0b3268`.
Reader matching and consumer expansion use path patterns. Source:
`tools/ci_inputs.py:143-188@6892ec6fd45271f538230998998a73906d0b3268`.

**INFERRED:** A source move must preserve the existing public capture facade and every compiled test selector. A local helper must consume the existing reference owner rather than create a second lock or root resolver.

**UNKNOWN:** The final cross-platform receipt set for T-3 is not established by this specification. Each required operating system needs its own run. Do not infer an OS result from another OS.

No architecture contract is amended by these source-only slices.
