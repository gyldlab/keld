//! Each rule has one single-fault fixture mutant.
//!
//! The clean fixture passes first, so a mutant's RED comes from its fault alone. Every
//! mutant must trip exactly the rule it names: if a second rule also fired, deleting
//! the first rule could leave the fixture red and hide that deletion.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

const PIN: &str = "1111111111111111111111111111111111111111";
const FOREIGN_SHA: &str = "2222222222222222222222222222222222222222";
const ABSENT_SHA: &str = "3333333333333333333333333333333333333333";
const SEAL: &str = "4444444444444444444444444444444444444444";
const CITED_FILE: &str = "crates/x/src/lib.rs";

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Git facts for fixtures: one pin with a ten-line file, one foreign head, one seal.
struct Table {
    sealed: BTreeSet<String>,
}

impl History for Table {
    fn paths_at(&self, commit: &str, _dir: &str) -> Result<BTreeSet<String>, String> {
        assert_eq!(commit, SEAL);
        Ok(self.sealed.clone())
    }
    fn has_commit(&self, sha: &str) -> bool {
        sha == PIN || sha == FOREIGN_SHA
    }
    fn lines_at(&self, commit: &str, path: &str) -> Option<usize> {
        (path == CITED_FILE && (commit == PIN || commit == FOREIGN_SHA)).then_some(10)
    }
}

