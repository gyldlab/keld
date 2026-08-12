//! `keld_docs_search` — embedded architecture + error-registry corpus.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One heading-chunk from the compile-time docs corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocChunkSource {
    title: String,
    source_path: &'static str,
    body: String,
}

/// Arguments for `keld_docs_search`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocsSearchArgs {
    /// Free-text query (keywords; case-insensitive).
    pub query: String,
    /// Max results to return (default 5, capped at 20).
    #[serde(default)]
    pub max_results: Option<u8>,
}

/// One ranked search hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DocChunk {
    /// Section heading (or document title).
    pub title: String,
    /// Repo-relative path, e.g. `docs/architecture/03-security.md`.
    pub source_path: String,
    /// Snippet from the section body.
    pub snippet: String,
    /// Deterministic keyword score (higher is better).
    pub score: u32,
}

/// `keld_docs_search` structured output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct DocsSearchResult {
    /// Ranked chunks (deterministic order).
    pub results: Vec<DocChunk>,
    /// True when more matches existed than `max_results`.
    pub truncated: bool,
    /// Guidance when truncated or empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

const DEFAULT_MAX: u8 = 5;
const CAP_MAX: u8 = 20;
const SNIPPET_CHARS: usize = 280;

const CORPUS_TOPICS_HINT: &str = "corpus topics: overview, ipc, security/capability manifest, electron-compat, \
     webview, runtime/tooling, agent-experience, error-codes";

/// Compile-time corpus: architecture specs + the KELD-* registry, chunked by `##`.
fn corpus() -> Vec<DocChunkSource> {
    const FILES: &[(&str, &str)] = &[
        (
            "docs/architecture/01-overview.md",
            include_str!("../../../../docs/architecture/01-overview.md"),
        ),
        (
            "docs/architecture/02-ipc.md",
            include_str!("../../../../docs/architecture/02-ipc.md"),
        ),
        (
            "docs/architecture/03-security.md",
            include_str!("../../../../docs/architecture/03-security.md"),
        ),
        (
            "docs/architecture/04-electron-compat.md",
            include_str!("../../../../docs/architecture/04-electron-compat.md"),
        ),
        (
            "docs/architecture/05-webview-and-native.md",
            include_str!("../../../../docs/architecture/05-webview-and-native.md"),
        ),
        (
            "docs/architecture/06-runtime-and-tooling.md",
            include_str!("../../../../docs/architecture/06-runtime-and-tooling.md"),
        ),
        (
            "docs/architecture/07-agent-experience.md",
            include_str!("../../../../docs/architecture/07-agent-experience.md"),
        ),
        (
            "docs/engineering/keld-error-codes.md",
            include_str!("../../../../docs/engineering/keld-error-codes.md"),
        ),
    ];

    let mut chunks = Vec::new();
    for &(path, text) in FILES {
        chunks.extend(chunk_markdown(path, text));
    }
    chunks
}

fn chunk_markdown(source_path: &'static str, text: &str) -> Vec<DocChunkSource> {
    let mut title = text
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .unwrap_or("untitled")
        .to_owned();

    let mut chunks = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            chunks.push(DocChunkSource {
                title: title.clone(),
                source_path,
                body: body_lines.join("\n").trim().to_owned(),
            });
            title.clear();
            title.push_str(rest.trim());
            body_lines.clear();
        } else {
            body_lines.push(line);
        }
    }
    chunks.push(DocChunkSource {
        title,
        source_path,
        body: body_lines.join("\n").trim().to_owned(),
    });
    chunks
}

