# Documentation and Mermaid playbook

Load for agent-docs, generated docs, Mermaid, or public documentation changes.

## Generated docs

- `tools/llms_docs.rs` owns the allowlisted `llms.txt`/`llms-full.txt` corpus. Never
  hand-edit generated outputs; rebuild the working-tree generator, then run `just llms`.
- Included-source changes MUST pass `just llms-test` and `just llms-check`. A stale but
  self-consistent artifact is not proof that the generator is current.
- Numbered documentation paths are identifiers; renumbering MUST update every reference
  in the same change.

## Diagram selection and meaning

- Add Mermaid only when it materially clarifies relationships over prose/a small table.
  Use flowchart for topology/decisions, sequenceDiagram for ordered messages,
  stateDiagram-v2 for state machines, gantt only for real dates, erDiagram for data.
- Every block has `accTitle` and `accDescr`. Labels carry current/target and
  framework/showcase meaning without relying on color; surrounding prose names source of
  truth and any implementation gap.
- Use stable repository/GitHub syntax. For unfamiliar syntax, Context7 MAY locate current
  material, but official Mermaid docs remain the authority.

## Shared semantic palette

```text
classDef current fill:#dcfce7,stroke:#15803d,color:#052e16,stroke-width:2px
classDef target fill:#dbeafe,stroke:#1d4ed8,color:#172554,stroke-width:2px
classDef showcase fill:#f3e8ff,stroke:#7e22ce,color:#3b0764,stroke-width:2px,stroke-dasharray:5 3
classDef gate fill:#fef3c7,stroke:#b45309,color:#451a03,stroke-width:2px
classDef external fill:#e2e8f0,stroke:#475569,color:#0f172a,stroke-width:2px
classDef denied fill:#fee2e2,stroke:#b91c1c,color:#450a0a,stroke-width:2px
```

## Render evidence

- Changed diagrams run `just mermaid-test`, `just mermaid-check`, and
  `just mermaid-render-check` using the repository's digest-pinned isolated renderer.
- Inspect the rendered relationship once; parse success cannot find reversed edges,
  misleading grouping, or clipped semantic labels.
- PR/handoff reports source files/block count, renderer/version/digest, exact command,
  output format, and observed result. Generated pictures stay temporary unless the image
  itself is a reviewed artifact.
- Private-research diagrams additionally follow `.agents/research.md` provenance rules.
