---
name: "2026-10-31-process-linear-mcp-comment-pagination"
description: "Linear MCP returned a single page in this observation. Use a stable order and page cursors. Long descriptions can be truncated."
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

- In this observation, `list_comments` returned one page; results were newest-first. A missing ID on page one does not prove the comment is absent.
- Choose a stable order and keep it for the walk. Follow `pageInfo` with the returned cursor until the target appears or the response indicates there is no next page. Page size and default ordering can vary by connector version.
- In this observation, an oversized response moved to a file that contained one JSON object on one line. Inspect the actual format before parsing.
- On 2026-10-31, `get_issue` truncated one long description and appended "(truncated, use get_issue for full description)" even with `fields: ["description"]`. Quote the comment thread or a landed artifact instead, and label the citation as secondary.

Historical negative control (2026-10-31): the first response contained 40 comments and `hasNextPage: true`. The target ID appeared only after following the cursor. This is an observation, not a fixed page-size limit.
