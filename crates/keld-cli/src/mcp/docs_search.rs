//! `keld_docs_search` — embedded generated documentation corpus.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One heading-chunk from the compile-time docs corpus.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DocChunkSource {
    title: String,
    source_path: String,
    body: String,
}

/// Arguments for `keld_docs_search`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct DocsSearchArgs {
    /// Free-text query (keywords; case-insensitive).
    pub query: String,
    /// Max results to return (default 5, 1..=20).
    #[serde(default)]
    #[schemars(range(min = 1, max = 20))]
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

const CORPUS_TOPICS_HINT: &str = "corpus topics: quickstart, wire-formats, overview, ipc, security/capability manifest, \
     electron-compat, webview, runtime/tooling, agent-experience, error-codes";

/// Compile-time `llms-full.txt` corpus, chunked by source marker and `##`.
fn corpus() -> Vec<DocChunkSource> {
    const FULL_CORPUS: &str = include_str!("../../../../llms-full.txt");
    parse_corpus(FULL_CORPUS)
}

/// Split `llms-full.txt` (or a test fixture) on `## Source:` markers.
///
/// GitHub Windows checkouts may yield CRLF; normalize before the LF markers.
fn parse_corpus(raw: &str) -> Vec<DocChunkSource> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut chunks = Vec::new();
    for document in normalized.split("\n## Source: `").skip(1) {
        let Some((path, text)) = document.split_once("`\n\n") else {
            continue;
        };
        chunks.extend(chunk_markdown(path, text));
    }
    chunks
}

fn chunk_markdown(source_path: &str, text: &str) -> Vec<DocChunkSource> {
    let mut title = text
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
        .unwrap_or("untitled")
        .to_owned();

    let mut chunks = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            push_chunk(
                &mut chunks,
                std::mem::take(&mut title),
                source_path,
                &body_lines,
            );
            title.push_str(rest.trim());
            body_lines.clear();
        } else {
            body_lines.push(line);
        }
    }
    push_chunk(&mut chunks, title, source_path, &body_lines);
    chunks
}