struct Fixture {
    root: PathBuf,
    rows: usize,
    sealed: BTreeSet<String>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn header() -> String {
    format!(
        "- Status: draft\n- Tracker: GH-1\n- Owner: owner\n- Updated: 2026-10-31\n- Pin: {PIN}\n"
    )
}

fn single_spec() -> String {
    let mut text = format!("# Spec: single\n\n{}\n", header());
    for (index, title) in SECTIONS.iter().enumerate() {
        text.push_str(&format!("## {}. {title}\n\n", index + 1));
        match index + 1 {
            3 => text.push_str(&ac_block(1)),
            6 => text.push_str("### T-1: slice\n\n- **Blocked by:** none\n\n"),
            _ => text.push_str("none\n\n"),
        }
    }
    text
}

fn ac_block(number: usize) -> String {
    format!(
        "### AC-{number}: rule\n\n- **Given:** a state\n- **When:** an action\n- **Then:** a \
         result\n- **Negative control:**\n  - **Single fault:** the rule is removed\n  - \
         **At-risk precondition:** the state holds\n  - **Expected RED:** the test fails\n\n"
    )
}

fn index_spec() -> String {
    format!(
        "# Spec: directory\n\n{}\n## 1. Goal & non-goals\n\nA goal.\n\n## Parts\n\n{PART_TABLE_HEADER}\n\
         |---|---|---|\n| [acceptance.md](acceptance.md) | AC-1 and AC-2 | testing |\n\
         | [design.md](design.md) | D-1 and E-1 | designing |\n\
         | [delivery.md](delivery.md) | tasks | implementing |\n\n\
         ## Current blockers\n\n- B-1\n\n## 8. Review gates triggered\n\nnone\n",
        header()
    )
}

fn part_header(title: &str, owns: &str) -> String {
    format!("# Spec: directory: {title}\n\nPart of: [index](index.md)\nOwns: {owns}\n\n")
}

fn acceptance_part() -> String {
    format!(
        "{}## 2. Spec refs\n\nnone\n\n## 3. Acceptance criteria\n\n{}{}",
        part_header("acceptance", "acceptance criteria"),
        ac_block(1),
        ac_block(2)
    )
}

fn design_part() -> String {
    format!(
        "{}## 4. Design\n\n### D-1: choice\n\nAC-1 needs `{CITED_FILE}:10` and \
         `{CITED_FILE}:1-2@{FOREIGN_SHA}` and `{CITED_FILE}:3@{ABSENT_SHA}`. See \
         [goal](index.md#1-goal--non-goals), [#1311](https://example.com) AC-9 and #7 B-8.\n\n\
         ## 5. Boundaries\n\nnone\n",
        part_header("design", "decisions")
    )
}

fn delivery_part() -> String {
    format!(
        "{}## 6. Tasks\n\n### T-1: slice\n\n- **Blocked by:** B-1\n\n\
         ## 7. Test plan & evidence\n\n### E-1: run\n\n- **Run:** one\n- **n:** 3\n\
         - **Environment:** host\n- **Limits:** none\n\n## 9. Perf impact\n\nnone\n\n\
         ## 10. Open questions, blockers & unknowns\n\n### B-1: stop\n\n- **Status:** open\n\
         - **Owner:** owner\n- **Impact:** blocks T-1\n- **Check:** a test\n\n\
         ### Q-1: which\n\nA question.\n\n### U-1: what\n\nAn unknown.\n",
        part_header("delivery", "tasks and evidence")
    )
}

fn legacy_big() -> String {
    "line\n".repeat(600)
}

const BIG: &str = "docs/specs/old-big.md";
const SMALL: &str = "docs/specs/old-small.md";
const DIR: &str = "docs/specs/gh1-directory";
const SINGLE: &str = "docs/specs/gh2-single.md";
const BASELINE: &str = "docs/specs/.spec-baseline.tsv";

impl Fixture {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!("keld-spec-structure-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let fixture = Fixture {
            root,
            rows: 2,
            sealed: [BIG, SMALL].iter().map(|path| (*path).to_owned()).collect(),
        };
        fixture.write(SINGLE, &single_spec());
        fixture.write(&format!("{DIR}/index.md"), &index_spec());
        fixture.write(&format!("{DIR}/acceptance.md"), &acceptance_part());
        fixture.write(&format!("{DIR}/design.md"), &design_part());
        fixture.write(&format!("{DIR}/delivery.md"), &delivery_part());
        fixture.write(BIG, &legacy_big());
        fixture.write(SMALL, "# old\n");
        fixture.write(
            BASELINE,
            &format!("{BASELINE_HEADER}\n{BIG}\t600\t-\t-\texempt\n{SMALL}\t-\t-\t-\texempt\n"),
        );
        fixture.write(
            TEMPLATE,
            &format!("# Template\n\n```markdown\n{}```\n", single_spec()),
        );
        fixture
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture dir");
        fs::write(path, text).expect("fixture write");
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(rel)).expect("fixture read")
    }

    fn replace(&self, rel: &str, from: &str, to: &str) {
        let text = self.read(rel);
        assert!(text.contains(from), "mutation anchor `{from}` in {rel}");
        self.write(rel, &text.replacen(from, to, 1));
    }

    fn remove(&self, rel: &str) {
        fs::remove_file(self.root.join(rel)).expect("fixture remove");
    }

    fn run(&self) -> Report {
        let history = Table {
            sealed: self.sealed.clone(),
        };
        let config = Config {
            rows: self.rows,
            seal: SEAL,
        };
        check_tree(&self.root, &config, &history).expect("check ran")
    }

    fn rules(&self) -> BTreeSet<&'static str> {
        self.run()
            .findings
            .iter()
            .map(|finding| finding.rule.code())
            .collect()
    }
}

fn assert_only(fixture: &Fixture, rule: Rule, label: &str) {
    let report = fixture.run();
    let codes: BTreeSet<&str> = report.findings.iter().map(|f| f.rule.code()).collect();
    assert_eq!(
        codes,
        BTreeSet::from([rule.code()]),
        "mutant `{label}` must trip exactly {}: {:#?}",
        rule.code(),
        report.findings
    );
}

/// One single-fault mutant of the clean fixture.
struct Mutant {
    rule: Rule,
    label: &'static str,
    apply: fn(&mut Fixture),
}

fn long_line() -> String {
    format!("{}\n", "x".repeat(MAX_LINE_CHARS + 1))
}

