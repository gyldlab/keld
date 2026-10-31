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

/// Git facts for fixtures: one reachable pin with a ten-line file, one foreign head that
/// is present but unreachable, one seal.
struct Table {
    sealed: BTreeMap<String, String>,
    tracked: Vec<String>,
}

impl History for Table {
    fn paths_at(&self, commit: &str, _dir: &str) -> Result<BTreeSet<String>, String> {
        assert_eq!(commit, SEAL);
        Ok(self.sealed.keys().cloned().collect())
    }
    fn has_commit(&self, sha: &str) -> bool {
        sha == PIN || sha == FOREIGN_SHA
    }
    fn is_reachable(&self, sha: &str) -> bool {
        sha == PIN
    }
    fn text_at(&self, commit: &str, path: &str) -> Option<String> {
        if commit == SEAL {
            return self.sealed.get(path).cloned();
        }
        (path == CITED_FILE && (commit == PIN || commit == FOREIGN_SHA))
            .then(|| "line\n".repeat(10))
    }
    fn tracked_markdown(&self) -> Result<Vec<String>, String> {
        Ok(self.tracked.clone())
    }
}

struct Fixture {
    root: PathBuf,
    rows: usize,
    sealed: BTreeMap<String, String>,
    tracked: Vec<String>,
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
const SINGLE: &str = "docs/specs/gh1-single.md";
const BASELINE: &str = "docs/specs/.spec-baseline.tsv";

impl Fixture {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!("keld-spec-structure-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let fixture = Fixture {
            root,
            rows: 2,
            sealed: BTreeMap::from([
                (BIG.to_owned(), legacy_big()),
                (SMALL.to_owned(), "# old\n".to_owned()),
            ]),
            tracked: Vec::new(),
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
            tracked: self.tracked.clone(),
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
                let row = "| [../gh1-single.md](../gh1-single.md) | x | y |\n\n## Current";
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
            rule: Rule::Fields,
            label: "tracker left at the template value",
            apply: |f| f.replace(SINGLE, "Tracker: GH-1", "Tracker: GH-0"),
        },
        Mutant {
            rule: Rule::Fields,
            label: "directory tracker disagrees with the directory name",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(&path, "Tracker: GH-1", "Tracker: KEL-1");
            },
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
                f.replace(
                    &path,
                    "## Current blockers",
                    "## Notes\n\nx\n\n## Current blockers",
                );
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "part table order breaks section order",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                let design = "| [design.md](design.md) | D-1 and E-1 | designing |\n";
                f.replace(&path, design, "");
                f.replace(
                    &path,
                    "| [acceptance.md]",
                    &format!("{design}| [acceptance.md]"),
                );
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
            rule: Rule::LinkCode,
            label: "a link into code goes stale without running this gate",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(
                    &path,
                    "[goal](index.md#1-goal--non-goals)",
                    "[goal](../../../crates/x/src/lib.rs)",
                );
            },
        },
        Mutant {
            rule: Rule::Outside,
            label: "a spec written outside the spec root",
            apply: |f| {
                let path = "docs/design/gh9-outside.md";
                f.write(path, "# Spec\n\n## 3. Acceptance criteria\n");
                f.tracked.push(path.to_owned());
            },
        },
        Mutant {
            rule: Rule::Outside,
            label: "a landed spec moved out under its own name",
            apply: |f| {
                let path = "docs/archive/old-big.md";
                f.write(path, "# moved\n");
                f.tracked.push(path.to_owned());
            },
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
            rule: Rule::IdDangling,
            label: "a local link in front of an ID does not make it foreign",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "non-goals),", "non-goals) AC-9,");
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
            rule: Rule::IdRequired,
            label: "a spec with no acceptance criterion",
            apply: |f| f.replace(SINGLE, &ac_block(1), "none\n\n"),
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "a spec with no task",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "### T-1: slice\n\n- **Blocked by:** none\n\n",
                    "none\n\n",
                )
            },
        },
        Mutant {
            rule: Rule::Fence,
            label: "an AC definition hidden in a fence",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let fence = "\n```text\n### AC-9: hidden\n```\n\n## 5. Boundaries";
                f.replace(&path, "\n## 5. Boundaries", fence);
            },
        },
        Mutant {
            rule: Rule::Fence,
            label: "a fence that never closes",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.write(&path, &format!("{}\n```text\nhidden\n", f.read(&path)));
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
            rule: Rule::FieldUnknown,
            label: "a new spec defers its negative control",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "**Single fault:** the rule is removed",
                    "**Single fault:** UNKNOWN: later",
                );
            },
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "a new spec names no owner",
            apply: |f| f.replace(SINGLE, "- Owner: owner", "- Owner: UNKNOWN (nobody yet)"),
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a template slot left in a negative control",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "**Expected RED:** the test fails",
                    "**Expected RED:** <what fails>",
                );
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a bare none is not a negative control",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "**At-risk precondition:** the state holds",
                    "**At-risk precondition:** none",
                );
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a blocker check left at TBD",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Check:** a test", "- **Check:** TBD");
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a template slot left in the owner",
            apply: |f| f.replace(SINGLE, "- Owner: owner", "- Owner: <human>"),
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
            rule: Rule::PinMissing,
            label: "pin is a commit here that this branch cannot reach",
            apply: |f| {
                f.replace(
                    SINGLE,
                    &format!("Pin: {PIN}"),
                    &format!("Pin: {FOREIGN_SHA}"),
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
            rule: Rule::BaselineRaised,
            label: "a row raised to fit a grown file",
            apply: |f| {
                f.write(BIG, &format!("{}line\n", legacy_big()));
                f.replace(BASELINE, "\t600\t", "\t601\t");
            },
        },
        Mutant {
            rule: Rule::BaselineRaised,
            label: "a row records a cap its file passed at the seal",
            apply: |f| {
                f.write(SMALL, &format!("# old\n{}", long_line()));
                f.replace(
                    BASELINE,
                    &format!("{SMALL}\t-\t-\t-"),
                    &format!("{SMALL}\t-\t-\t1"),
                );
            },
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

const ALL_RULES: [Rule; 40] = [
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
    Rule::LinkCode,
    Rule::Anchor,
    Rule::Outside,
    Rule::IdDuplicate,
    Rule::IdDangling,
    Rule::IdSection,
    Rule::IdLegacyForm,
    Rule::IdRequired,
    Rule::Fence,
    Rule::AcFields,
    Rule::TaskFields,
    Rule::BlockerFields,
    Rule::EvidenceFields,
    Rule::FieldUnknown,
    Rule::FieldPlaceholder,
    Rule::OpenBlockers,
    Rule::PinMissing,
    Rule::CitePath,
    Rule::CiteLine,
    Rule::CiteSha,
    Rule::BaselineFormat,
    Rule::BaselineNew,
    Rule::BaselineCount,
    Rule::BaselineGrew,
    Rule::BaselineRaised,
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
    let mut fixture = Fixture::new();
    let long = long_line();
    let at_seal = format!("# old\n{long}{long}{long}");
    fixture.sealed.insert(SMALL.to_owned(), at_seal);
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
    let third = (
        "docs/specs/kel9-third.md".to_owned(),
        "# third\n".to_owned(),
    );
    fixture.sealed.insert(third.0, third.1);
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

const MIGRATED_ROW: &str = "docs/specs/gh1-directory.md";

/// A sealed flat spec, now a directory, with three unfilled control fields of one AC.
fn migrated_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.rows = 3;
    fixture
        .sealed
        .insert(MIGRATED_ROW.to_owned(), "# flat\n".to_owned());
    fixture.replace(
        BASELINE,
        BIG,
        &format!("{MIGRATED_ROW}\t-\t-\t-\tmigrated:3\n{BIG}"),
    );
    let path = format!("{DIR}/acceptance.md");
    for field in [
        "Single fault:** the rule is removed",
        "At-risk precondition:** the state holds",
        "Expected RED:** the test fails",
    ] {
        let (label, _) = field.split_once("** ").expect("field anchor");
        fixture.replace(&path, field, &format!("{label}** UNKNOWN: none named"));
    }
    fixture
}

#[test]
fn only_a_sealed_spec_can_keep_unknown_required_fields() {
    let fixture = migrated_fixture();
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let mut unsealed = migrated_fixture();
    unsealed.sealed.remove(MIGRATED_ROW);
    assert_only(
        &unsealed,
        Rule::BaselineNew,
        "a new spec claims a migrated row",
    );

    let fixture = migrated_fixture();
    fixture.replace(
        &format!("{DIR}/index.md"),
        "- Owner: owner",
        "- Owner: UNKNOWN",
    );
    assert_only(&fixture, Rule::BaselineGrew, "a fourth UNKNOWN field");

    let fixture = migrated_fixture();
    let path = format!("{DIR}/acceptance.md");
    fixture.replace(
        &path,
        "Single fault:** UNKNOWN: none named",
        "Single fault:** a real fault",
    );
    assert_only(
        &fixture,
        Rule::BaselineStale,
        "one field filled, count not lowered",
    );
    fixture.replace(BASELINE, "migrated:3", "migrated:2");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let fixture = migrated_fixture();
    fixture.write(&path, &acceptance_part());
    assert_only(
        &fixture,
        Rule::BaselineStale,
        "every field filled, row kept",
    );
}

#[test]
fn a_flat_landed_spec_can_migrate_in_place() {
    let fixture = Fixture::new();
    let text = single_spec().replace(
        "**Expected RED:** the test fails",
        "**Expected RED:** UNKNOWN",
    );
    fixture.write(SMALL, &text);
    fixture.replace(
        BASELINE,
        &format!("{SMALL}\t-\t-\t-\texempt"),
        &format!("{SMALL}\t-\t-\t-\tmigrated:1"),
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn a_migrated_row_with_no_spec_left_is_stale() {
    let fixture = migrated_fixture();
    fs::remove_dir_all(fixture.root.join(DIR)).expect("fixture remove");
    assert_only(&fixture, Rule::BaselineStale, "migrated spec deleted");
}

/// Only a `migrated:<n>` row follows its spec into a directory.
#[test]
fn an_exempt_row_does_not_cover_a_directory() {
    let mut fixture = Fixture::new();
    fixture.rows = 3;
    fixture
        .sealed
        .insert(MIGRATED_ROW.to_owned(), "# flat\n".to_owned());
    fixture.replace(
        BASELINE,
        BIG,
        &format!("{MIGRATED_ROW}\t-\t-\t-\texempt\n{BIG}"),
    );
    assert_only(
        &fixture,
        Rule::BaselineStale,
        "flat file gone, exempt row kept",
    );
}

#[test]
fn a_migrated_count_must_be_a_positive_number() {
    for cell in ["migrated:0", "migrated:x", "migrated"] {
        let fixture = migrated_fixture();
        fixture.replace(BASELINE, "migrated:3", cell);
        assert!(fixture.rules().contains("BASELINE-FORMAT"), "{cell}");
    }
}

#[test]
fn real_statements_are_not_non_values() {
    for value in [
        "None of the handlers run.",
        "Unknown channels are refused.",
        "unknown channel accepted",
        "UNKNOWNS stay labelled",
        "Pending calls fail",
        "INFERRED: a page runs",
        "a `<state>` slot in prose",
    ] {
        assert_eq!(non_value(value, true), None, "{value}");
    }
    assert_eq!(
        non_value("none", false),
        None,
        "`Given: none` is a statement"
    );
    for value in [
        "UNKNOWN",
        "UNKNOWN: none named",
        "UNKNOWN (no owner)",
        "unknown.",
    ] {
        assert_eq!(non_value(value, false), Some(NonValue::Unknown), "{value}");
    }
    for value in ["<state>", "TBD", "todo", "N/A", "-", "?", "pending"] {
        assert_eq!(
            non_value(value, false),
            Some(NonValue::Placeholder),
            "{value}"
        );
    }
    assert_eq!(non_value("None.", true), Some(NonValue::Placeholder));
}

/// A blocker owner may be unknown: root AGENTS.md says to state that and name the first step.
#[test]
fn a_blocker_may_name_an_unknown_owner() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/delivery.md");
    fixture.replace(
        &path,
        "- **Owner:** owner",
        "- **Owner:** UNKNOWN: ask the maintainers",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

/// The template's slots are the copy source; the same slot in a copy is a defect above.
#[test]
fn the_template_fence_may_keep_its_slots() {
    let fixture = Fixture::new();
    fixture.replace(TEMPLATE, "- Owner: owner", "- Owner: <human>");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn a_non_spec_page_outside_the_root_is_not_a_spec() {
    let mut fixture = Fixture::new();
    fixture.write(
        "docs/guide.md",
        "# Guide\n\n```text\n## 3. Acceptance criteria\n```\n",
    );
    fixture.tracked.push("docs/guide.md".to_owned());
    fixture.tracked.push(TEMPLATE.to_owned());
    fixture.tracked.push(SINGLE.to_owned());
    fixture
        .tracked
        .push("docs/deleted-in-the-worktree.md".to_owned());
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn a_link_may_reach_any_documentation_but_no_code() {
    for path in [
        "docs/a.png",
        "docs",
        ".agents/ci.md",
        "AGENTS.md",
        "crates/x/README.md",
    ] {
        assert!(in_docs_lane(path), "{path}");
    }
    for path in [
        "crates/x/src/lib.rs",
        "justfile",
        "docs-old/a.png",
        ".agents-old/a.txt",
    ] {
        assert!(!in_docs_lane(path), "{path}");
    }
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
    let foreign = |line: &str| id_tokens(line).iter().all(|token| token.foreign);
    assert!(foreign("[#1311](https://example.com/x) AC-9"));
    assert!(foreign("[KEL-9](u), AC-9"));
    assert!(!foreign("[goal](index.md) AC-9"));
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
    assert_eq!(parse_citation("a/b.rs:12:5"), Some(("a/b.rs", 12, None)));
    assert_eq!(
        parse_citation(".agents/ci.md:3"),
        Some((".agents/ci.md", 3, None))
    );
    assert_eq!(parse_citation("ghcr.io/org/image:11"), None, "image tag");
    assert_eq!(parse_citation("ghcr.io/org/image.v1:11"), None, "domain");
    assert_eq!(parse_citation("org/image:11"), None, "no file extension");
    assert_eq!(parse_citation("C:/x/y.rs:3"), None, "absolute Windows path");
    assert_eq!(parse_citation("/usr/lib/y.so:3"), None, "absolute path");
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
    git(&[
        &identity[..],
        &["commit", "-q", "--allow-empty", "--no-gpg-sign", "-m", "y"],
    ]
    .concat());
    let unreachable = git(&["rev-parse", "HEAD"]);
    git(&["reset", "-q", "--soft", &head]);

    let history = GitHistory {
        root: &fixture.root,
    };
    assert!(history.has_commit(&head));
    assert!(history.is_reachable(&head));
    assert!(history.has_commit(&unreachable));
    assert!(!history.is_reachable(&unreachable), "a dropped commit");
    assert!(!history.is_reachable(ABSENT_SHA));
    let tracked = history.tracked_markdown().expect("index reads");
    assert!(tracked.iter().any(|path| path == BIG), "{tracked:?}");
    assert!(
        tracked.iter().all(|path| path.ends_with(".md")),
        "{tracked:?}"
    );
    assert!(!history.has_commit(ABSENT_SHA));
    assert_eq!(history.lines_at(&head, BIG), Some(600));
    assert_eq!(history.lines_at(&head, "docs/specs/absent.md"), None);
    let paths = history.paths_at(&head, SPEC_ROOT).expect("tree reads");
    assert!(paths.contains(BIG) && paths.contains(SINGLE));

    let error = history
        .paths_at(ABSENT_SHA, SPEC_ROOT)
        .expect_err("seal absent");
    assert!(
        error.contains(ABSENT_SHA) && error.contains(&seal_fix(false)),
        "{error}"
    );
    assert!(seal_fix(true).contains("fetch-depth: 0"));
    assert!(seal_fix(false).contains("git fetch origin main"));
    let config = Config {
        rows: 2,
        seal: ABSENT_SHA,
    };
    let refused = check_tree(&fixture.root, &config, &history);
    assert!(refused.is_err(), "a missing seal must not read as a pass");
}
