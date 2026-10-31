//! Each rule has one single-fault fixture mutant.
//!
//! The clean fixture passes first, so a mutant's RED comes from its fault alone. Every
//! mutant must trip exactly the rule it names: if a second rule also fired, deleting
//! the first rule could leave the fixture red and hide that deletion.

use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const PIN: &str = "1111111111111111111111111111111111111111";
const FOREIGN_SHA: &str = "2222222222222222222222222222222222222222";
const ABSENT_SHA: &str = "3333333333333333333333333333333333333333";
const SEAL: &str = "4444444444444444444444444444444444444444";
const BASE: &str = "5555555555555555555555555555555555555555";
const CITED_FILE: &str = "crates/x/src/lib.rs";

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// Git facts for fixtures: one reachable pin with a ten-line file, one foreign head that
/// is present but unreachable, one seal.
struct Table {
    sealed: BTreeMap<String, String>,
    tracked: Vec<String>,
    symlinks: Vec<String>,
    /// The baseline at the comparison base, or why the base does not resolve.
    previous: Result<Option<String>, String>,
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
    fn tracked_files(&self) -> Result<Vec<Tracked>, String> {
        let file = |path: &String, symlink| Tracked {
            path: path.clone(),
            symlink,
        };
        let plain = self.tracked.iter().map(|path| file(path, false));
        let links = self.symlinks.iter().map(|path| file(path, true));
        Ok(plain.chain(links).collect())
    }
    fn previous_baseline(&self, _path: &str) -> Result<Previous, String> {
        self.previous.clone().map(|text| Previous {
            commit: BASE.to_owned(),
            text,
        })
    }
}