fn mutants() -> Vec<Mutant> {
    vec![
        Mutant {
            rule: Rule::SizeLines,
            label: "single file one line over the cap",
            apply: |f| {
                let text = f.read(SINGLE);
                let pad = "pad\n".repeat(MAX_LINES + 1 - text.lines().count());
                f.write(SINGLE, &format!("{text}{pad}"));
            },
        },
        Mutant {
            rule: Rule::SizeBytes,
            label: "single file one byte over the cap, under the line cap",
            apply: |f| {
                let text = f.read(SINGLE);
                let line = format!("{}\n", "y".repeat(MAX_LINE_CHARS - 1));
                let mut body = text.clone();
                while body.len() + line.len() <= MAX_BYTES {
                    body.push_str(&line);
                }
                body.push_str(&"z".repeat(MAX_BYTES + 1 - body.len()));
                f.write(SINGLE, &body);
            },
        },
        Mutant {
            rule: Rule::SizeLineLength,
            label: "one table row over the character cap",
            apply: |f| f.replace(SINGLE, "none\n", &long_line()),
        },
        Mutant {
            rule: Rule::SizeLineLength,
            label: "a long line hidden in a code fence still counts",
            apply: |f| {
                let fenced = format!("```text\n{}```\n", long_line());
                f.replace(SINGLE, "none\n", &fenced);
            },
        },
        Mutant {
            rule: Rule::IndexSize,
            label: "index over its line cap",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                let pad = "pad\n".repeat(INDEX_MAX_LINES);
                f.replace(&path, "A goal.\n", &pad);
            },
        },
        Mutant {
            rule: Rule::Name,
            label: "new file without a tracker id",
            apply: |f| {
                let text = f.read(SINGLE);
                f.remove(SINGLE);
                f.write("docs/specs/untracked-name.md", &text);
            },
        },
        Mutant {
            rule: Rule::Entry,
            label: "nested directory inside a spec",
            apply: |f| f.write(&format!("{DIR}/nested/more.txt"), "x\n"),
        },
        Mutant {
            rule: Rule::Entry,
            label: "stray file in the spec root",
            apply: |f| f.write("docs/specs/notes.txt", "x\n"),
        },
        Mutant {
            rule: Rule::IndexMissing,
            label: "directory without an index",
            apply: |f| f.remove(&format!("{DIR}/index.md")),
        },
        Mutant {
            rule: Rule::PartUnlisted,
            label: "orphan part",
            apply: |f| {
                let part = part_header("orphan", "nothing");
                f.write(&format!("{DIR}/orphan.md"), &part);
            },
        },
        Mutant {
            rule: Rule::PartMissing,
            label: "listed part is an existing file outside the directory",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                let row = "| [../gh2-single.md](../gh2-single.md) | x | y |\n\n## Current";
                f.replace(&path, "\n## Current", row);
            },
        },
        Mutant {
            rule: Rule::PartHeader,
            label: "part without the back-link",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "Part of: [index](index.md)\n", "");
            },
        },
        Mutant {
            rule: Rule::PartHeader,
            label: "part without its owns line",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "Owns: decisions\n", "");
            },
        },
        Mutant {
            rule: Rule::PartTable,
            label: "part row without when to load it",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(&path, " | designing |", " |  |");
            },
        },
        Mutant {
            rule: Rule::PartTable,
            label: "part listed twice",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                let row = "| [design.md](design.md) | again | again |\n\n## Current";
                f.replace(&path, "\n## Current", row);
            },
        },
        Mutant {
            rule: Rule::Fields,
            label: "index without an owner",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(&path, "- Owner: owner\n", "");
            },
        },
        Mutant {
            rule: Rule::Fields,
            label: "unknown status",
            apply: |f| f.replace(SINGLE, "- Status: draft", "- Status: final"),
        },
        Mutant {
            rule: Rule::Fields,
            label: "short pin",
            apply: |f| f.replace(SINGLE, &format!("Pin: {PIN}"), "Pin: 1111111"),
        },
        Mutant {
            rule: Rule::Sections,
            label: "two sections swapped in a single file",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "## 9. Perf impact",
                    "## 10. Open questions, blockers & unknowns",
                );
                let text = f.read(SINGLE);
                let at = text.rfind("## 10.").expect("second heading");
                let swapped = format!("{}## 9. Perf impact{}", &text[..at], &text[at + 41..]);
                f.write(SINGLE, &swapped);
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "a part misses section 5",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "\n## 5. Boundaries\n\nnone\n", "");
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "index-owned section in a part",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "## 5. Boundaries", "## 8. Review gates triggered");
                let index = format!("{DIR}/delivery.md");
                f.replace(
                    &index,
                    "## 6. Tasks",
                    "## 5. Boundaries\n\nnone\n\n## 6. Tasks",
                );
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "index carries an extra section",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(&path, "## Current blockers", "## Notes\n\nx\n\n## Current blockers");
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "part table order breaks section order",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                let design = "| [design.md](design.md) | D-1 and E-1 | designing |\n";
                f.replace(&path, design, "");
                f.replace(&path, "| [acceptance.md]", &format!("{design}| [acceptance.md]"));
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "template fence loses a section",
            apply: |f| f.replace(TEMPLATE, "## 9. Perf impact\n", ""),
        },
        Mutant {
            rule: Rule::Link,
            label: "relative link target absent",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(
                    &path,
                    "[goal](index.md#1-goal--non-goals)",
                    "[goal](gone.md)",
                );
            },
        },
        Mutant {
            rule: Rule::Link,
            label: "a landed exempt file still needs resolvable links",
            apply: |f| f.write(SMALL, "# old\n\n[x](missing.md)\n"),
        },
        Mutant {
            rule: Rule::Anchor,
            label: "relative link anchor absent",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "index.md#1-goal--non-goals", "index.md#2-gone");
            },
        },
        Mutant {
            rule: Rule::IdDuplicate,
            label: "AC defined twice across parts",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "### B-1: stop", "### Q-1: again\n\n### B-1: stop");
            },
        },
        Mutant {
            rule: Rule::IdDangling,
            label: "reference to an undefined blocker in another part",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(
                    &path,
                    "### D-1: choice\n\nAC-1",
                    "### D-1: choice\n\nB-7 and AC-1",
                );
            },
        },
        Mutant {
            rule: Rule::IdSection,
            label: "decision defined outside the design section",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "### Q-1: which", "### D-2: misplaced");
            },
        },
        Mutant {
            rule: Rule::IdLegacyForm,
            label: "bare AC form",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "AC-1 needs", "AC1 needs");
            },
        },
        Mutant {
            rule: Rule::AcFields,
            label: "AC without its expected RED",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(&path, "  - **Expected RED:** the test fails\n", "");
            },
        },
        Mutant {
            rule: Rule::AcFields,
            label: "AC with an empty Then",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(&path, "- **Then:** a result\n", "- **Then:**\n");
            },
        },
        Mutant {
            rule: Rule::TaskFields,
            label: "task names no blockers",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Blocked by:** B-1\n", "");
            },
        },
        Mutant {
            rule: Rule::TaskFields,
            label: "task blockers are prose",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(
                    &path,
                    "- **Blocked by:** B-1\n",
                    "- **Blocked by:** later\n",
                );
            },
        },
        Mutant {
            rule: Rule::BlockerFields,
            label: "blocker without a check",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Check:** a test\n", "");
            },
        },
        Mutant {
            rule: Rule::EvidenceFields,
            label: "evidence without limits",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Limits:** none\n", "");
            },
        },
        Mutant {
            rule: Rule::OpenBlockers,
            label: "open blocker missing from the index",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(&path, "- B-1\n", "none\n");
            },
        },
        Mutant {
            rule: Rule::OpenBlockers,
            label: "closed blocker still listed as current",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Status:** open", "- **Status:** closed");
            },
        },
        Mutant {
            rule: Rule::PinMissing,
            label: "pin is not a commit here",
            apply: |f| {
                f.replace(
                    SINGLE,
                    &format!("Pin: {PIN}"),
                    &format!("Pin: {ABSENT_SHA}"),
                );
            },
        },
        Mutant {
            rule: Rule::CitePath,
            label: "cited path absent at the pin",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, &format!("`{CITED_FILE}:10`"), "`crates/gone.rs:10`");
            },
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "cited line one past the end",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let past = format!("`{CITED_FILE}:11`");
                f.replace(&path, &format!("`{CITED_FILE}:10`"), &past);
            },
        },
        Mutant {
            rule: Rule::CiteSha,
            label: "short @sha",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, &format!("@{FOREIGN_SHA}"), "@2222222");
            },
        },
        Mutant {
            rule: Rule::BaselineFormat,
            label: "unsorted baseline",
            apply: |f| {
                let text = format!(
                    "{BASELINE_HEADER}\n{SMALL}\t-\t-\t-\texempt\n{BIG}\t600\t-\t-\texempt\n"
                );
                f.write(BASELINE, &text);
            },
        },
        Mutant {
            rule: Rule::BaselineNew,
            label: "swap: a new oversized file takes a drained row's place",
            apply: |f| {
                f.remove(SMALL);
                f.replace(BASELINE, SMALL, "docs/specs/z-new.md");
                f.write("docs/specs/z-new.md", "# new\n");
            },
        },
        Mutant {
            rule: Rule::BaselineCount,
            label: "a row drained but the fixed count did not",
            apply: |f| {
                f.replace(BASELINE, &format!("{SMALL}\t-\t-\t-\texempt\n"), "");
                f.remove(SMALL);
            },
        },
        Mutant {
            rule: Rule::BaselineGrew,
            label: "a listed file grows by one line",
            apply: |f| f.write(BIG, &format!("{}line\n", legacy_big())),
        },
        Mutant {
            rule: Rule::BaselineGrew,
            label: "a listed file crosses a cap its row does not record",
            apply: |f| f.write(SMALL, &format!("# old\n{}", long_line())),
        },
        Mutant {
            rule: Rule::BaselineStale,
            label: "a listed file shrank and the row did not click down",
            apply: |f| f.write(BIG, &"line\n".repeat(599)),
        },
        Mutant {
            rule: Rule::BaselineStale,
            label: "a listed file now fits and its value is still recorded",
            apply: |f| f.write(BIG, &"line\n".repeat(499)),
        },
        Mutant {
            rule: Rule::BaselineStale,
            label: "a listed file now conforms and is still exempt",
            apply: |f| f.write(SMALL, &single_spec()),
        },
        Mutant {
            rule: Rule::BaselineStale,
            label: "a fully compliant file stays listed as ok",
            apply: |f| {
                f.write(SMALL, &single_spec());
                f.replace(
                    BASELINE,
                    &format!("{SMALL}\t-\t-\t-\texempt"),
                    &format!("{SMALL}\t-\t-\t-\tok"),
                );
            },
        },
    ]
}

