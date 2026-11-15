---
name: "2026-10-31-process-linear-mcp-comment-pagination"
description: "Linear MCP list_comments returns the newest page only; old comment ids need createdAt order plus cursor, and get_issue truncates long descriptions."
area: "process"
tags: ["linear", "mcp", "comments", "pagination", "cursor", "claims"]
platform: "all"
recorded: "2026-10-31"
status: "verified"
verified: "2026-10-31"
source: "https://github.com/gyldlab/keld/blob/231a212a7f8441d54f5fe24793d6fd020f54699e/docs/agents/workflow.md#L77"
related: []
---

## Fact

The workflow requires a re-fetch of issue comments before the first edit. The Linear MCP connector does not return a whole thread in one call.

- `list_comments` returns one page of at most 50 comments, newest first by `updatedAt`. On KEL-140, about 50 comments from 2026-10-11 pushed the 2026-10-10 product observation (`604aa046`) past page three. A missing id on page one is not evidence that the comment is absent.
- To reach an old comment id, pass `orderBy: createdAt` and the `cursor` from the previous page. Budget one extra call per 30 comments on a busy issue.
- A page that exceeds the tool output cap is written to a file. Parse it with a script; the line is one JSON object.
- `get_issue` truncates a long description and appends "(truncated, use get_issue for full description)" even when called with `fields: ["description"]` (KEL-55 on 2026-10-31). Quote the comment thread or a landed artifact instead, and label the citation as secondary.

Negative control: fetching KEL-140 with the default order on 2026-10-31 returned 40 comments and `hasNextPage: true`; the target id appeared only after the cursor walk. Evidence: prompt-tracker PR https://github.com/0monish/prompt-tracker/pull/58, `commands.log` and packet §13.