struct Fixture {
    root: PathBuf,
    rows: usize,
    sealed: BTreeMap<String, String>,
    tracked: Vec<String>,
    symlinks: Vec<String>,
    previous: Result<Option<String>, String>,
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
            symlinks: Vec::new(),
            previous: Ok(None),
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
        self.write_bytes(rel, text.as_bytes());
    }

    fn write_bytes(&self, rel: &str, bytes: &[u8]) {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture dir");
        fs::write(path, bytes).expect("fixture write");
    }

    /// A tracked file outside the spec root, which the census must read.
    fn track(&mut self, rel: &str, bytes: &[u8]) {
        self.write_bytes(rel, bytes);
        self.tracked.push(rel.to_owned());
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

    fn history(&self) -> Table {
        Table {
            sealed: self.sealed.clone(),
            tracked: self.tracked.clone(),
            symlinks: self.symlinks.clone(),
            previous: self.previous.clone(),
        }
    }

    fn try_run(&self) -> Result<Report, String> {
        let config = Config {
            rows: self.rows,
            seal: SEAL,
        };
        check_tree(&self.root, &config, &self.history())
    }

    fn run(&self) -> Report {
        self.try_run().expect("check ran")
    }

    /// Makes the committed baseline the comparison base, as after this PR lands.
    fn commit_baseline(&mut self) {
        self.previous = Ok(Some(self.read(BASELINE)));
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
        // Finding 4430014030: only a blocker owner may be UNKNOWN.
        Mutant {
            rule: Rule::FieldUnknown,
            label: "a new spec claims evidence with an UNKNOWN run",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Run:** one", "- **Run:** UNKNOWN");
            },
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "a new spec leaves evidence limits UNKNOWN",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Limits:** none", "- **Limits:** UNKNOWN: later");
            },
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "a new spec leaves a blocker impact UNKNOWN",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Impact:** blocks T-1", "- **Impact:** UNKNOWN");
            },
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "a new spec leaves a blocker check UNKNOWN",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Check:** a test", "- **Check:** UNKNOWN");
            },
        },
        // Finding 4430014035: nested content meets the same non-value rules.
        Mutant {
            rule: Rule::FieldUnknown,
            label: "an indented UNKNOWN fills an empty Then",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "- **Then:** a result\n",
                    "- **Then:**\n  - UNKNOWN\n",
                );
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "an indented TBD fills an empty Single fault",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "  - **Single fault:** the rule is removed\n",
                    "  - **Single fault:**\n    - TBD\n",
                );
            },
        },
        Mutant {
            rule: Rule::AcFields,
            label: "a bare nested list marker fills an empty Then",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(&path, "- **Then:** a result\n", "- **Then:**\n  *\n");
            },
        },
        Mutant {
            rule: Rule::AcFields,
            label: "a nested comment fills an empty Then",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "- **Then:** a result\n",
                    "- **Then:**\n  <!-- a result -->\n",
                );
            },
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "an indented UNKNOWN fills the header owner",
            apply: |f| f.replace(SINGLE, "- Owner: owner\n", "- Owner:\n  - UNKNOWN\n"),
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "an indented TBD fills a blocker owner",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "- **Owner:** owner\n", "- **Owner:**\n  - TBD\n");
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a value of bare punctuation states nothing",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(&path, "- **Then:** a result", "- **Then:** .");
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a fenced TBD fills an empty Then",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "- **Then:** a result\n",
                    "- **Then:**\n  ```text\n  TBD\n  ```\n",
                );
            },
        },
        // Finding 4430014043: one spec has one location.
        Mutant {
            rule: Rule::Collision,
            label: "a flat spec shadows a directory spec",
            apply: |f| f.write(&format!("{DIR}.md"), &single_spec()),
        },
        // Finding 4430014046: a shrink is permanent.
        Mutant {
            rule: Rule::BaselineGrew,
            label: "a file and its row regrow to the seal after a shrink",
            apply: |f| {
                let row = format!("{BIG}\t599\t-\t-\texempt\n{SMALL}\t-\t-\t-\texempt\n");
                f.previous = Ok(Some(format!("{BASELINE_HEADER}\n{row}")));
            },
        },
        Mutant {
            rule: Rule::BaselineGrew,
            label: "a value that left its row at the base comes back",
            apply: |f| {
                let row = format!("{BIG}\t-\t-\t-\texempt\n{SMALL}\t-\t-\t-\texempt\n");
                f.previous = Ok(Some(format!("{BASELINE_HEADER}\n{row}")));
            },
        },
        Mutant {
            rule: Rule::BaselineGrew,
            label: "a structure cell widens from its base",
            apply: |f| {
                let row = format!("{BIG}\t600\t-\t-\texempt\n{SMALL}\t-\t-\t-\tok\n");
                f.previous = Ok(Some(format!("{BASELINE_HEADER}\n{row}")));
            },
        },
        Mutant {
            rule: Rule::BaselineNew,
            label: "a drained row returns",
            apply: |f| {
                let row = format!("{BIG}\t600\t-\t-\texempt\n");
                f.previous = Ok(Some(format!("{BASELINE_HEADER}\n{row}")));
            },
        },
        // Finding 4430014051: a directory holds no flat size.
        Mutant {
            rule: Rule::BaselineRaised,
            label: "a migrated directory row reacquires a line allowance",
            apply: |f| {
                migrate(f);
                let old = "line\n".repeat(600);
                f.sealed.insert(MIGRATED_ROW.to_owned(), old);
                f.replace(
                    BASELINE,
                    &format!("{MIGRATED_ROW}\t-\t-\t-"),
                    &format!("{MIGRATED_ROW}\t600\t-\t-"),
                );
            },
        },
        Mutant {
            rule: Rule::BaselineRaised,
            label: "a migrated directory row reacquires a byte allowance",
            apply: |f| {
                migrate(f);
                let old = "line\n".repeat(600);
                f.sealed.insert(MIGRATED_ROW.to_owned(), old);
                f.replace(
                    BASELINE,
                    &format!("{MIGRATED_ROW}\t-\t-\t-"),
                    &format!("{MIGRATED_ROW}\t-\t3000\t-"),
                );
            },
        },
        Mutant {
            rule: Rule::BaselineRaised,
            label: "a migrated directory row reacquires a line-overflow allowance",
            apply: |f| {
                migrate(f);
                let old = format!("{}\n", "x".repeat(MAX_LINE_CHARS + 9));
                f.sealed.insert(MIGRATED_ROW.to_owned(), old);
                f.replace(
                    BASELINE,
                    &format!("{MIGRATED_ROW}\t-\t-\t-"),
                    &format!("{MIGRATED_ROW}\t-\t-\t9"),
                );
            },
        },
        // Finding 4430014056: each UNKNOWN is bound to its slot.
        Mutant {
            rule: Rule::BaselineGrew,
            label: "a filled field trades places with an unfilled one",
            apply: |f| {
                migrate(f);
                let path = format!("{DIR}/acceptance.md");
                f.replace(&path, "**Then:** a result", "**Then:** UNKNOWN");
                f.replace(
                    &path,
                    "Single fault:** UNKNOWN: none named",
                    "Single fault:** a real fault",
                );
            },
        },
        Mutant {
            rule: Rule::BaselineGrew,
            label: "the row follows the trade but the base does not",
            apply: |f| {
                migrate(f);
                f.commit_baseline();
                let path = format!("{DIR}/acceptance.md");
                f.replace(&path, "**Then:** a result", "**Then:** UNKNOWN");
                f.replace(
                    &path,
                    "Single fault:** UNKNOWN: none named",
                    "Single fault:** a real fault",
                );
                f.replace(BASELINE, "AC-1.Single fault", "AC-1.Then");
            },
        },
        // Finding 4430014058: the lane is a rule on the real file.
        Mutant {
            rule: Rule::LinkCode,
            label: "a tracked symlink checked out as text leaves the lane",
            apply: |f| {
                f.write("docs/alias.md", "../crates/x/src/lib.rs");
                f.symlinks.push("docs/alias.md".to_owned());
                link_from_design(f, "../../alias.md");
            },
        },
        Mutant {
            rule: Rule::LinkCode,
            label: "a tracked symlink ancestor leaves the lane",
            apply: |f| {
                f.write("docs/alias/lib.md", "# real\n");
                f.symlinks.push("docs/alias".to_owned());
                link_from_design(f, "../../alias/lib.md");
            },
        },
        // Finding 4430014060: the one lexer follows CommonMark ATX headings.
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading indented one space",
            apply: |f| f.track("docs/n.md", b"# Spec\n\n ## Acceptance criteria\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading indented three spaces",
            apply: |f| f.track("docs/n.md", b"# Spec\n\n   ## Acceptance criteria\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading with a tab after the hashes",
            apply: |f| f.track("docs/n.md", b"# Spec\n\n##\tAcceptance criteria\n"),
        },
        Mutant {
            rule: Rule::Sections,
            label: "an indented heading is a section too",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "## 9. Perf impact",
                    "## 9. Perf impact\n\n  ## Extra",
                )
            },
        },
        // Finding 4430014066: an unreadable census input is never proof.
        Mutant {
            rule: Rule::Unreadable,
            label: "tracked Markdown that is not UTF-8",
            apply: |f| f.track("crates/x/n.md", b"# Spec\n## Acceptance criteria\n\xff"),
        },
        Mutant {
            rule: Rule::Unreadable,
            label: "a tracked Markdown path that cannot be read",
            apply: |f| {
                fs::create_dir_all(f.root.join("docs/odd.md")).expect("fixture dir");
                f.tracked.push("docs/odd.md".to_owned());
            },
        },
        // Finding 4430014068: code spans match by delimiter width.
        Mutant {
            rule: Rule::CiteLine,
            label: "a cited line past the end in a two-backtick span",
            apply: |f| cite_in_design(f, &format!("``{CITED_FILE}:999``")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a cited line past the end in a padded span",
            apply: |f| cite_in_design(f, &format!("`` {CITED_FILE}:999 ``")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a span that opens after an unmatched wider run",
            apply: |f| cite_in_design(f, &format!("``` then `{CITED_FILE}:999`")),
        },
        // Finding 4430014070: both ends of a range must exist, in order.
        Mutant {
            rule: Rule::CiteLine,
            label: "a reversed range ending on a real line",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:999-2`")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a range whose start is past the end",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:11-10`")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a reversed range inside the file",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:9-3`")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a range that starts at zero",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:0-1`")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a range with a third part",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:1-2-3`")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a line number no integer holds",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:99999999999999999999999`")),
        },
        Mutant {
            rule: Rule::CiteLine,
            label: "a reversed range with a column",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:9-3:5`")),
        },
        // Finding 4430018059: a comment renders nothing, so it defines nothing.
        Mutant {
            rule: Rule::IdRequired,
            label: "the only AC sits in a multi-line comment",
            apply: |f| {
                let block = format!("<!--\n{}-->\n\n", ac_block(1));
                f.replace(SINGLE, &ac_block(1), &block);
            },
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "the only task sits in a multi-line comment",
            apply: |f| {
                let task = "### T-1: slice\n\n- **Blocked by:** none\n\n";
                f.replace(SINGLE, task, &format!("<!--\n{task}-->\n\n"));
            },
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "a comment that opens mid-line hides the definition",
            apply: |f| {
                let block = format!("text <!--\n{}-->\n\n", ac_block(1));
                f.replace(SINGLE, &ac_block(1), &block);
            },
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "text after a block comment on its line is no heading",
            apply: |f| f.replace(SINGLE, "### AC-1: rule", "<!-- x --> ### AC-1: rule"),
        },
        Mutant {
            rule: Rule::Comment,
            label: "a comment that never closes",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.write(&path, &format!("{}\n<!-- hidden\n", f.read(&path)));
            },
        },
        Mutant {
            rule: Rule::Comment,
            label: "a raw HTML block that never closes",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.write(&path, &format!("{}\n<pre>\nhidden\n", f.read(&path)));
            },
        },
        Mutant {
            rule: Rule::AcFields,
            label: "a field value that is only a comment",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                f.replace(
                    &path,
                    "- **Then:** a result",
                    "- **Then:** <!-- a result -->",
                );
            },
        },
        Mutant {
            rule: Rule::Anchor,
            label: "a heading in a comment is no anchor",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(&path, "A goal.\n", "A goal.\n\n<!--\n# hidden\n-->\n");
                let path = format!("{DIR}/design.md");
                f.replace(&path, "index.md#1-goal--non-goals", "index.md#hidden");
            },
        },
        // Same-class sweep: other ways Markdown hides or fakes content.
        Mutant {
            rule: Rule::Outside,
            label: "an off-root setext heading",
            apply: |f| f.track("docs/n.md", b"# Spec\n\nAcceptance criteria\n---\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading split over a setext paragraph",
            apply: |f| f.track("docs/n.md", b"Acceptance\ncriteria\n===\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading with a closing run",
            apply: |f| f.track("docs/n.md", b"## Acceptance criteria ##\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading in emphasis and double spaces",
            apply: |f| f.track("docs/n.md", b"## **Acceptance**  *criteria*\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root struck heading",
            apply: |f| f.track("docs/n.md", b"## ~~Acceptance criteria~~\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading in a block quote",
            apply: |f| f.track("docs/n.md", b"> > ## Acceptance criteria\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading in a list item",
            apply: |f| f.track("docs/n.md", b"- ## Acceptance criteria\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading after a BOM",
            apply: |f| f.track("docs/n.md", "\u{feff}## Acceptance criteria\n".as_bytes()),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root heading after a bare CR",
            apply: |f| f.track("docs/n.md", b"# Spec\r## Acceptance criteria\r"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root spec under another Markdown extension",
            apply: |f| f.track("docs/n.markdown", b"## Acceptance criteria\n"),
        },
        Mutant {
            rule: Rule::Outside,
            label: "an off-root spec under an upper-case extension",
            apply: |f| f.track("docs/n.MD", b"## Acceptance criteria\n"),
        },
        Mutant {
            rule: Rule::Sections,
            label: "a setext heading adds an eleventh section",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "## 9. Perf impact",
                    "Extra\n---\n\n## 9. Perf impact",
                )
            },
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "an AC heading in indented code is code",
            apply: |f| f.replace(SINGLE, "### AC-1: rule", "    ### AC-1: rule"),
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "an AC heading after a tab is code",
            apply: |f| f.replace(SINGLE, "### AC-1: rule", "\t### AC-1: rule"),
        },
        Mutant {
            rule: Rule::AcFields,
            label: "a field in indented code is code",
            apply: |f| {
                let path = format!("{DIR}/acceptance.md");
                let code = "    - **Given:** a state\n\n";
                f.replace(&path, "- **Given:** a state\n", code);
            },
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "an AC in an HTML block is raw text",
            apply: |f| f.replace(SINGLE, "### AC-1: rule", "<details>\n### AC-1: rule"),
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "an AC after a lone tag is raw text",
            apply: |f| f.replace(SINGLE, "### AC-1: rule", "<span>\n### AC-1: rule"),
        },
        Mutant {
            rule: Rule::IdRequired,
            label: "a raw element hides an AC across blank lines",
            apply: |f| {
                let block = format!("<pre>\n\n{}</pre>\n\n", ac_block(1));
                f.replace(SINGLE, &ac_block(1), &block);
            },
        },
        Mutant {
            rule: Rule::Fence,
            label: "an AC definition hidden in a tilde fence",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let fence = "\n~~~text\n### AC-9: hidden\n~~~\n\n## 5. Boundaries";
                f.replace(&path, "\n## 5. Boundaries", fence);
            },
        },
        Mutant {
            rule: Rule::Fence,
            label: "a tilde fence that a backtick fence does not close",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.write(&path, &format!("{}\n~~~\nhidden\n```\n", f.read(&path)));
            },
        },
        Mutant {
            rule: Rule::IdDangling,
            label: "one line of triple-backtick code opens no fence",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(
                    &path,
                    "### D-1: choice\n",
                    "### D-1: choice\n\n```a```\n\nB-7\n",
                );
            },
        },
        Mutant {
            rule: Rule::LinkCode,
            label: "a reference definition into code",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(
                    &path,
                    "### D-1: choice\n",
                    "### D-1: choice\n\n[code]: ../../../crates/x/src/lib.rs\n",
                );
            },
        },
        Mutant {
            rule: Rule::Link,
            label: "a reference definition to nothing",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(
                    &path,
                    "### D-1: choice\n",
                    "### D-1: choice\n\n[gone]: gone.md\n",
                );
            },
        },
        Mutant {
            rule: Rule::LinkCode,
            label: "a scheme text inside a relative link",
            apply: |f| link_from_design(f, "../../../crates/x/src/lib.rs?a=://"),
        },
        Mutant {
            rule: Rule::LinkCode,
            label: "a title parenthesis hides the next link",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(
                    &path,
                    "[goal](index.md#1-goal--non-goals)",
                    "[a](index.md \"(\") [b](../../../crates/x/src/lib.rs)",
                );
            },
        },
        Mutant {
            rule: Rule::Anchor,
            label: "an anchor into a file no rule can read",
            apply: |f| {
                f.write_bytes("docs/bin.md", b"\xff");
                let path = format!("{DIR}/design.md");
                let link = "[goal](../../bin.md#section)";
                f.replace(&path, "[goal](index.md#1-goal--non-goals)", link);
            },
        },
        Mutant {
            rule: Rule::PartTable,
            label: "a part table without a delimiter row",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(&path, "|---|---|---|\n", "|x|\n");
            },
        },
        Mutant {
            rule: Rule::PartTable,
            label: "a second link behind an escaped pipe in a part cell",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                let cell = "| [design.md](design.md) \\| [index.md](index.md) |";
                f.replace(&path, "| [design.md](design.md) |", cell);
            },
        },
        Mutant {
            rule: Rule::CitePath,
            label: "a package path with an at sign is still a citation",
            apply: |f| cite_in_design(f, "`crates/@scope/gone.rs:3`"),
        },
    ]
}