const ALL_RULES: [Rule; 33] = [
    Rule::SizeLines,
    Rule::SizeBytes,
    Rule::SizeLineLength,
    Rule::IndexSize,
    Rule::Name,
    Rule::Entry,
    Rule::IndexMissing,
    Rule::PartUnlisted,
    Rule::PartMissing,
    Rule::PartHeader,
    Rule::PartTable,
    Rule::Fields,
    Rule::Sections,
    Rule::Link,
    Rule::Anchor,
    Rule::IdDuplicate,
    Rule::IdDangling,
    Rule::IdSection,
    Rule::IdLegacyForm,
    Rule::AcFields,
    Rule::TaskFields,
    Rule::BlockerFields,
    Rule::EvidenceFields,
    Rule::OpenBlockers,
    Rule::PinMissing,
    Rule::CitePath,
    Rule::CiteLine,
    Rule::CiteSha,
    Rule::BaselineFormat,
    Rule::BaselineNew,
    Rule::BaselineCount,
    Rule::BaselineGrew,
    Rule::BaselineStale,
];

#[test]
fn the_clean_fixture_passes_and_reports_what_it_skipped() {
    let fixture = Fixture::new();
    let report = fixture.run();
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
    assert_eq!(report.specs, 4);
    assert_eq!(report.exempt, 2);
    assert_eq!(report.citations_verified, 2, "pin and present @sha");
    assert_eq!(
        report.citations_unverifiable, 1,
        "absent @sha is never a pass"
    );
}