/// Searches the embedded corpus. Ranking is deterministic: score desc, then
/// `source_path`, then original heading index.
#[must_use]
pub fn search_docs(args: &DocsSearchArgs) -> DocsSearchResult {
    let max = usize::from(args.max_results.unwrap_or(DEFAULT_MAX).clamp(1, CAP_MAX));
    let terms: Vec<String> = args
        .query
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();

    if terms.is_empty() {
        return DocsSearchResult {
            results: Vec::new(),
            truncated: false,
            hint: Some(format!("empty query — try keywords; {CORPUS_TOPICS_HINT}")),
        };
    }

    let mut scored: Vec<(u32, usize, DocChunk)> = Vec::new();
    for (idx, chunk) in corpus().into_iter().enumerate() {
        let hay_title = chunk.title.to_ascii_lowercase();
        let hay_body = chunk.body.to_ascii_lowercase();
        let hay_path = chunk.source_path.to_ascii_lowercase();
        let mut score = 0u32;
        for term in &terms {
            if hay_title.contains(term) {
                score = score.saturating_add(10);
            }
            if hay_path.contains(term) {
                score = score.saturating_add(4);
            }
            if hay_body.contains(term) {
                score = score.saturating_add(2);
                let hits =
                    u32::try_from(hay_body.matches(term.as_str()).count()).unwrap_or(u32::MAX);
                score = score.saturating_add(hits);
            }
        }
        if score == 0 {
            continue;
        }
        scored.push((
            score,
            idx,
            DocChunk {
                title: chunk.title,
                source_path: chunk.source_path.to_owned(),
                snippet: snippet(&chunk.body),
                score,
            },
        ));
    }

    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.2.source_path.cmp(&b.2.source_path))
            .then_with(|| a.1.cmp(&b.1))
    });

    let total = scored.len();
    let truncated = total > max;
    let results: Vec<DocChunk> = scored.into_iter().take(max).map(|(_, _, c)| c).collect();

    let hint = if results.is_empty() {
        Some(format!(
            "no matches — try different keywords; {CORPUS_TOPICS_HINT}"
        ))
    } else if truncated {
        Some("raise `max_results` or narrow the query".to_owned())
    } else {
        None
    };

    DocsSearchResult {
        results,
        truncated,
        hint,
    }
}

fn snippet(body: &str) -> String {
    let flat: String = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= SNIPPET_CHARS {
        return flat;
    }
    let truncated: String = flat.chars().take(SNIPPET_CHARS).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_registry_code_is_searchable() {
        let result = search_docs(&DocsSearchArgs {
            query: "KELD-IPC-004".to_owned(),
            max_results: Some(5),
        });
        assert!(
            result.results.iter().any(|r| {
                r.source_path.contains("keld-error-codes") && r.title.contains("KELD-IPC-004")
            }),
            "expected registry heading KELD-IPC-004, got {:?}",
            result.results
        );
    }

    #[test]
    fn finds_real_architecture_heading() {
        let result = search_docs(&DocsSearchArgs {
            query: "Official Keld MCP server".to_owned(),
            max_results: Some(5),
        });
        assert!(
            result.results.iter().any(|r| {
                r.source_path.contains("07-agent-experience")
                    && r.title.contains("Official Keld MCP server")
            }),
            "expected heading from 07-agent-experience.md, got {:?}",
            result.results
        );
    }

    #[test]
    fn capability_manifest_hits_security_doc() {
        let result = search_docs(&DocsSearchArgs {
            query: "capability manifest".to_owned(),
            max_results: Some(5),
        });
        assert!(
            result
                .results
                .iter()
                .any(|r| r.source_path.contains("03-security")),
            "expected a hit from 03-security.md, got {:?}",
            result.results
        );
    }

    #[test]
    fn search_is_deterministic() {
        let args = DocsSearchArgs {
            query: "Official Keld MCP server".to_owned(),
            max_results: Some(5),
        };
        let a = search_docs(&args);
        let b = search_docs(&args);
        let c = search_docs(&args);
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn garbage_query_misses_with_hint() {
        let result = search_docs(&DocsSearchArgs {
            query: "zzzxnonexistentqqq".to_owned(),
            max_results: None,
        });
        assert!(
            result.results.is_empty(),
            "garbage query must not hallucinate chunks: {:?}",
            result.results
        );
        assert!(!result.truncated);
        let hint = result.hint.expect("hint");
        assert!(hint.contains("overview"), "{hint}");
        assert!(
            hint.contains("security") || hint.contains("capability"),
            "{hint}"
        );
    }

    #[test]
    fn truncation_hint_when_capped() {
        let uncapped = search_docs(&DocsSearchArgs {
            query: "keld".to_owned(),
            max_results: Some(20),
        });
        assert!(
            uncapped.results.len() > 1,
            "query `keld` must match more than one architecture chunk so max_results=1 can truncate; got {}",
            uncapped.results.len()
        );
        let result = search_docs(&DocsSearchArgs {
            query: "keld".to_owned(),
            max_results: Some(1),
        });
        assert!(
            result.truncated,
            "max_results=1 must truncate a broad query"
        );
        assert_eq!(
            result.hint.as_deref(),
            Some("raise `max_results` or narrow the query")
        );
        assert_eq!(result.results.len(), 1);
    }
}