/// Puts one code citation into the clean design part, in place of the pin citation.
fn cite_in_design(fixture: &Fixture, citation: &str) {
    let path = format!("{DIR}/design.md");
    fixture.replace(&path, &format!("`{CITED_FILE}:10`"), citation);
}

/// Points the clean design part's one relative link at `target`.
fn link_from_design(fixture: &Fixture, target: &str) {
    let path = format!("{DIR}/design.md");
    let link = format!("[goal]({target})");
    fixture.replace(&path, "[goal](index.md#1-goal--non-goals)", &link);
}

const ALL_RULES: [Rule; 43] = [
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
    Rule::Comment,
    Rule::Collision,
    Rule::Unreadable,
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
    assert_eq!(
        report.ratchet,
        format!("no baseline at comparison base {BASE}")
    );
    assert_eq!(report.absent_markdown, 0);
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
/// The three slots `migrate` leaves unfilled, as the row prints them.
const MIGRATED_CELL: &str =
    "migrated:AC-1.At-risk precondition,AC-1.Expected RED,AC-1.Single fault";

/// A sealed flat spec, now a directory, with three unfilled control fields of one AC.
fn migrated_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    migrate(&mut fixture);
    fixture
}

fn migrate(fixture: &mut Fixture) {
    fixture.rows = 3;
    fixture
        .sealed
        .insert(MIGRATED_ROW.to_owned(), "# flat\n".to_owned());
    fixture.replace(
        BASELINE,
        BIG,
        &format!("{MIGRATED_ROW}\t-\t-\t-\t{MIGRATED_CELL}\n{BIG}"),
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
        "one field filled, slot not removed",
    );
    fixture.replace(BASELINE, ",AC-1.Single fault", "");
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
        &format!("{SMALL}\t-\t-\t-\tmigrated:AC-1.Expected RED"),
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn a_migrated_row_with_no_spec_left_is_stale() {
    let fixture = migrated_fixture();
    fs::remove_dir_all(fixture.root.join(DIR)).expect("fixture remove");
    assert_only(&fixture, Rule::BaselineStale, "migrated spec deleted");
}

/// Only a `migrated:` row follows its spec into a directory.
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

/// The printed form is the only form, so a row cannot hide a slot in spelling.
#[test]
fn a_migrated_cell_names_sorted_unique_slots() {
    for cell in [
        "migrated:3",
        "migrated:0",
        "migrated:",
        "migrated",
        "migrated:AC-1.Nonsense",
        "migrated:x.Then",
        "migrated:AC-1x.Then",
        "migrated:AC-1.Then,AC-1.Given",
        "migrated:AC-1.Then,AC-1.Then",
        "migrated:AC-1.Then,",
    ] {
        let fixture = migrated_fixture();
        fixture.replace(BASELINE, MIGRATED_CELL, cell);
        assert!(fixture.rules().contains("BASELINE-FORMAT"), "{cell}");
    }
    assert!(is_slot("AC-1.Then") && is_slot("T-2.Blocked by") && is_slot("E-3.n"));
    assert!(is_slot("B-4.Check") && is_slot(HEADER_OWNER_SLOT));
}

/// The real row grew to a list, and a list stays a fixed point of its own parser.
#[test]
fn a_migrated_cell_round_trips() {
    let parsed = Structure::parse(MIGRATED_CELL).expect("cell parses");
    assert_eq!(parsed.cell(), MIGRATED_CELL);
}

/// A flat shadow of a healthy migrated directory: both forms would share one row.
#[test]
fn a_directory_and_its_flat_shadow_collide() {
    let fixture = migrated_fixture();
    let shadow = format!("{DIR}.md");
    let text = single_spec()
        .replace(
            "**Single fault:** the rule is removed",
            "**Single fault:** UNKNOWN",
        )
        .replace(
            "**At-risk precondition:** the state holds",
            "**At-risk precondition:** UNKNOWN",
        )
        .replace(
            "**Expected RED:** the test fails",
            "**Expected RED:** UNKNOWN",
        );
    fixture.write(&shadow, &text);
    assert_only(&fixture, Rule::Collision, "same-stem flat shadow");
    fixture.remove(&shadow);
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

/// Exactly a blocker's `Owner`: the neighbouring fields stay refused by the mutants above.
#[test]
fn unknown_is_allowed_only_for_a_blocker_owner() {
    assert!(allows_unknown("B", "Owner"));
    for (kind, label) in [
        ("B", "Impact"),
        ("B", "Check"),
        ("B", "Status"),
        ("E", "Run"),
        ("E", "Limits"),
        ("AC", "Owner"),
        ("T", "Blocked by"),
    ] {
        assert!(!allows_unknown(kind, label), "{kind} {label}");
    }
}

#[test]
fn a_nested_real_value_passes_every_empty_field() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/acceptance.md");
    fixture.replace(
        &path,
        "- **Then:** a result\n",
        "- **Then:**\n  - a nested result\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "- **Then:** a result\n",
        "- **Then:**\n  ```text\n  a fenced result\n  ```\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let fixture = Fixture::new();
    let delivery = format!("{DIR}/delivery.md");
    fixture.replace(
        &delivery,
        "- **Blocked by:** B-1\n",
        "- **Blocked by:**\n  - B-1\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

/// CommonMark reads `-` under a paragraph as a setext underline, so the field line turns
/// into a heading. It stays RED, through the field rule and the section rule.
#[test]
fn a_bare_dash_under_an_empty_field_is_red() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/acceptance.md");
    fixture.replace(&path, "- **Then:** a result\n", "- **Then:**\n  -\n");
    assert!(
        fixture.rules().contains("AC-FIELDS"),
        "{:?}",
        fixture.rules()
    );
}

/// `Then` is empty when the next line is a sibling, whatever its indent.
#[test]
fn a_sibling_field_is_not_a_nested_value() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/acceptance.md");
    fixture.replace(
        &path,
        "- **Then:** a result\n- **Negative control:**\n",
        "- **Then:**\n- **Negative control:**\n",
    );
    assert_only(&fixture, Rule::AcFields, "next line is a sibling");
}

/// A bytes row at 60,578 lands; the seal keeps 60,580. The schedule the review traced.
#[test]
fn a_shrink_stays_shrunk_across_pull_requests() {
    const SEALED: usize = 60_580;
    const LANDED: usize = 60_578;
    fn sized(bytes: usize) -> String {
        let line = format!("{}\n", "y".repeat(MAX_LINE_CHARS - 1));
        let mut text = line.repeat(bytes / line.len());
        text.push_str(&"z".repeat(bytes - text.len()));
        text
    }
    fn row(bytes: usize) -> String {
        format!("{BIG}\t-\t{bytes}\t-\texempt\n{SMALL}\t-\t-\t-\texempt\n")
    }
    let mut fixture = Fixture::new();
    fixture.sealed.insert(BIG.to_owned(), sized(SEALED));
    fixture.write(BIG, &sized(LANDED));
    fixture.write(BASELINE, &format!("{BASELINE_HEADER}\n{}", row(LANDED)));
    fixture.previous = Ok(Some(format!("{BASELINE_HEADER}\n{}", row(SEALED))));
    assert!(
        fixture.rules().is_empty(),
        "first shrink: {:?}",
        fixture.rules()
    );

    fixture.commit_baseline();
    fixture.write(BIG, &sized(SEALED));
    fixture.write(BASELINE, &format!("{BASELINE_HEADER}\n{}", row(SEALED)));
    assert_only(&fixture, Rule::BaselineGrew, "regrown to the seal");

    fixture.write(BIG, &sized(LANDED - 2));
    fixture.write(BASELINE, &format!("{BASELINE_HEADER}\n{}", row(LANDED - 2)));
    assert!(
        fixture.rules().is_empty(),
        "second shrink: {:?}",
        fixture.rules()
    );
}

/// The committed row for the file the review named has already shrunk under its seal.
#[test]
fn the_committed_gh1095_row_is_below_its_seal() {
    let root = env::current_dir().expect("cwd");
    let history = GitHistory {
        root: &root,
        base_ref: "HEAD".to_owned(),
    };
    let path = "docs/specs/gh1095-request-error-codes.md";
    let text = history.text_at(BASELINE_SEAL, path).expect("seal holds it");
    let at_seal = measure(&Doc::parse(path, &text)).bytes;
    let committed = fs::read_to_string(root.join(SPEC_ROOT).join(BASELINE_FILE))
        .expect("run from the repository root");
    let mut report = Report::default();
    let rows = parse_baseline(BASELINE, &committed, &mut report);
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    let bytes = rows[path].bytes.expect("over the byte cap");
    assert!(bytes < at_seal, "{bytes} at head, {at_seal} at the seal");
}

#[test]
fn a_ratchet_with_no_base_fails_closed() {
    let mut fixture = Fixture::new();
    fixture.previous = Err("SPEC-STRUCTURE: no base".to_owned());
    let error = fixture.try_run().err().expect("a missing base is no pass");
    assert!(error.contains("no base"), "{error}");
}

#[test]
fn a_base_baseline_that_does_not_parse_fails_closed() {
    let mut fixture = Fixture::new();
    fixture.previous = Ok(Some("# wrong header\n".to_owned()));
    let error = fixture.try_run().err().expect("an unread base is no pass");
    assert!(error.contains("does not parse"), "{error}");
}

#[test]
fn a_committed_baseline_reports_its_comparison_base() {
    let mut fixture = Fixture::new();
    fixture.commit_baseline();
    let report = fixture.run();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.ratchet, format!("comparison base {BASE}"));
}

/// Every lawful narrowing passes, so the ratchet does not freeze a drain.
#[test]
fn a_structure_cell_only_narrows() {
    let slots =
        |names: &[&str]| Structure::Migrated(names.iter().map(|name| (*name).to_owned()).collect());
    let (wide, narrow) = (slots(&["AC-1.Then", "AC-2.Then"]), slots(&["AC-1.Then"]));
    assert!(!Structure::Conforms.loosens(&Structure::Exempt));
    assert!(!Structure::Conforms.loosens(&wide));
    assert!(!wide.loosens(&Structure::Exempt));
    assert!(!narrow.loosens(&wide));
    assert!(!wide.loosens(&wide));
    assert!(!Structure::Exempt.loosens(&Structure::Exempt));
    assert!(!Structure::Conforms.loosens(&Structure::Conforms));
    assert!(wide.loosens(&narrow));
    assert!(wide.loosens(&Structure::Conforms));
    assert!(Structure::Exempt.loosens(&wide));
    assert!(Structure::Exempt.loosens(&Structure::Conforms));
}

/// Same-source controls: a direct code link is RED, a real in-lane file stays GREEN, and a
/// real symlink is RED without the index's help.
#[test]
fn a_real_symlink_cannot_carry_a_link_out_of_the_lane() {
    #[cfg(unix)]
    fn link(target: &Path, at: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, at)
    }
    #[cfg(windows)]
    fn link(target: &Path, at: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(target, at)
    }

    let fixture = Fixture::new();
    link_from_design(&fixture, "../../../crates/x/src/lib.rs");
    assert_only(&fixture, Rule::LinkCode, "direct code link");

    let fixture = Fixture::new();
    fixture.write("docs/real.md", "# real\n");
    link_from_design(&fixture, "../../real.md");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let fixture = Fixture::new();
    fixture.write("crates/x/src/lib.rs", "line\n");
    fs::create_dir_all(fixture.root.join("docs")).expect("fixture dir");
    let target = fixture.root.join("crates/x/src/lib.rs");
    match link(&target, &fixture.root.join("docs/alias.md")) {
        // Creating a symlink is a privilege on Windows; hosted Linux always proves this arm.
        Err(error) if cfg!(windows) => eprintln!("symlink arm not exercised: {error}"),
        created => {
            created.expect("fixture symlink");
            link_from_design(&fixture, "../../alias.md");
            assert_only(&fixture, Rule::LinkCode, "symlink alias");
        }
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
    fixture.track("docs/other.txt", b"## Acceptance criteria\n\xff");
    fixture.track(
        "docs/code.md",
        b"# Code\n\n    ## Acceptance criteria\n\n<!--\n## Acceptance criteria\n-->\n",
    );
    let report = fixture.run();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(
        report.absent_markdown, 1,
        "a skip is counted, never implied"
    );
}

/// The review's control: the same text is OUTSIDE when valid and UNREADABLE when not.
#[test]
fn an_unreadable_census_input_is_named_and_a_valid_one_is_classified() {
    let mut fixture = Fixture::new();
    fixture.track("crates/x/n.md", b"# Spec\n## Acceptance criteria\n");
    assert_only(&fixture, Rule::Outside, "valid control");
    fixture.write_bytes("crates/x/n.md", b"# Spec\n## Acceptance criteria\n\xff");
    assert_only(&fixture, Rule::Unreadable, "one invalid byte");
    fixture.write_bytes("crates/x/n.md", b"# Notes\n");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn every_rendered_markdown_extension_is_censused() {
    for path in ["a.md", "b/c.MD", "d.markdown", "e.Mkd", "f.mdx", "g.mdown"] {
        assert!(is_markdown_path(path), "{path}");
    }
    for path in ["a.txt", "md", "b.rs", "c.md.txt"] {
        assert!(!is_markdown_path(path), "{path}");
    }
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

/// A comment opener in code is code, and a closed comment hides only itself.
#[test]
fn a_comment_marker_in_code_opens_nothing() {
    for text in [
        "### D-1: choice\n\n`<!--` and ``<!-- x`` stay code\n\nAC-1",
        "### D-1: choice\n\n```text\n<!-- fenced\n```\n\nAC-1",
        "### D-1: choice\n\n~~~\n<!--\n~~~\n\nAC-1",
        "### D-1: choice\n\n    <!-- indented code\n\nAC-1",
        "### D-1: choice\n\nAC-1 <!-- B-99 --> and <!-- AC-98 --> AC-1\n\nAC-1",
        "### D-1: choice\n\n<!-- B-99 -->\n\n<!--\nB-98\n-->\n\nAC-1",
        "### D-1: choice\n\n\\<!-- not a comment B-1\n\nAC-1",
    ] {
        let fixture = Fixture::new();
        let path = format!("{DIR}/design.md");
        fixture.replace(&path, "### D-1: choice\n\nAC-1", text);
        assert!(fixture.rules().is_empty(), "{text}: {:?}", fixture.rules());
    }
}

/// A list item's continuation is not indented code, however deep.
#[test]
fn list_content_stays_visible_at_any_indent() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/acceptance.md");
    fixture.replace(
        &path,
        "  - **Single fault:** the rule is removed",
        "    - **Single fault:** the rule is removed",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());

    let doc = Doc::parse(
        "x.md",
        "- item\n\n    continued\n\n1. one\n2. two\n\n   text\n\n        code\n",
    );
    assert_eq!(doc.visible[2].as_deref(), Some("    continued"));
    assert_eq!(doc.visible[7].as_deref(), Some("   text"));
    assert_eq!(doc.visible[9], None, "four columns past the item");
}

/// Facts of the one lexer, each from the CommonMark 0.31.2 examples.
#[test]
fn the_lexer_follows_commonmark_blocks() {
    let doc = |text: &str| Doc::parse("x.md", text);
    let heading_levels = |text: &str| -> Vec<(usize, String)> {
        doc(text)
            .headings()
            .iter()
            .map(|heading| (heading.level, heading.text.clone()))
            .collect()
    };
    let one = |level: usize, title: &str| vec![(level, title.to_owned())];
    assert_eq!(heading_levels("   ## a"), one(2, "a"));
    assert_eq!(heading_levels("##\ta"), one(2, "a"));
    assert_eq!(heading_levels("## a ##"), one(2, "a"));
    assert_eq!(heading_levels("## a#"), one(2, "a#"));
    assert_eq!(heading_levels("## a \\#"), one(2, "a \\#"));
    assert_eq!(heading_levels("###"), one(3, ""));
    assert_eq!(heading_levels("## ##"), one(2, ""));
    assert_eq!(heading_levels("    ## a"), vec![], "four spaces is code");
    assert_eq!(heading_levels("#a"), vec![], "no delimiter");
    assert_eq!(heading_levels("####### a"), vec![], "seven hashes");
    assert_eq!(heading_levels("\\## a"), vec![], "escaped");
    assert_eq!(heading_levels("a\n===\n"), one(1, "a"));
    assert_eq!(heading_levels("a\n---\n"), one(2, "a"));
    assert_eq!(heading_levels("a\nb\n---\n"), one(2, "a b"));
    assert_eq!(heading_levels("a\n\n---\n"), vec![], "a thematic break");
    assert_eq!(
        heading_levels("- a\n---\n"),
        vec![],
        "the underline ends the list"
    );
    assert_eq!(heading_levels("- a\n  ---\n"), one(2, "a"));
    assert_eq!(heading_levels("- ## a"), one(2, "a"));
    assert_eq!(
        heading_levels("a\n    ## b"),
        vec![],
        "paragraph continuation"
    );
    assert_eq!(
        heading_levels("<div>\n## a\n</div>\n"),
        vec![],
        "HTML block"
    );
    assert_eq!(
        heading_levels("<div>\n\n## a\n"),
        one(2, "a"),
        "ended by a blank"
    );
    assert_eq!(
        heading_levels("a\n<span>\n## b"),
        one(2, "b"),
        "lone tag in a paragraph"
    );
    assert_eq!(heading_levels("```a```\n## b"), one(2, "b"), "inline code");
    assert_eq!(
        heading_levels("~~~ a`b\n## c\n~~~\n"),
        vec![],
        "a tilde info string"
    );
    assert_eq!(
        heading_levels("````\n```\n## a\n```\n````\n"),
        vec![],
        "nested fences"
    );
    assert_eq!(
        heading_levels("- ```\n  ## a\n  ```\n"),
        vec![],
        "a fence in a list"
    );
    assert_eq!(
        heading_levels("- ```\n  a\n## b"),
        one(2, "b"),
        "a fence ends with its item"
    );
    assert_eq!(
        heading_levels("    ```\n## a"),
        one(2, "a"),
        "indented code is no fence"
    );
    assert!(doc("- ```\n  a\n## b").unclosed_fence.is_none());
    assert!(doc("```\n").unclosed_fence.is_some());
    assert_eq!(
        heading_levels("- <div>\n## a\n"),
        one(2, "a"),
        "ended by its item"
    );
    assert_eq!(
        heading_levels("> a\n---\n"),
        vec![],
        "a quote ends at a thematic break"
    );
    assert_eq!(
        heading_levels("- a\n> b\n---\n"),
        vec![],
        "a quote ends the list"
    );
    assert_eq!(heading_levels("## a"), one(2, "a"));
    assert_eq!(heading_levels(" ## a"), one(2, "a"));
    assert_eq!(heading_levels("  ## a"), one(2, "a"));
    assert_eq!(atx_heading("   ## a"), Some((2, "a".to_owned())));
    assert_eq!(atx_heading("    ## a"), None);
    assert!(doc("<!--\n").unclosed_raw.is_some());
    assert!(
        doc("<!-->\n").unclosed_raw.is_none(),
        "CommonMark 0.31 short comment"
    );
    assert!(doc("<!---->\n").unclosed_raw.is_none());
    assert!(doc("<?x\n?>\n").unclosed_raw.is_none());
    assert!(doc("<?x\n").unclosed_raw.is_some());
    assert!(doc("<![CDATA[x\n").unclosed_raw.is_some());
    assert!(doc("<!DOCTYPE x\n").unclosed_raw.is_some());
    assert!(doc("<script>\n\n</SCRIPT>\n").unclosed_raw.is_none());
    assert!(
        doc("<https://example.com>\n").visible[0].is_some(),
        "an autolink"
    );
    assert!(doc("<id>-<slug> text\n").visible[0].is_some(), "prose");
    assert!(doc("<state>\n").visible[0].is_none(), "a lone tag");
}

#[test]
fn only_a_real_list_item_interrupts_a_paragraph() {
    assert_eq!(list_marker("2. b", true), None);
    assert_eq!(list_marker("1. b", true), Some(2));
    assert_eq!(list_marker("-", true), None);
    assert_eq!(list_marker("- b", true), Some(1));
    assert_eq!(list_marker("2. b", false), Some(2));
    assert_eq!(list_marker("-", false), Some(1));
    assert_eq!(list_marker("-b", false), None);
    assert_eq!(list_marker("1234567890. b", false), None, "ten digits");
    let doc = Doc::parse("x.md", "a\n2. b\n    c\n\n    d\n");
    assert_eq!(doc.visible[2].as_deref(), Some("    c"));
    assert_eq!(doc.visible[4], None, "no list opened, so this is code");
}

/// Whether an item stays open decides whether a deep line is its content or code.
#[test]
fn an_open_list_item_decides_what_indented_text_is() {
    let last = |text: &str| Doc::parse("x.md", text).visible.last().cloned().flatten();
    let content = Some("    c".to_owned());
    assert_eq!(last("- a\n\n    c\n"), content, "item content");
    assert_eq!(
        last("- a\nb\n\n    c\n"),
        content,
        "a lazy line keeps the item"
    );
    assert_eq!(last("- a\n> b\n\n    c\n"), None, "a quote closes the item");
    assert_eq!(
        last("- a\n## b\n\n    c\n"),
        None,
        "a heading closes the item"
    );
    let deep = Some("     b".to_owned());
    assert_eq!(
        last("2. a\n\n     b\n"),
        deep,
        "no paragraph, so `2.` opens a list"
    );
    assert_eq!(
        last("-\n\n     b\n"),
        Some("     b".to_owned()),
        "an empty item"
    );
}

#[test]
fn inline_comments_leave_the_line_around_them() {
    assert_eq!(
        without_comments("a <!-- b --> c"),
        ("a  c".to_owned(), false)
    );
    assert_eq!(without_comments("a <!-- b"), ("a ".to_owned(), true));
    assert_eq!(without_comments("a <!--> b"), ("a  b".to_owned(), false));
    assert_eq!(without_comments("`<!--` a"), ("`<!--` a".to_owned(), false));
    assert_eq!(
        without_comments("a <!-- ` --> b `"),
        ("a  b `".to_owned(), false)
    );
    assert_eq!(
        without_comments("<!--a--> <!--b-->"),
        (" ".to_owned(), false)
    );
    assert_eq!(
        without_comments(r"\<!-- a -->"),
        (r"\<!-- a -->".to_owned(), false)
    );
    assert_eq!(without_comments(r"\\<!-- a -->"), (r"\\".to_owned(), false));
}

#[test]
fn a_bom_crlf_and_bare_cr_are_invisible_to_every_rule() {
    let fixture = Fixture::new();
    fixture.write(SINGLE, &format!("\u{feff}{}", single_spec()));
    assert!(fixture.rules().is_empty(), "BOM: {:?}", fixture.rules());

    let fixture = Fixture::new();
    fixture.write(SINGLE, &single_spec().replace('\n', "\r\n"));
    assert!(fixture.rules().is_empty(), "CRLF: {:?}", fixture.rules());

    let fixture = Fixture::new();
    fixture.write(SINGLE, &single_spec().replace('\n', "\r"));
    assert!(fixture.rules().is_empty(), "CR: {:?}", fixture.rules());
}

/// A closing run and setext-free spelling stay definitions, so the lexer adds no red.
#[test]
fn a_closing_run_is_not_part_of_a_definition() {
    let fixture = Fixture::new();
    fixture.replace(SINGLE, "### AC-1: rule", "### AC-1: rule ###");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn a_table_cell_may_hold_an_escaped_pipe() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/index.md");
    fixture.replace(&path, "| D-1 and E-1 |", r"| D-1 \| E-1 |");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    assert_eq!(table_cells(r"| a \| b | c |"), [r"a \| b", "c"]);
    assert_eq!(table_cells(r"| a \\| b |"), [r"a \\", "b"]);
    assert!(is_table_delimiter("|---|:--|-:|"));
    assert!(is_table_delimiter("| --- | :-: | --- |"));
    assert!(!is_table_delimiter("|---|---|"));
    assert!(!is_table_delimiter("|x|y|z|"));
}

#[test]
fn links_parse_as_commonmark_destinations() {
    let targets = |line: &str| link_targets(line);
    assert_eq!(targets(r#"[a](b.md "t")"#), ["b.md"]);
    assert_eq!(targets(r#"[a](b.md "t)") [c](d.md)"#), ["b.md", "d.md"]);
    assert_eq!(targets("[a](<b c.md>)"), ["b c.md"]);
    assert_eq!(targets("[a](b(c).md)"), ["b(c).md"]);
    assert_eq!(targets("![a](b.png)"), ["b.png"]);
    assert_eq!(targets("[a]: b.md"), ["b.md"]);
    assert_eq!(targets("  [a]: <b c.md> 'title'"), ["b c.md"]);
    assert!(targets("[^1]: a footnote").is_empty());
    assert!(targets("`[a](b.md)`").is_empty());
    for url in ["https://x.y/z", "git+ssh://x/y", "mailto:a@b.c"] {
        assert!(is_external(url), "{url}");
    }
    for path in ["a.md", "../x.rs?y=://", "a b://c", "://x", "1a://x"] {
        assert!(!is_external(path), "{path}");
    }
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
    let cited = |path, lines, sha| {
        Some(Citation {
            path,
            lines: Some(lines),
            sha,
        })
    };
    assert_eq!(parse_citation("a/b.rs:12"), cited("a/b.rs", (12, 12), None));
    assert_eq!(parse_citation("a/b.rs:3-9"), cited("a/b.rs", (3, 9), None));
    assert_eq!(
        parse_citation("a/b.rs:3@abc"),
        cited("a/b.rs", (3, 3), Some("abc"))
    );
    assert_eq!(parse_citation("127.0.0.1:8080"), None);
    assert_eq!(parse_citation("http://x/y:80"), None);
    assert_eq!(parse_citation("a/b.rs"), None);
    assert_eq!(
        parse_citation("a/b.rs:12:5"),
        cited("a/b.rs", (12, 12), None)
    );
    assert_eq!(
        parse_citation("a/b.rs:3-9:5"),
        cited("a/b.rs", (3, 9), None)
    );
    assert_eq!(
        parse_citation(".agents/ci.md:3"),
        cited(".agents/ci.md", (3, 3), None)
    );
    assert_eq!(
        parse_citation("packages/@keld/a.ts:3"),
        cited("packages/@keld/a.ts", (3, 3), None)
    );
    assert_eq!(
        parse_citation("packages/@keld/a.ts:3@abc"),
        cited("packages/@keld/a.ts", (3, 3), Some("abc"))
    );
    assert_eq!(parse_citation("ghcr.io/org/image:11"), None, "image tag");
    assert_eq!(parse_citation("ghcr.io/org/image.v1:11"), None, "domain");
    assert_eq!(parse_citation("org/image:11"), None, "no file extension");
    assert_eq!(parse_citation("C:/x/y.rs:3"), None, "absolute Windows path");
    assert_eq!(parse_citation("/usr/lib/y.so:3"), None, "absolute path");
    assert_eq!(parse_citation("a/b.rs:x"), None, "no line part");
}

/// A shape no range can parse stays a citation, so `CITE-LINE` refuses it.
#[test]
fn a_malformed_line_part_is_refused_not_ignored() {
    for span in [
        "a/b.rs:1-2-3",
        "a/b.rs:5-",
        "a/b.rs:-5",
        "a/b.rs:99999999999999999999999",
    ] {
        let citation = parse_citation(span).expect(span);
        assert_eq!(citation.lines, None, "{span}");
        assert!(!range_exists(citation.lines, 10), "{span}");
    }
}

#[test]
fn a_range_exists_only_with_both_ends_in_order() {
    for lines in [(1, 2), (1, 10), (10, 10), (3, 3)] {
        assert!(range_exists(Some(lines), 10), "{lines:?}");
    }
    for lines in [
        (11, 10),
        (999, 2),
        (0, 1),
        (9, 3),
        (0, 0),
        (1, 11),
        (11, 11),
    ] {
        assert!(!range_exists(Some(lines), 10), "{lines:?}");
    }
}

#[test]
fn a_citation_verifies_in_any_span_width() {
    for span in [
        format!("`{CITED_FILE}:1-2`"),
        format!("``{CITED_FILE}:1-2``"),
        format!("`` {CITED_FILE}:1-2 ``"),
        format!("`` `{CITED_FILE}:1-2` ``"),
    ] {
        let fixture = Fixture::new();
        cite_in_design(&fixture, &span);
        let report = fixture.run();
        assert!(report.findings.is_empty(), "{span}: {:?}", report.findings);
        assert_eq!(report.citations_verified, 2, "{span}");
    }
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
        base_ref: head.clone(),
    };
    assert!(history.has_commit(&head));
    assert!(history.is_reachable(&head));
    assert!(history.has_commit(&unreachable));
    assert!(!history.is_reachable(&unreachable), "a dropped commit");
    assert!(!history.is_reachable(ABSENT_SHA));
    let tracked = history.tracked_files().expect("index reads");
    assert!(tracked.iter().any(|file| file.path == BIG), "{tracked:?}");
    assert!(tracked.iter().all(|file| !file.symlink), "{tracked:?}");
    let blob = git(&["rev-parse", &format!("HEAD:{BIG}")]);
    let entry = format!("120000,{blob},docs/alias.md");
    git(&["update-index", "--add", "--cacheinfo", &entry]);
    let tracked = history.tracked_files().expect("index reads");
    let alias = tracked.iter().find(|file| file.path == "docs/alias.md");
    assert!(alias.is_some_and(|file| file.symlink), "{tracked:?}");
    git(&["rm", "-q", "--cached", "docs/alias.md"]);
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

    let previous = history.previous_baseline(BASELINE).expect("base resolves");
    assert_eq!(previous.commit, head);
    let committed = previous.text.expect("the head holds the baseline");
    assert_eq!(committed.replace("\r\n", "\n"), fixture.read(BASELINE));
    let absent = history.previous_baseline("docs/specs/none.tsv");
    assert!(absent.expect("base resolves").text.is_none());

    for base_ref in [
        "origin/none",
        "",
        "0000000000000000000000000000000000000000",
        ABSENT_SHA,
    ] {
        let history = GitHistory {
            root: &fixture.root,
            base_ref: base_ref.to_owned(),
        };
        let error = history
            .previous_baseline(BASELINE)
            .err()
            .expect("a missing base is no pass");
        assert!(
            error.contains("git fetch origin main") && error.contains(BASE_ENV),
            "{base_ref}: {error}"
        );
        // The seal resolves here, so only the missing base can refuse this run.
        let config = Config {
            rows: 2,
            seal: &head,
        };
        let refused = check_tree(&fixture.root, &config, &history);
        let refused = refused.err().expect("a missing base must not pass");
        assert!(refused.contains("comparison base"), "{base_ref}: {refused}");
    }
    let unrelated = git(&[
        &identity[..],
        &["commit-tree", "-m", "z", &format!("{head}^{{tree}}")],
    ]
    .concat());
    let history = GitHistory {
        root: &fixture.root,
        base_ref: unrelated,
    };
    let error = history
        .previous_baseline(BASELINE)
        .err()
        .expect("no history");
    assert!(error.contains("no merge base"), "{error}");
}

fn printed_rows(fixture: &Fixture) -> String {
    baseline_rows_at(&fixture.root, &fixture.history()).expect("rows print")
}

/// `baseline-rows` is the paste source for a drain: what it prints must be what `check`
/// accepts, for a directory spec and a flat spec that both keep `UNKNOWN` fields.
#[test]
fn baseline_rows_prints_what_check_accepts() {
    let fixture = migrated_fixture();
    let text = single_spec().replace(
        "**Expected RED:** the test fails",
        "**Expected RED:** UNKNOWN",
    );
    fixture.write(SMALL, &text);
    let printed = printed_rows(&fixture);
    let migrated = format!("{MIGRATED_ROW}\t-\t-\t-\t{MIGRATED_CELL}");
    let flat = format!("{SMALL}\t-\t-\t-\tmigrated:AC-1.Expected RED");
    assert!(printed.lines().any(|row| row == migrated), "{printed}");
    assert!(printed.lines().any(|row| row == flat), "{printed}");
    fixture.write(BASELINE, &printed);
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    assert_eq!(printed_rows(&fixture), printed, "printing is a fixed point");
}

#[test]
fn baseline_rows_reproduces_the_committed_baseline() {
    let root = env::current_dir().expect("cwd");
    let committed = fs::read_to_string(root.join(SPEC_ROOT).join(BASELINE_FILE))
        .expect("run from the repository root")
        .replace("\r\n", "\n");
    let history = GitHistory {
        root: &root,
        base_ref: "HEAD".to_owned(),
    };
    assert_eq!(
        baseline_rows_at(&root, &history).expect("rows print"),
        committed
    );
}