#[test]
fn every_mutant_trips_exactly_its_rule() {
    for mutant in mutants() {
        let mut fixture = Fixture::new();
        (mutant.apply)(&mut fixture);
        assert_only(&fixture, mutant.rule, mutant.label);
    }
}

#[test]
fn every_rule_has_a_mutant() {
    let covered: BTreeSet<&str> = mutants().iter().map(|m| m.rule.code()).collect();
    let all: BTreeSet<&str> = ALL_RULES.iter().map(|rule| rule.code()).collect();
    assert_eq!(all.len(), ALL_RULES.len(), "duplicate rule code");
    assert_eq!(covered, all);
}

/// Green at exactly each cap; the mutants above prove cap plus one.
#[test]
fn values_at_each_cap_pass() {
    let fixture = Fixture::new();
    let text = fixture.read(SINGLE);
    let pad = "pad\n".repeat(MAX_LINES - text.lines().count());
    fixture.write(SINGLE, &format!("{text}{pad}"));
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let fixture = Fixture::new();
    fixture.replace(
        SINGLE,
        "none\n",
        &format!("{}\n", "x".repeat(MAX_LINE_CHARS)),
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let fixture = Fixture::new();
    let mut body = fixture.read(SINGLE);
    let line = format!("{}\n", "y".repeat(MAX_LINE_CHARS - 1));
    while body.len() + line.len() <= MAX_BYTES {
        body.push_str(&line);
    }
    body.push_str(&"z".repeat(MAX_BYTES - body.len()));
    fixture.write(SINGLE, &body);
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let fixture = Fixture::new();
    let path = format!("{DIR}/index.md");
    let text = fixture.read(&path);
    let pad = "pad\n".repeat(INDEX_MAX_LINES - text.lines().count());
    fixture.replace(&path, "A goal.\n", &format!("A goal.\n{pad}"));
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

/// A listed part that is absent is both a missing part and a dead link.
#[test]
fn an_absent_listed_part_is_red() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/index.md");
    let row = "| [ghost.md](ghost.md) | nothing | never |\n\n## Current";
    fixture.replace(&path, "\n## Current", row);
    let rules = fixture.rules();
    assert!(
        rules.contains("PART-MISSING") && rules.contains("LINK"),
        "{rules:?}"
    );
}

#[test]
fn a_ratchet_row_that_records_the_shrink_stays_green() {
    let fixture = Fixture::new();
    fixture.write(BIG, &"line\n".repeat(599));
    fixture.replace(BASELINE, "\t600\t", "\t599\t");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

/// The bypass the character sum exists for: same longest line, one more long line.
#[test]
fn a_second_long_line_grows_the_overflow_even_at_the_same_maximum() {
    let fixture = Fixture::new();
    let long = long_line();
    fixture.write(SMALL, &format!("# old\n{long}"));
    fixture.replace(
        BASELINE,
        &format!("{SMALL}\t-\t-\t-"),
        &format!("{SMALL}\t-\t-\t1"),
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    fixture.write(SMALL, &format!("# old\n{long}{long}"));
    assert_only(&fixture, Rule::BaselineGrew, "second long line");
}

#[test]
fn a_row_above_the_count_is_refused_even_when_it_is_sealed() {
    let mut fixture = Fixture::new();
    fixture.sealed.insert("docs/specs/kel9-third.md".to_owned());
    fixture.write("docs/specs/kel9-third.md", "# third\n");
    let row = "docs/specs/kel9-third.md\t-\t-\t-\texempt\n";
    fixture.replace(BASELINE, BIG, &format!("{row}{BIG}"));
    assert_only(&fixture, Rule::BaselineCount, "extra sealed row");
}

/// A row that does not parse leaves its file unlisted, so more rules fire with it.
#[test]
fn a_malformed_row_is_a_format_defect() {
    let fixture = Fixture::new();
    fixture.replace(BASELINE, "\t600\t", "\tmany\t");
    assert!(fixture.rules().contains("BASELINE-FORMAT"));
    let fixture = Fixture::new();
    fixture.replace(BASELINE, BASELINE_HEADER, "# path");
    assert_only(&fixture, Rule::BaselineFormat, "wrong header");
}

#[test]
fn a_fence_hides_ids_and_a_foreign_id_is_not_resolved() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/design.md");
    let fence = "### D-1: choice\n\n```text\nB-99 AC42\n```\n\nKEL-9 AC-77 and GH-4 B-5\n\nAC-1";
    fixture.replace(&path, "### D-1: choice\n\nAC-1", fence);
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn id_tokens_respect_word_boundaries() {
    let numbers = |line: &str| -> Vec<(&str, u32)> {
        id_tokens(line)
            .into_iter()
            .map(|token| (token.kind, token.number))
            .collect()
    };
    assert_eq!(
        numbers("see AC-1/AC-2, (B-3)."),
        [("AC", 1), ("AC", 2), ("B", 3)]
    );
    assert_eq!(numbers("CODE-T-1 and T-1x and x-U-2 and Q-3-b"), []);
    assert_eq!(numbers("https://example.com/AC-1 and D-Bus"), []);
}

#[test]
fn citations_parse_only_repository_paths() {
    assert_eq!(parse_citation("a/b.rs:12"), Some(("a/b.rs", 12, None)));
    assert_eq!(parse_citation("a/b.rs:3-9"), Some(("a/b.rs", 9, None)));
    assert_eq!(
        parse_citation("a/b.rs:3@abc"),
        Some(("a/b.rs", 3, Some("abc")))
    );
    assert_eq!(parse_citation("127.0.0.1:8080"), None);
    assert_eq!(parse_citation("http://x/y:80"), None);
    assert_eq!(parse_citation("a/b.rs"), None);
}

#[test]
fn slugs_match_github_anchors() {
    let doc = Doc::parse(
        "x.md",
        "# Top\n\n## 1. Goal & non-goals\n\n## `code` [link](y.md)\n\n## Top\n\n```\n# hidden\n```\n",
    );
    let slugs = heading_slugs(&doc);
    for slug in ["top", "1-goal--non-goals", "code-link", "top-1"] {
        assert!(slugs.contains(slug), "{slug}: {slugs:?}");
    }
    assert!(!slugs.contains("hidden"));
}

#[test]
fn crlf_checkouts_measure_the_same_as_lf() {
    let lf = Doc::parse("x.md", "a\nb\n");
    let crlf = Doc::parse("x.md", "a\r\nb\r\n");
    assert_eq!(measure(&lf), measure(&crlf));
}

/// The adapter against a real repository, including the fail-closed seal.
#[test]
fn git_history_reads_a_real_repository_and_fails_closed_on_a_missing_seal() {
    let fixture = Fixture::new();
    let git = |args: &[&str]| {
        let mut command = Command::new("git");
        command.arg("-C").arg(&fixture.root).args(args);
        for name in GIT_ENV {
            command.env_remove(name);
        }
        let output = command.output().expect("git runs");
        assert!(output.status.success(), "{args:?}: {output:?}");
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    git(&["init", "-q"]);
    git(&["config", "maintenance.auto", "false"]);
    git(&["add", "docs"]);
    let identity = ["-c", "user.name=t", "-c", "user.email=t@example.invalid"];
    git(&[&identity[..], &["commit", "-q", "--no-gpg-sign", "-m", "x"]].concat());
    let head = git(&["rev-parse", "HEAD"]);

    let history = GitHistory {
        root: &fixture.root,
    };
    assert!(history.has_commit(&head));
    assert!(!history.has_commit(ABSENT_SHA));
    assert_eq!(history.lines_at(&head, BIG), Some(600));
    assert_eq!(history.lines_at(&head, "docs/specs/absent.md"), None);
    let paths = history.paths_at(&head, SPEC_ROOT).expect("tree reads");
    assert!(paths.contains(BIG) && paths.contains(SINGLE));

    let error = history
        .paths_at(ABSENT_SHA, SPEC_ROOT)
        .expect_err("seal absent");
    assert!(error.contains("fetch-depth: 0"), "{error}");
    let config = Config {
        rows: 2,
        seal: ABSENT_SHA,
    };
    let refused = check_tree(&fixture.root, &config, &history);
    assert!(refused.is_err(), "a missing seal must not read as a pass");
}