fn push_chunk(
    chunks: &mut Vec<DocChunkSource>,
    title: String,
    source_path: &str,
    body_lines: &[&str],
) {
    let body = body_lines.join("\n").trim().to_owned();
    if body.is_empty() {
        return;
    }
    chunks.push(DocChunkSource {
        title,
        source_path: source_path.to_owned(),
        body,
    });
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

    // Lowercase once per chunk: the haystacks are needed twice (document
    // frequency, then scoring) and the corpus is re-parsed on every call.
    let haystacks: Vec<(String, String, String, DocChunkSource)> = corpus()
        .into_iter()
        .map(|chunk| {
            (
                chunk.title.to_ascii_lowercase(),
                chunk.body.to_ascii_lowercase(),
                chunk.source_path.to_ascii_lowercase(),
                chunk,
            )
        })
        .collect();

    // Rarity weight per term (integer IDF).
    //
    // Without this the score was raw occurrence count, so one common word
    // repeated often outranked a chunk matching several rare words: the query
    // "Windows transport diverges" returned the Windows scoreboard section
    // (which says "Windows" constantly and neither other term) above the IPC
    // doc that actually discusses a diverging transport. Frequency is also
    // capped below, so repetition can add emphasis but cannot dominate.
    let total_chunks = u32::try_from(haystacks.len()).unwrap_or(u32::MAX).max(1);
    let weights: Vec<u32> = terms
        .iter()
        .map(|term| {
            let df = u32::try_from(
                haystacks
                    .iter()
                    .filter(|(t, b, p, _)| t.contains(term) || b.contains(term) || p.contains(term))
                    .count(),
            )
            .unwrap_or(u32::MAX);
            // A term in every chunk carries no signal (weight 1); a term in one
            // chunk is maximally discriminating. Capped so a single rare word
            // cannot swamp an otherwise better match.
            (total_chunks / df.max(1)).clamp(1, 8)
        })
        .collect();

    let mut scored: Vec<(u32, usize, DocChunk)> = Vec::new();
    for (idx, (hay_title, hay_body, hay_path, chunk)) in haystacks.into_iter().enumerate() {
        let mut score = 0u32;
        for (term, weight) in terms.iter().zip(&weights) {
            if hay_title.contains(term) {
                score = score.saturating_add(10 * weight);
            }
            if hay_path.contains(term) {
                score = score.saturating_add(4 * weight);
            }
            if hay_body.contains(term) {
                score = score.saturating_add(2 * weight);
                // Cap the frequency bonus: matching three distinct query terms
                // must beat repeating one of them thirty times.
                let hits =
                    u32::try_from(hay_body.matches(term.as_str()).count()).unwrap_or(u32::MAX);
                score = score.saturating_add(hits.min(3) * weight);
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
                source_path: chunk.source_path,
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
    fn generated_corpus_includes_compat_scoreboard_placeholder() {
        let result = search_docs(&DocsSearchArgs {
            query: "compat-scoreboard".to_owned(),
            max_results: Some(5),
        });
        assert!(
            result
                .results
                .iter()
                .any(|r| r.source_path.contains("compat-scoreboard")),
            "expected generated scoreboard source, got {:?}",
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
    fn wire_formats_doc_is_in_generated_corpus() {
        let result = search_docs(&DocsSearchArgs {
            query: "Windows transport diverges".to_owned(),
            max_results: Some(5),
        });
        assert!(
            result.results.iter().any(|r| {
                r.source_path.contains("04-wire-formats-and-contracts")
                    && (r.snippet.contains("diverges")
                        || r.snippet.contains("named pipe")
                        || r.snippet.contains("Loopback TCP"))
            }),
            "KEL-61: onboarding/04 must be searchable in llms-full.txt, got {:?}",
            result.results
        );
        let no_guard = search_docs(&DocsSearchArgs {
            query: "no capability check".to_owned(),
            max_results: Some(5),
        });
        assert!(
            no_guard
                .results
                .iter()
                .any(|r| r.source_path.contains("04-wire-formats-and-contracts")),
            "KEL-61: echo-path honesty ledger must be retrievable, got {:?}",
            no_guard.results
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

    #[test]
    fn empty_heading_chunks_are_skipped() {
        let chunks = chunk_markdown(
            "docs/x.md",
            "# Doc\n\nintro body\n\n## Empty\n\n## Full\n\nbody here\n",
        );
        assert!(
            chunks.iter().all(|c| !c.body.is_empty()),
            "empty bodies must not become hits: {chunks:?}"
        );
        assert!(
            chunks
                .iter()
                .any(|c| c.title == "Full" && c.body.contains("body here")),
            "{chunks:?}"
        );
        assert!(
            !chunks.iter().any(|c| c.title == "Empty"),
            "empty heading must not occupy a result slot: {chunks:?}"
        );
        assert!(
            chunks.iter().any(|c| c.body.contains("intro body")),
            "preamble must still be searchable: {chunks:?}"
        );
    }

    #[test]
    fn corpus_parses_crlf_source_markers() {
        // GitHub windows-latest checkouts yield CRLF; LF-only split markers
        // must not be the only path that produces chunks (KEL-54 / PR #2).
        let lf = concat!(
            "# Keld full documentation corpus\n\n",
            "## Source: `docs/architecture/03-security.md`\n\n",
            "# Security\n\n",
            "The capability manifest is default-deny.\n",
            "\n## Source: `docs/architecture/07-agent-experience.md`\n\n",
            "# Agent experience\n\n",
            "## Official Keld MCP server\n\n",
            "Search hits include KELD-IPC-004.\n",
        );
        let crlf = lf.replace('\n', "\r\n");
        assert!(
            crlf.contains("\r\n## Source: `"),
            "fixture must contain CRLF source markers"
        );

        let chunks = parse_corpus(&crlf);
        assert!(
            chunks.iter().any(|c| {
                c.source_path.contains("03-security") && c.body.contains("capability manifest")
            }),
            "CRLF corpus must yield 03-security.md, got {chunks:?}"
        );
        assert!(
            chunks.iter().any(|c| {
                c.source_path.contains("07-agent-experience")
                    && c.title.contains("Official Keld MCP server")
                    && c.body.contains("KELD-IPC-004")
            }),
            "CRLF corpus must yield 07-agent-experience.md heading, got {chunks:?}"
        );
    }
}
