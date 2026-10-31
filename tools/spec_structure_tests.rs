//! Each rule has one single-fault fixture mutant.
//!
//! The clean fixture passes first, so a mutant's RED comes from its fault alone. Every
//! mutant must trip exactly the rule it names: if a second rule also fired, deleting
//! the first rule could leave the fixture red and hide that deletion.

use super::rendered::{MAX_LIST_DEPTH, is_thematic, split_cells};
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
    /// A sealed path Git lists and cannot show.
    lost: Option<String>,
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
            let lost = self.lost.as_deref() == Some(path);
            return self.sealed.get(path).filter(|_| !lost).cloned();
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
    lost: Option<String>,
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
            lost: None,
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
            lost: self.lost.clone(),
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

/// The value of `label` on the first line of `line`, read from its rendered view.
fn value_of(line: &str, label: &str, spelling: Spelling) -> Option<String> {
    let doc = Doc::parse("x.md", line);
    field_value(doc.line(0).field()?, label, spelling)
}

/// The rendered text of one line, or `None` when it shows nothing.
fn shown(doc: &Doc, index: usize) -> Option<String> {
    doc.line(index).prose().map(|line| line.text.clone())
}

/// Every line of `text` in the rendered view: `None` for one that shows nothing.
fn shown_lines(text: &str) -> Vec<Option<String>> {
    let doc = Doc::parse("x.md", text);
    (0..doc.line_count()).map(|at| shown(&doc, at)).collect()
}

/// The link targets of `text`, definitions included.
fn targets_of(text: &str) -> Vec<String> {
    let doc = Doc::parse("x.md", text);
    doc.targets()
        .iter()
        .map(|(_, target)| (*target).to_owned())
        .collect()
}

/// The first definition or footnote label of `text`.
fn label_of(text: &str) -> Option<String> {
    let doc = Doc::parse("x.md", text);
    doc.labels().first().map(|(_, label)| (*label).to_owned())
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
            rule: Rule::Entity,
            label: "a heading holds a reference no rule resolves",
            apply: |f| f.replace(SINGLE, "## 5.", "### A note&foo;\n\n## 5."),
        },
        Mutant {
            rule: Rule::Entity,
            label: "a label holds a reference no rule resolves",
            apply: |f| f.replace(SINGLE, "## 5.", "&foo;Pin: x\n\n## 5."),
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
            rule: Rule::AcFields,
            label: "a comment across a paragraph hides a field",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- **Then:** a result\n",
                    "text <!--\nThen: a result\n-->\n",
                );
            },
        },
        Mutant {
            rule: Rule::IdDangling,
            label: "a blank line ends a mid-line comment, so the text after it renders",
            apply: |f| f.replace(SINGLE, "none\n\n## 5.", "a <!--\n\nAC-99\n\n-->\n\n## 5."),
        },
        Mutant {
            rule: Rule::IdDangling,
            label: "a heading ends a mid-line comment, so the text after it renders",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "none\n\n## 5.",
                    "a <!--\n\n### D-1: x\n\nAC-99\n\n-->\n\n## 5.",
                );
            },
        },
        Mutant {
            rule: Rule::IdDangling,
            label: "a quote ends a comment it opened",
            apply: |f| f.replace(SINGLE, "none\n\n## 5.", "> <!--\n\nAC-99\n\n-->\n\n## 5."),
        },
        Mutant {
            rule: Rule::IdDangling,
            label: "a list item ends a comment it opened",
            apply: |f| f.replace(SINGLE, "none\n\n## 5.", "- <!--\nAC-99\n-->\n\n## 5."),
        },
        Mutant {
            rule: Rule::IdDangling,
            label: "a lazy line after a quote opens no HTML block",
            apply: |f| f.replace(SINGLE, "none\n\n## 5.", "> a\n<span>\nAC-99\n\n## 5."),
        },
        Mutant {
            rule: Rule::AcFields,
            label: "a nested definition lends its parent no field",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "### AC-1: rule\n",
                    "### AC-1: rule\n\n#### AC-2: nested\n",
                );
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a repeated field is empty",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- **Then:** a result\n",
                    "- **Then:** a result\n- **Then:**\n",
                );
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a repeated field holds UNKNOWN",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- **Then:** a result\n",
                    "- **Then:** a result\n- **Then:** UNKNOWN\n",
                );
            },
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "UNKNOWN in quotes",
            apply: |f| f.replace(SINGLE, "- **Then:** a result", "- **Then:** \"UNKNOWN\""),
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "UNKNOWN in brackets",
            apply: |f| f.replace(SINGLE, "- **Then:** a result", "- **Then:** [UNKNOWN]"),
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "TBD in brackets",
            apply: |f| f.replace(SINGLE, "- **Then:** a result", "- **Then:** [TBD]"),
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "TBD with a label",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- **Then:** a result",
                    "- **Then:** TBD: the owner decides",
                )
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a bare none as the negative control",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- **Negative control:**\n",
                    "- **Negative control:** none\n",
                )
            },
        },
        Mutant {
            rule: Rule::Unreadable,
            label: "a sealed path Git lists and cannot show",
            apply: |f| f.lost = Some(BIG.to_owned()),
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
        // Finding 4430984129: a field has one statement, and a spec has one header.
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a second valid Pin with the same value",
            apply: |f| second_pin(f, PIN),
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a second malformed Pin",
            apply: |f| second_pin(f, "1111111"),
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a second Pin this branch cannot reach",
            apply: |f| second_pin(f, ABSENT_SHA),
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "an unreachable Pin before the valid one",
            apply: |f| {
                let line = format!("- Pin: {PIN}\n");
                f.replace(SINGLE, &line, &format!("- Pin: {ABSENT_SHA}\n{line}"));
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a second conflicting Owner",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- Owner: owner\n",
                    "- Owner: owner\n- Owner: other\n",
                )
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a second Run in one evidence",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(
                    &path,
                    "- **Run:** one\n",
                    "- **Run:** one\n- **Run:** two\n",
                );
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a part states the Pin",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let owns = "Owns: decisions\n";
                f.replace(&path, owns, &format!("{owns}Pin: {ABSENT_SHA}\n"));
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a part states the status",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let owns = "Owns: decisions\n";
                f.replace(&path, owns, &format!("{owns}Status: done\n"));
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a section restates the Pin",
            apply: |f| f.replace(SINGLE, "## 5.", &format!("- Pin: {ABSENT_SHA}\n\n## 5.")),
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a part states its scope twice",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "Owns: decisions\n", "Owns: decisions\nOwns: all\n");
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a part states its index twice",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let line = "Owns: decisions\nPart of: [index](index.md)\n";
                f.replace(&path, "Owns: decisions\n", line);
            },
        },
        Mutant {
            rule: Rule::FieldDuplicate,
            label: "a section of a part restates the Pin",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let line = format!("## 5. Boundaries\n\n- Pin: {PIN}\n");
                f.replace(&path, "## 5. Boundaries\n", &line);
            },
        },
        // The same fault in a value that is one field.
        Mutant {
            rule: Rule::Fields,
            label: "a second Pin nested under the first",
            apply: |f| {
                let line = format!("- Pin: {PIN}\n");
                f.replace(SINGLE, &line, &format!("{line}  - {ABSENT_SHA}\n"));
            },
        },
        Mutant {
            rule: Rule::BlockerFields,
            label: "a second status nested under the first",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(
                    &path,
                    "- **Status:** open\n",
                    "- **Status:** open\n  - closed\n",
                );
            },
        },
        Mutant {
            rule: Rule::FieldPlaceholder,
            label: "a later nested statement holds TBD",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- **Then:** a result\n",
                    "- **Then:**\n  - a result\n  - TBD\n",
                )
            },
        },
        Mutant {
            rule: Rule::FieldUnknown,
            label: "a statement nested under a filled field holds UNKNOWN",
            apply: |f| {
                f.replace(
                    SINGLE,
                    "- **Then:** a result\n",
                    "- **Then:** a result\n  - UNKNOWN\n",
                )
            },
        },
        Mutant {
            rule: Rule::TaskFields,
            label: "a task is blocked by none and by an ID",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(
                    &path,
                    "- **Blocked by:** B-1\n",
                    "- **Blocked by:** none\n  - B-1\n",
                );
            },
        },
        Mutant {
            rule: Rule::TaskFields,
            label: "a task's second blocker is prose",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                let list = "- **Blocked by:**\n  - B-1\n  - later\n";
                f.replace(&path, "- **Blocked by:** B-1\n", list);
            },
        },
        // Same-class sweep: other places where one of two statements wins silently.
        Mutant {
            rule: Rule::Sections,
            label: "a section repeated across parts",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(
                    &path,
                    "## 6. Tasks",
                    "## 5. Boundaries\n\nmore\n\n## 6. Tasks",
                );
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "a section repeated in one part",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.write(
                    &path,
                    &format!("{}\n## 5. Boundaries\n\nmore\n", f.read(&path)),
                );
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "a section repeated in a single file",
            apply: |f| f.replace(SINGLE, "## 5.", "## 4. Design\n\nmore\n\n## 5."),
        },
        Mutant {
            rule: Rule::Sections,
            label: "a second parts heading in an index",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                f.replace(
                    &path,
                    "## Current blockers",
                    "## Parts\n\n## Current blockers",
                );
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "a second blockers heading in an index",
            apply: |f| {
                let path = format!("{DIR}/index.md");
                let extra = "- B-1\n\n## Current blockers\n\n- B-1\n";
                f.replace(&path, "- B-1\n", extra);
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "a second title in a single file",
            apply: |f| f.replace(SINGLE, "## 1. Goal", "# Spec: other\n\n## 1. Goal"),
        },
        Mutant {
            rule: Rule::Sections,
            label: "a second title in a part",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                f.replace(&path, "## 5. Boundaries", "# Other\n\n## 5. Boundaries");
            },
        },
        Mutant {
            rule: Rule::Sections,
            label: "a template with a second copy source",
            apply: |f| {
                let text = f.read(TEMPLATE);
                f.write(TEMPLATE, &format!("{text}\n```markdown\n# x\n```\n"));
            },
        },
        Mutant {
            rule: Rule::Link,
            label: "a reference label defined twice with two targets",
            apply: |f| reference_pair(f, "[a]: index.md", "[a]: design.md"),
        },
        Mutant {
            rule: Rule::Link,
            label: "a reference label defined twice in another case and spacing",
            apply: |f| reference_pair(f, "[A b]: index.md", "[a  B]: design.md"),
        },
        Mutant {
            rule: Rule::Link,
            label: "a footnote defined twice",
            apply: |f| reference_pair(f, "[^1]: one", "[^1]: two"),
        },
        Mutant {
            rule: Rule::Link,
            label: "a landed exempt file still has one target for each label",
            apply: |f| f.write(SMALL, "# old\n\n[a]: old-big.md\n\n[a]: old-small.md\n"),
        },
        Mutant {
            rule: Rule::LinkCode,
            label: "a reference definition into code, its target on the next line",
            apply: |f| {
                let path = format!("{DIR}/design.md");
                let block = "### D-1: choice\n\n[code]:\n  ../../../crates/x/src/lib.rs\n";
                f.replace(&path, "### D-1: choice\n", block);
            },
        },
        Mutant {
            rule: Rule::BaselineFormat,
            label: "a row listed twice",
            apply: |f| {
                let row = format!("{SMALL}\t-\t-\t-\texempt\n");
                f.replace(BASELINE, &row, &format!("{row}{row}"));
            },
        },
        Mutant {
            rule: Rule::CiteSha,
            label: "a citation with two pins",
            apply: |f| cite_in_design(f, &format!("`{CITED_FILE}:1@{PIN}@{ABSENT_SHA}`")),
        },
        Mutant {
            rule: Rule::IdDuplicate,
            label: "an ID defined twice across parts",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(
                    &path,
                    "## 9. Perf impact",
                    "### AC-1: again\n\n## 9. Perf impact",
                );
            },
        },
        Mutant {
            rule: Rule::IdDuplicate,
            label: "a heading restates an ID in other words",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(&path, "### B-1: stop", "### Q-1 again\n\n### B-1: stop");
            },
        },
        Mutant {
            rule: Rule::IdDuplicate,
            label: "a heading restates an ID in emphasis",
            apply: |f| {
                let path = format!("{DIR}/delivery.md");
                f.replace(
                    &path,
                    "### B-1: stop",
                    "### **Q-1**: again\n\n### B-1: stop",
                );
            },
        },
    ]
}

/// Adds a second `Pin` after the valid one.
fn second_pin(fixture: &Fixture, value: &str) {
    let line = format!("- Pin: {PIN}\n");
    fixture.replace(SINGLE, &line, &format!("{line}- Pin: {value}\n"));
}

/// Adds two definitions to the clean design part.
fn reference_pair(fixture: &Fixture, first: &str, second: &str) {
    let path = format!("{DIR}/design.md");
    let pair = format!("### D-1: choice\n\n{first}\n\n{second}\n");
    fixture.replace(&path, "### D-1: choice\n", &pair);
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

const ALL_RULES: [Rule; 45] = [
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
    Rule::Entity,
    Rule::AcFields,
    Rule::TaskFields,
    Rule::BlockerFields,
    Rule::EvidenceFields,
    Rule::FieldUnknown,
    Rule::FieldPlaceholder,
    Rule::FieldDuplicate,
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
fn a_metric_has_one_spelling() {
    assert_eq!(parse_metric("600"), Some(Some(600)));
    assert_eq!(parse_metric("-"), Some(None));
    for text in ["0", "+600", "0600", "6e2", ""] {
        assert_eq!(parse_metric(text), None, "{text}");
    }
}

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

#[test]
fn a_field_may_use_any_list_marker_and_either_emphasis() {
    for line in [
        "- **Then:** x",
        "* **Then:** x",
        "+ **Then:** x",
        "1. **Then:** x",
        "- **Then**: x",
        "**Then**: x",
        "Then: x",
    ] {
        let value = value_of(line, "Then", Spelling::Exact);
        assert_eq!(value.as_deref(), Some("x"), "{line}");
    }
    assert_eq!(value_of("- Thenceforth: x", "Then", Spelling::Header), None);
    assert_eq!(value_of("- Then you: x", "Then", Spelling::Header), None);
    let doc = Doc::parse("x.md", "- **Then:**\n\n  a loose value\n- **When:** x\n");
    assert_eq!(field_statements(&doc, 0, 4, None), ["a loose value"]);
    let doc = Doc::parse("x.md", "- **Then:**\n\n- **When:** x\n");
    let none: [String; 0] = [];
    assert_eq!(
        field_statements(&doc, 0, 3, None),
        none,
        "a sibling after a blank line"
    );
}

/// One spelling for a reader is one label for the checker, or a variant hides a repeat.
#[test]
fn no_spelling_hides_a_second_field() {
    for spelled in [
        format!("- Pin: {ABSENT_SHA}"),
        format!("- pin: {ABSENT_SHA}"),
        format!("- PIN: {ABSENT_SHA}"),
        format!("- *Pin*: {ABSENT_SHA}"),
        format!("- *Pin:* {ABSENT_SHA}"),
        format!("- **Pin**: {ABSENT_SHA}"),
        format!("- **Pin:** {ABSENT_SHA}"),
        format!("- __Pin__: {ABSENT_SHA}"),
        format!("- `Pin`: {ABSENT_SHA}"),
        format!("- Pin : {ABSENT_SHA}"),
        format!("1. Pin: {ABSENT_SHA}"),
        format!("Pin: {ABSENT_SHA}"),
    ] {
        let fixture = Fixture::new();
        let line = format!("- Pin: {PIN}\n");
        fixture.replace(SINGLE, &line, &format!("{line}{spelled}\n"));
        assert_only(&fixture, Rule::FieldDuplicate, &spelled);
    }
    for line in ["pin: x", "*Pin*: x", "**Pin**: x", "`Pin`: x", "Pin : x"] {
        let value = value_of(line, "Pin", Spelling::Header);
        assert_eq!(value.as_deref(), Some("x"), "{line}");
    }
}

/// What a reader cannot tell from the label is the label, in a header and in a body.
#[test]
fn no_invisible_spelling_hides_a_second_field() {
    for spelled in [
        format!("- - Pin: {ABSENT_SHA}"),
        format!("1. - Pin: {ABSENT_SHA}"),
        format!("- `Pin:` {ABSENT_SHA}"),
        format!("- Pin\u{a0}: {ABSENT_SHA}"),
        format!("- Pi\u{200b}n: {ABSENT_SHA}"),
        format!("\u{200b}- Pin: {ABSENT_SHA}"),
        format!("- Pin\u{2060}: {ABSENT_SHA}"),
    ] {
        let fixture = Fixture::new();
        let line = format!("- Pin: {PIN}\n");
        fixture.replace(SINGLE, &line, &format!("{line}{spelled}\n"));
        assert_only(&fixture, Rule::FieldDuplicate, &spelled);
    }
    for spelled in [
        "**Expected  RED:** other",
        "**Expected** RED: other",
        "*Expected* *RED*: other",
        "EXPECTED RED: other",
    ] {
        let fixture = Fixture::new();
        let line = "  - **Expected RED:** the test fails\n";
        fixture.replace(SINGLE, line, &format!("{line}  - {spelled}\n"));
        assert_only(&fixture, Rule::FieldDuplicate, spelled);
    }
    let fixture = Fixture::new();
    let path = format!("{DIR}/delivery.md");
    fixture.replace(&path, "- **n:** 3\n", "- **n:** 3\n- **N:** 4\n");
    assert_only(
        &fixture,
        Rule::FieldDuplicate,
        "a label that opens in lower case",
    );
}

/// A variant can repeat a field, never state one: c42b0aed refused each of these alone.
#[test]
fn a_variant_alone_does_not_state_a_field() {
    for (line, variant) in [
        (format!("- Pin: {PIN}\n"), format!("- pin: {PIN}\n")),
        (
            "- Status: draft\n".to_owned(),
            "- STATUS: draft\n".to_owned(),
        ),
        ("- Owner: owner\n".to_owned(), "- owner: owner\n".to_owned()),
    ] {
        let fixture = Fixture::new();
        fixture.replace(SINGLE, &line, &variant);
        assert_only(&fixture, Rule::Fields, &variant);
    }
    let fixture = Fixture::new();
    fixture.replace(SINGLE, "- Owner: owner\n", "- owner:\n  - owner\n");
    assert_only(
        &fixture,
        Rule::Fields,
        "a variant lends a nested value nothing",
    );
    let fixture = Fixture::new();
    fixture.replace(SINGLE, "- **Given:** a state\n", "- **given:** a state\n");
    assert_only(&fixture, Rule::AcFields, "a lower-case AC field");
    let fixture = Fixture::new();
    let line = "- Tracker: GH-1\n";
    fixture.replace(SINGLE, line, "- Tracker: GH-2\n- tracker: GH-1\n");
    assert_only(
        &fixture,
        Rule::FieldDuplicate,
        "a repeat has no tracker to compare",
    );
    let fixture = Fixture::new();
    let path = format!("{DIR}/index.md");
    fixture.replace(&path, "- Tracker: GH-1\n", "- tracker: KEL-9\n");
    assert_only(&fixture, Rule::Fields, "a variant tracker reports once");
}

/// gh566 line 367 and gh527 line 67 hold these shapes: a lower-case word that opens a list
/// item or a wrapped line, then a colon.
#[test]
fn a_lower_case_label_in_a_body_is_prose() {
    let path = format!("{DIR}/delivery.md");
    let impact = "- **Impact:** blocks T-1\n";
    for prose in [
        "  - owner: @0monish, at this spec's approval\n",
        "  - status: see below\n",
        "  and its\n  check: the one mutation\n",
    ] {
        let fixture = Fixture::new();
        fixture.replace(&path, impact, &format!("{impact}{prose}"));
        assert!(fixture.rules().is_empty(), "{prose}: {:?}", fixture.rules());
    }
    let fixture = Fixture::new();
    fixture.replace(
        SINGLE,
        "## 5.",
        "- updated: true when a row moves\n- pin: the Bun version\n- tracker: the object\n\n## 5.",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "## 9. Perf impact\n\nnone\n",
        "## 9. Perf impact\n\n- pin: the Bun version\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "## 9. Perf impact\n\nnone\n",
        "## 9. Perf impact\n\n- UPDATED: a body variant\n",
    );
    assert_only(&fixture, Rule::FieldDuplicate, "a variant in capitals");
    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "Owns: tasks and evidence\n",
        "Owns: tasks and evidence\n\nstatus: draft\n",
    );
    assert_only(
        &fixture,
        Rule::FieldDuplicate,
        "a part header holds no prose",
    );
}

/// A code span that runs past the colon quotes a field, on a wrapped line or anywhere.
#[test]
fn a_quoted_field_states_nothing() {
    assert_eq!(value_of("`Pin: x` holds", "Pin", Spelling::Header), None);
    assert_eq!(value_of("`Pin:x`", "Pin", Spelling::Header), None);
    let value = value_of("`Pin:` x", "Pin", Spelling::Header);
    assert_eq!(value.as_deref(), Some("x"));
    let fixture = Fixture::new();
    let path = format!("{DIR}/delivery.md");
    fixture.replace(
        &path,
        "- **Impact:** blocks T-1\n",
        "- **Impact:** a spec that says\n  `Status: draft` blocks T-1\n",
    );
    fixture.replace(
        &path,
        "## 9. Perf impact\n\nnone\n",
        "## 9. Perf impact\n\nA spec\n`Pin: x` once.\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

/// The checklist's "every" rule: each single-valued field, in both orders, reads one RED.
/// Neither statement reaches a later rule, so the order cannot change the outcome.
#[test]
fn every_header_field_is_stated_once_in_either_order() {
    let index = format!("{DIR}/index.md");
    for (label, real, other) in [
        ("Status", "draft", "approved"),
        ("Tracker", "GH-1", "GH-2"),
        ("Owner", "owner", "other"),
        ("Updated", "2026-10-31", "2026-10-30"),
        ("Pin", PIN, ABSENT_SHA),
        ("Pin", PIN, FOREIGN_SHA),
        ("Pin", PIN, "1111111"),
        ("Pin", PIN, PIN),
    ] {
        let line = format!("- {label}: {real}\n");
        let extra = format!("- {label}: {other}\n");
        for text in [format!("{line}{extra}"), format!("{extra}{line}")] {
            for path in [SINGLE, index.as_str()] {
                let fixture = Fixture::new();
                fixture.replace(path, &line, &text);
                let name = format!("{path}: {text}");
                assert_only(&fixture, Rule::FieldDuplicate, &name);
            }
        }
    }
}

#[test]
fn every_required_field_is_stated_once_in_either_order() {
    let acceptance = format!("{DIR}/acceptance.md");
    let delivery = format!("{DIR}/delivery.md");
    let mut fields: Vec<(&str, &str)> = Vec::new();
    fields.extend(AC_FIELDS.map(|label| (acceptance.as_str(), label)));
    fields.extend(BLOCKER_FIELDS.map(|label| (delivery.as_str(), label)));
    fields.extend(EVIDENCE_FIELDS.map(|label| (delivery.as_str(), label)));
    fields.push((delivery.as_str(), TASK_BLOCKERS_FIELD));
    for (path, label) in fields {
        let fixture = Fixture::new();
        let text = fixture.read(path);
        let needle = format!("**{label}:**");
        let line = text
            .lines()
            .find(|line| line.contains(&needle))
            .unwrap_or_else(|| panic!("no {label}"));
        let lead = &line[..line.find('-').expect("list item")];
        let extra = format!("{lead}- **{label}:** another value");
        for repeated in [format!("{line}\n{extra}"), format!("{extra}\n{line}")] {
            let fixture = Fixture::new();
            fixture.replace(path, line, &repeated);
            assert_only(&fixture, Rule::FieldDuplicate, &repeated);
        }
    }
}

/// A value may run across nested lines. That is one field, and each line is read.
#[test]
fn one_field_may_hold_many_nested_statements() {
    for value in [
        "- **Then:** two results:\n  - one\n  - two\n",
        "- **Then:**\n  - one\n  - two\n",
        "- **Then:** a record:\n  ```json\n  {\n    \"a\": 1\n  }\n  ```\n",
        "- **Then:**\n  ```text\n  a\n  ---\n  ```\n  - another result\n",
    ] {
        let fixture = Fixture::new();
        fixture.replace(SINGLE, "- **Then:** a result\n", value);
        assert!(fixture.rules().is_empty(), "{value}: {:?}", fixture.rules());
    }
    let fixture = Fixture::new();
    let path = format!("{DIR}/delivery.md");
    fixture.replace(
        &path,
        "- **Blocked by:** B-1\n",
        "- **Blocked by:**\n  - B-1\n  - T-1\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn a_fence_is_one_statement_wherever_it_opens() {
    let statements = |text: &str| {
        let doc = Doc::parse("x.md", text);
        field_statements(&doc, 0, doc.line_count(), None)
    };
    assert_eq!(
        statements("- **Then:**\n  - ```\n    x\n    y\n    ```\n  - z\n"),
        ["x y", "z"]
    );
    assert_eq!(
        statements("- **Then:**\n  ```text\n  x\n\n  y\n  ```\n  - z\n"),
        ["x y", "z"]
    );
    assert_eq!(
        statements("- **Then:**\n  ```text\n  x\n  ```\n\n  ```text\n  y\n  ```\n"),
        ["x y"],
        "a fence is no value, so it joins what stands before it"
    );
}

/// A statement is a block, not a line: a wrapped word is the end of a sentence.
#[test]
fn a_wrapped_line_continues_its_statement() {
    let statements = |text: &str, inline: &str| {
        let doc = Doc::parse("x.md", text);
        field_statements(&doc, 0, doc.line_count(), Some(inline.to_owned()))
    };
    assert_eq!(statements("- Then: a\n  b\n  c\n", "a"), ["a b c"]);
    assert_eq!(statements("- Then: a\n  - b\n    c\n", "a"), ["a", "b c"]);
    assert_eq!(statements("- Then: a\n\n  b\n", "a"), ["a", "b"]);
    assert_eq!(
        statements("- Then: a\n  | x |\n  |---|\n", "a"),
        ["a | x |"]
    );
    assert_eq!(statements("- Then: a\n\n  ```\n  b\n  ```\n", "a"), ["a b"]);
    assert_eq!(
        statements("- Then: a\n  ```\n  b\n  ```\n  c\n", "a"),
        ["a b", "c"]
    );
    for value in [
        "- **Then:** the state stays\n  unknown.\n",
        "- **Then:** the reply stays\n  pending.\n",
        "- **Then:** the count is\n  none\n",
        "- **Then:** a table:\n  | a | b |\n  |---|---|\n  | 1 | 2 |\n",
        "- **Then:** the gate prints:\n\n  ```text\n  none\n  ```\n",
        "- **Then:** the gate prints:\n  ```text\n  TODO\n  ```\n",
    ] {
        let fixture = Fixture::new();
        fixture.replace(SINGLE, "- **Then:** a result\n", value);
        assert!(fixture.rules().is_empty(), "{value}: {:?}", fixture.rules());
    }
    for (value, rule) in [
        ("- **Then:** real\n  - TBD\n", Rule::FieldPlaceholder),
        ("- **Then:** real\n\n  TBD\n", Rule::FieldPlaceholder),
        ("- **Then:**\n  TBD\n", Rule::FieldPlaceholder),
        ("- **Then:**\n  ```\n  TBD\n  ```\n", Rule::FieldPlaceholder),
        ("- **Then:** real\n  - UNKNOWN\n", Rule::FieldUnknown),
    ] {
        let fixture = Fixture::new();
        fixture.replace(SINGLE, "- **Then:** a result\n", value);
        assert_only(&fixture, rule, value);
    }
}

/// `<https://...>` is an autolink, the form the template's `<command or link>` asks for.
#[test]
fn an_autolink_is_a_value_and_a_slot_is_not() {
    let url = "<https://github.com/gyldlab/keld/actions/runs/1>";
    assert_eq!(non_value(url, true), None);
    assert_eq!(
        non_value("<command or link>", false),
        Some(NonValue::Placeholder)
    );
}

/// `Status` and `Owner` also label a blocker, and a section may use them in its own prose.
#[test]
fn a_header_label_keeps_its_other_meanings() {
    let fixture = Fixture::new();
    fixture.replace(
        SINGLE,
        "## 5.",
        "Owner: the team\n\nStatus: unchanged\n\n## 5.",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

/// A repeated migrated slot still holds no real value, so its row stays right.
#[test]
fn a_repeated_migrated_slot_is_one_defect() {
    let path = format!("{DIR}/acceptance.md");
    let slot = "  - **Single fault:** UNKNOWN: none named\n";
    for second in [
        "  - **Single fault:** UNKNOWN: none named\n",
        "  - **Single fault:** a real fault\n",
    ] {
        for text in [format!("{slot}{second}"), format!("{second}{slot}")] {
            let fixture = migrated_fixture();
            fixture.replace(&path, slot, &text);
            assert_only(&fixture, Rule::FieldDuplicate, &text);
        }
    }
    let fixture = migrated_fixture();
    fixture.replace(
        &path,
        "- **Then:** a result\n",
        "- **Then:** a result\n- **Then:** another\n",
    );
    assert_only(
        &fixture,
        Rule::FieldDuplicate,
        "a slot the row does not name",
    );
}

/// The migrated header slot of KEL-142, with the reviewer's second `Owner`.
#[test]
fn a_repeated_migrated_owner_is_one_defect() {
    let fixture = migrated_fixture();
    let path = format!("{DIR}/index.md");
    fixture.replace(
        BASELINE,
        MIGRATED_CELL,
        &format!("{MIGRATED_CELL},{HEADER_OWNER_SLOT}"),
    );
    fixture.replace(&path, "- Owner: owner\n", "- Owner: UNKNOWN (none named)\n");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    fixture.replace(
        &path,
        "- Owner: UNKNOWN (none named)\n",
        "- Owner: UNKNOWN (none named)\n- Owner: other\n",
    );
    assert_only(&fixture, Rule::FieldDuplicate, "second migrated owner");
}

/// Both statements decide the same row, so the base must refuse it too.
#[test]
fn a_contradictory_row_is_refused_here_and_at_the_base() {
    let fixture = Fixture::new();
    let row = format!("{BIG}\t600\t-\t-\texempt\n");
    fixture.replace(BASELINE, &row, &format!("{row}{BIG}\t599\t-\t-\texempt\n"));
    assert!(
        fixture.rules().contains("BASELINE-FORMAT"),
        "{:?}",
        fixture.rules()
    );
    let mut fixture = Fixture::new();
    let doubled = format!("{BASELINE_HEADER}\n{row}{row}");
    fixture.previous = Ok(Some(doubled));
    let error = fixture.try_run().err().expect("a doubled base is no pass");
    assert!(error.contains("does not parse"), "{error}");
}

/// A split section keeps a subtitle in each part, as KEL-142 section 4 does.
#[test]
fn a_split_section_cannot_restate_a_subtitle_in_another_spelling() {
    for spelling in ["ONE", "one", " one"] {
        let fixture = Fixture::new();
        let path = format!("{DIR}/design.md");
        fixture.replace(&path, "## 5. Boundaries\n", "## 5. Boundaries: one\n");
        let delivery = format!("{DIR}/delivery.md");
        fixture.replace(
            &delivery,
            "## 6. Tasks",
            &format!("## 5. Boundaries: {spelling}\n\nnone\n\n## 6. Tasks"),
        );
        assert_only(&fixture, Rule::Sections, spelling);
    }
}

#[test]
fn parts_may_split_a_section_under_subtitles() {
    let fixture = Fixture::new();
    let path = format!("{DIR}/design.md");
    fixture.replace(&path, "## 5. Boundaries\n", "## 5. Boundaries: inside\n");
    let delivery = format!("{DIR}/delivery.md");
    fixture.replace(
        &delivery,
        "## 6. Tasks",
        "## 5. Boundaries: outside\n\nnone\n\n## 6. Tasks",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
}

#[test]
fn a_target_may_follow_its_label_but_prose_makes_no_definition() {
    let target = |text: &str| targets_of(text);
    assert_eq!(target("[a]:\n  b.md\n"), ["b.md"]);
    assert_eq!(target("[a]:\n  b.md 'title'\n"), ["b.md"]);
    assert!(target("[a]:\nsome prose here\n").is_empty());
    assert!(target("[a]:\n\n").is_empty());
    for block in ["---", "***", "===", "# a.md", "- a.md", "> a.md"] {
        assert!(target(&format!("[a]:\n{block}\n")).is_empty(), "{block}");
    }
    assert_eq!(
        target("[a]:\n    b.md\n"),
        ["b.md"],
        "indentation continues a paragraph"
    );
    let whole = Doc::parse("x.md", "[a]: b.md\nc.md\n");
    assert_eq!(shown(&whole, 1).as_deref(), Some("c.md"), "already whole");
    let titled = Doc::parse("x.md", "[a]: b.md\n'title'\n");
    assert_eq!(shown(&titled, 1), None, "a title on the next line");
    assert!(target("[^1]:\nb.md\n").is_empty(), "a footnote");
    let fixture = Fixture::new();
    let path = format!("{DIR}/design.md");
    let block = "### D-1: choice\n\n[a]:\n  index.md\n\n[note]:\nthe host owns it\n\n\
                 [note]:\nthe guest owns it\n\n[rule]:\n***\n\n[item]:\n- gone.md\n";
    fixture.replace(&path, "### D-1: choice\n", block);
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    let block = "### D-1: choice\n\n[a]: index.md\n\n- [a]: index.md\n";
    fixture.replace(&path, "### D-1: choice\n", block);
    assert_only(&fixture, Rule::Link, "a repeat in a list item");
    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "### D-1: choice\n",
        "### D-1: choice\n\n- [a]: gone.md\n",
    );
    assert_only(&fixture, Rule::Link, "a target in a list item");
}

#[test]
fn a_label_is_folded_as_commonmark_folds_it() {
    let label = label_of;
    assert_eq!(label("[Foo  Bar]: a.md"), label("[foo bar]: b.md"));
    assert_eq!(label("  [A]: a.md"), Some("a".to_owned()));
    assert_eq!(label("[STRASSE]: a.md"), label("[stra\u{df}e]: b.md"));
    assert_eq!(
        label("[\u{1e9e}]: a.md"),
        label("[SS]: b.md"),
        "CommonMark's example"
    );
    assert_ne!(
        label("[a\u{a0}b]: a.md"),
        label("[a b]: b.md"),
        "only ASCII space"
    );
    assert_eq!(
        label("- [A]: a.md"),
        Some("a".to_owned()),
        "a list item holds one"
    );
    assert_eq!(label("[a]: <>"), Some("a".to_owned()), "an empty target");
    assert_eq!(label("[a]:"), None, "no target");
    assert_eq!(label("[a]:\nprose words here"), None);
    assert_eq!(label("[a]:\n---"), None);
    assert_eq!(label("[A]:\n  b.md"), Some("a".to_owned()));
    assert_eq!(label("[^1]: a footnote"), Some("^1".to_owned()));
    assert_eq!(label("[FACT]: the host owns it"), None, "prose");
    assert_eq!(label("`[a]: b.md`"), None, "code");
    assert_eq!(label("`x` [a]: b.md"), None, "text before the bracket");
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
    // A drained row, or a value back under the byte cap, is the shrink this test guards.
    if let Some(bytes) = rows.get(path).and_then(|row| row.bytes) {
        assert!(bytes < at_seal, "{bytes} at head, {at_seal} at the seal");
    }
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
        "TODO items stay in the list",
        "NA values are refused",
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
        "\"UNKNOWN\"",
        "[UNKNOWN]",
        "'unknown'",
    ] {
        assert_eq!(non_value(value, false), Some(NonValue::Unknown), "{value}");
    }
    for value in [
        "<state>",
        "TBD",
        "todo",
        "N/A",
        "-",
        "?",
        "pending",
        "[TBD]",
        "\"TBD\"",
        "TBD?",
        "TBD: the owner decides",
        "TODO (write this)",
        "(n/a)",
    ] {
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
    assert_eq!(shown(&doc, 2).as_deref(), Some("continued"));
    assert_eq!(shown(&doc, 7).as_deref(), Some("text"));
    assert_eq!(shown(&doc, 9), None, "four columns past the item");
}

/// Facts of the one lexer, each from the CommonMark 0.31.2 examples.
#[test]
fn the_lexer_follows_commonmark_blocks() {
    let doc = |text: &str| Doc::parse("x.md", text);
    let heading_levels = |text: &str| -> Vec<(usize, String)> {
        doc(text)
            .headings()
            .iter()
            .map(|heading| (heading.level, heading.text().to_owned()))
            .collect()
    };
    let one = |level: usize, title: &str| vec![(level, title.to_owned())];
    assert_eq!(heading_levels("   ## a"), one(2, "a"));
    assert_eq!(heading_levels("##\ta"), one(2, "a"));
    assert_eq!(heading_levels("## a ##"), one(2, "a"));
    assert_eq!(heading_levels("## a#"), one(2, "a#"));
    assert_eq!(heading_levels("## a \\#"), one(2, "a #"));
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
    assert_eq!(
        heading_levels("~~a~~ ## b"),
        vec![],
        "struck text is not a marker"
    );
    assert_eq!(heading_levels("## ~~a~~ b ##"), one(2, "b"));
    assert_eq!(heading_levels("## a <!-- b --> ##"), one(2, "a"));
    assert_eq!(
        heading_levels("## a <!--\n## b\n-->\n"),
        vec![(2, "a <!--".to_owned()), (2, "b".to_owned())],
        "a heading is one line, so its `<!--` is literal"
    );
    for (text, why) in [
        ("a <!--\n\n## b\n\n-->\n", "a blank ends the paragraph"),
        ("a <!--\n## b\n-->\n", "a heading ends the paragraph"),
        ("> a <!--\n\n## b\n-->\n", "a blank ends the quote"),
        ("> <!--\n## b\n-->\n", "a heading ends the quote"),
        ("- <!--\n## b\n-->\n", "the item ends the block"),
        ("- <pre>\n## b\n", "the item ends the block"),
        ("> a\n<span>\n## b\n", "lazy text opens no HTML block"),
        ("-\ta\n\n    ## b\n", "a tab reaches the content column"),
    ] {
        assert_eq!(heading_levels(text), one(2, "b"), "{why}");
    }
    assert_eq!(
        heading_levels("- <!--\n\n  ## a\n  -->\n## b\n"),
        one(2, "b"),
        "a blank line stays in the item"
    );
    assert!(doc("- <!--\n## b\n").unclosed_raw.is_none());
    assert!(doc("- <!--\n\n  x\n").unclosed_raw.is_some());
    assert_eq!(
        heading_levels("> a\n===\n"),
        vec![],
        "no underline reaches a quote"
    );
    assert_eq!(heading_levels("> - a\n<span>\n## b\n"), one(2, "b"));
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
        doc("<pre>\n</style>\n").unclosed_raw.is_none(),
        "any raw end closes any raw element"
    );
    assert_eq!(heading_levels("<pre>\n</style>\n## a\n"), one(2, "a"));
    assert!(
        shown(&doc("<https://example.com>\n"), 0).is_some(),
        "an autolink"
    );
    assert!(shown(&doc("<id>-<slug> text\n"), 0).is_some(), "prose");
    assert!(shown(&doc("<state>\n"), 0).is_none(), "a lone tag");
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
    assert_eq!(shown(&doc, 2).as_deref(), Some("c"));
    assert_eq!(shown(&doc, 4), None, "no list opened, so this is code");
}

/// Whether an item stays open decides whether a deep line is its content or code.
#[test]
fn an_open_list_item_decides_what_indented_text_is() {
    let last = |text: &str| shown_lines(text).last().cloned().flatten();
    let content = Some("c".to_owned());
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
    let deep = Some("b".to_owned());
    assert_eq!(
        last("2. a\n\n     b\n"),
        deep,
        "no paragraph, so `2.` opens a list"
    );
    assert_eq!(
        last("-\n\n     b\n"),
        None,
        "a blank line ends an empty item"
    );
}

/// Blocks parse first, so a comment spans lines only inside one paragraph.
#[test]
fn a_comment_spans_lines_only_inside_its_paragraph() {
    let visible = shown_lines;
    let shown = |text: &str| Some(text.to_owned());
    assert_eq!(
        visible("a <!-- b\nc\nd --> e\n"),
        [shown("a"), None, shown("e")]
    );
    assert_eq!(
        visible("a <!-- b\nc --> d <!-- e\nf --> g\n"),
        [shown("a"), shown("d"), shown("g")]
    );
    assert_eq!(
        visible("a <!-- b\nc\n"),
        [shown("a <!-- b"), shown("c")],
        "nothing closes it, so it is text"
    );
    assert_eq!(
        visible("a <!-- b\n\nc --> d\n"),
        [shown("a <!-- b"), None, shown("c --> d")],
        "a blank line ends the paragraph"
    );
    assert_eq!(
        visible("- a\nb <!-- c\nd -->\n"),
        [shown("a"), shown("b"), None],
        "a lazy line is still that paragraph"
    );
    assert_eq!(
        visible("> a <!-- b\nc --> d\n"),
        [shown("a"), shown("d")],
        "a lazy line after a quote too"
    );
    assert_eq!(
        visible("a <!-- b\n> c --> d\n"),
        [shown("a <!-- b"), shown("c --> d")],
        "a quote ends the paragraph before it"
    );
}

/// `~~x~~ ### AC-1` renders as a paragraph, so it defines nothing and its `AC-1` dangles.
#[test]
fn struck_text_does_not_turn_a_line_into_a_definition() {
    let fixture = Fixture::new();
    fixture.replace(SINGLE, "### AC-1: rule", "~~x~~ ### AC-1: rule");
    let expected = BTreeSet::from([Rule::IdDangling.code(), Rule::IdRequired.code()]);
    assert_eq!(fixture.rules(), expected);
}

/// A line of list markers stays linear. A real spec never nests this deep.
#[test]
fn list_nesting_stops_at_the_depth_cap() {
    let depth = MAX_LIST_DEPTH + 4;
    let doc = Doc::parse(
        "x.md",
        &format!("{}x\n\n{}y\n", "- ".repeat(depth), "  ".repeat(depth)),
    );
    assert_eq!(shown(&doc, 2), None, "past the cap, the column is code");
    let doc = Doc::parse(
        "x.md",
        &format!(
            "{}x\n\n{}y\n",
            "- ".repeat(MAX_LIST_DEPTH),
            "  ".repeat(MAX_LIST_DEPTH)
        ),
    );
    assert!(
        shown(&doc, 2).is_some(),
        "at the cap, the column is content"
    );
    assert!(is_thematic("- - -"));
    assert!(is_thematic("***"));
    assert!(!is_thematic("--"));
    assert!(!is_thematic("- - x"));
    assert!(!is_thematic("x"));
    assert!(!is_thematic(""));
}

#[test]
fn inline_comments_leave_the_line_around_them() {
    let text = |line: &str| render_text(line).text;
    assert_eq!(text("a <!-- b --> c"), "a  c");
    assert_eq!(text("a <!-- b"), "a <!-- b");
    assert_eq!(text("a <!--> b"), "a  b");
    assert_eq!(text("`<!--` a"), "<!-- a");
    assert_eq!(text("a <!-- ` --> b `"), "a  b `");
    assert_eq!(text("<!--a--> <!--b-->"), "");
    assert_eq!(text(r"\<!-- a -->"), "<!-- a -->");
    assert_eq!(text(r"\\<!-- a -->"), r"\");
}

/// The review's control: a file outside the root that renders the spec heading is a spec,
/// in every spelling that renders the same, and a plain note stays green.
#[test]
fn every_spelling_of_the_spec_heading_is_outside() {
    for heading in [
        "## Acceptance criteria\n",
        "## Acceptance&#32;criteria\n",
        "## Acceptance&#x20;criteria\n",
        "## Acceptance&#00000032;criteria\n",
        "## Acceptance&nbsp;criteria\n",
        "## Acceptance&#xA0;criteria\n",
        "## Acceptance&Tab;criteria\n",
        "## Acceptance&NewLine;criteria\n",
        "## Acceptance<br>criteria\n",
        "## Acceptance<!-- x --> criteria\n",
        "## Accept<!-- x -->ance criteria\n",
        "## <span>Acceptance</span> criteria\n",
        "## [Acceptance criteria](x.md)\n",
        "## [Acceptance criteria][r]\n\n[r]: x.md\n",
        "## ![Acceptance criteria](x.png)\n",
        "## ACCEPTANCE CRITERIA\n",
        "## **Acceptance** _criteria_\n",
        "## `Acceptance criteria`\n",
        "## Accept&shy;ance criteria\n",
        "## Accept&#8203;ance criteria\n",
        "## &#65;cceptance criteria\n",
        "## ~~Acceptance criteria~~\n",
        "Acceptance criteria\n---\n",
        "Acceptance&#32;criteria\n===\n",
        "Acceptance\ncriteria\n---\n",
        "> ## Acceptance&#32;criteria\n",
        "> Acceptance criteria\n> ---\n",
        "> > ## Acceptance criteria\n",
        "- > ## Acceptance criteria\n",
        "## Acceptance&foo;criteria\n",
        "## Accept&foo;ance criteria\n",
        "## &foo;cceptance criteria\n",
        "## &Acirc;cceptance criteria\n",
    ] {
        let mut fixture = Fixture::new();
        fixture.track("crates/keld-compat/gh999-offroot.md", heading.as_bytes());
        assert_only(&fixture, Rule::Outside, heading);
    }
    for note in [
        "# Notes\n\nA note with &nbsp; and &#32; and &foo; in its prose.\n",
        "## Caf&eacute; report\n",
        "## AT&T; rules\n",
        "## &foo;\n",
        "## Acceptance\n\n## Criteria\n",
        "`## Acceptance criteria`\n",
        "\\## Acceptance criteria\n",
        "## Acceptance&#45;criteria\n",
        "## Acceptance&amp;#32;criteria\n",
        "<div>\n\n<!-- ## Acceptance criteria -->\n\n</div>\n",
    ] {
        let mut fixture = Fixture::new();
        fixture.track("crates/keld-compat/gh999-offroot.md", note.as_bytes());
        assert!(fixture.rules().is_empty(), "{note}: {:?}", fixture.rules());
    }
}

/// The review's control: only a fence a reader sees is the copy source.
#[test]
fn a_hidden_copy_source_is_no_copy_source() {
    let copy = single_spec();
    assert!(Fixture::new().rules().is_empty(), "the unchanged template");
    for template in [
        format!("<!--\n```markdown\n{copy}```\n-->\n"),
        format!("<!-- x\n\n```markdown\n{copy}```\n\n-->\n"),
        format!("<pre>\n```markdown\n{copy}```\n</pre>\n"),
        format!("<script>\n```markdown\n{copy}```\n</script>\n"),
        format!("<div>\n```markdown\n{copy}```\n"),
        format!("````\n```markdown\n{copy}```\n````\n"),
        format!("~~~\n```markdown\n{copy}```\n~~~\n"),
        format!("    ```markdown\n{}", copy.replace('\n', "\n    ")),
        format!("> ```markdown\n> {}\n> ```\n", copy.replace('\n', "\n> ")),
        format!("```markdown\n{copy}"),
        format!("```markdown\n```\n"),
        "No fence at all.\n".to_owned(),
        format!("```markdown\n{copy}```\n\n```markdown\n{copy}```\n"),
    ] {
        let fixture = Fixture::new();
        fixture.write(TEMPLATE, &format!("# Template\n\n{template}"));
        assert_only(&fixture, Rule::Sections, &template);
        assert_eq!(fixture.run().findings.len(), 1, "{template}");
    }
    for template in [
        format!("~~~markdown\n{copy}~~~\n"),
        format!("````markdown\n{copy}````\n"),
        format!("``` markdown extra\n{copy}```\n"),
        format!("<details>\n\n```markdown\n{copy}```\n\n</details>\n"),
        format!("```&#109;arkdown\n{copy}```\n"),
        format!("<!--\n```markdown\nhidden\n```\n-->\n\n```markdown\n{copy}```\n"),
    ] {
        let fixture = Fixture::new();
        fixture.write(TEMPLATE, &format!("# Template\n\n{template}"));
        assert!(
            fixture.rules().is_empty(),
            "{template}: {:?}",
            fixture.rules()
        );
    }
    let fixture = Fixture::new();
    fixture.write(
        TEMPLATE,
        "# Template\n\n<!--\n```markdown\nhidden\n```\n-->\n\n```markdown\n# broken\n```\n",
    );
    assert!(
        fixture.rules().contains("SECTIONS"),
        "only the visible fence is validated: {:?}",
        fixture.rules()
    );
}

/// A label reads as what it renders as, so each spelling states the field or repeats it.
#[test]
fn an_encoded_label_is_the_label_it_renders_as() {
    for spelled in [
        format!("- Pin&#58; {ABSENT_SHA}"),
        format!("- Pin&colon; {ABSENT_SHA}"),
        format!("- Pin\\: {ABSENT_SHA}"),
        format!("- [Pin](gh1-single.md): {ABSENT_SHA}"),
        format!("- [Pin](gh1-single.md)&#58; {ABSENT_SHA}"),
        format!("- &#80;in: {ABSENT_SHA}"),
        format!("- &#x50;in: {ABSENT_SHA}"),
        format!("- P&#105;n: {ABSENT_SHA}"),
        format!("- Pi&shy;n: {ABSENT_SHA}"),
        format!("- Pi&#8203;n: {ABSENT_SHA}"),
        format!("- Pin&nbsp;: {ABSENT_SHA}"),
        format!("- Pi<!-- x -->n: {ABSENT_SHA}"),
        format!("- <b>Pin</b>: {ABSENT_SHA}"),
        format!("- <span>Pin:</span> {ABSENT_SHA}"),
        format!("- **Pin&#58;** {ABSENT_SHA}"),
        format!("- *Pin*\\: {ABSENT_SHA}"),
    ] {
        let fixture = Fixture::new();
        let line = format!("- Pin: {PIN}\n");
        fixture.replace(SINGLE, &line, &format!("{line}{spelled}\n"));
        assert_only(&fixture, Rule::FieldDuplicate, &spelled);
    }
    for spelled in [
        format!("- &#80;in: {PIN}\n"),
        format!("- Pin&#58; {PIN}\n"),
        format!("- Pin\\: {PIN}\n"),
        format!("- [Pin](index.md): {PIN}\n"),
        format!("- Pin: `{PIN}`\n"),
        format!("- Pin: &#49;{}\n", &PIN[1..]),
    ] {
        let fixture = Fixture::new();
        let path = format!("{DIR}/index.md");
        fixture.replace(&path, &format!("- Pin: {PIN}\n"), &spelled);
        assert!(
            fixture.rules().is_empty(),
            "{spelled}: {:?}",
            fixture.rules()
        );
    }
    let fixture = Fixture::new();
    fixture.replace(
        SINGLE,
        "- **Given:** a state\n",
        "- **G&#105;ven&colon;** a state\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    // A label a reader sees as its own text, not a field, stays prose.
    for prose in ["- `Pin: x` holds it\n", "- \\`Pin: x\\` is a label\n"] {
        let fixture = Fixture::new();
        fixture.replace(SINGLE, "## 5.", &format!("{prose}\n## 5."));
        let quoted = prose.starts_with("- `");
        assert_eq!(fixture.rules().is_empty(), quoted, "{prose}");
    }
}

/// The value is what renders too, so an encoded blank or marker states nothing.
#[test]
fn an_encoded_value_is_the_value_it_renders_as() {
    for (value, rule) in [
        ("&nbsp;", Rule::AcFields),
        ("&#160;", Rule::AcFields),
        ("&#xA0;", Rule::AcFields),
        ("&#32;", Rule::AcFields),
        ("&emsp;", Rule::AcFields),
        ("&ZeroWidthSpace;", Rule::AcFields),
        ("<!-- none -->", Rule::AcFields),
        ("&foo;", Rule::FieldPlaceholder),
        ("&#84;BD", Rule::FieldPlaceholder),
        ("T&#66;D", Rule::FieldPlaceholder),
        ("&#84;&#66;&#68;", Rule::FieldPlaceholder),
        ("**TBD**", Rule::FieldPlaceholder),
        ("[TBD](x.md)", Rule::FieldPlaceholder),
        ("<state>", Rule::FieldPlaceholder),
        ("<state> &nbsp;", Rule::FieldPlaceholder),
        ("&lt;state&gt;", Rule::FieldPlaceholder),
        ("&#85;NKNOWN", Rule::FieldUnknown),
        ("[UNKNOWN](x.md)", Rule::FieldUnknown),
    ] {
        let fixture = Fixture::new();
        let path = format!("{DIR}/acceptance.md");
        let line = format!("- **Given:** {value}\n");
        fixture.replace(&path, "- **Given:** a state\n", &line);
        if value.contains("x.md") {
            fixture.write(&format!("{DIR}/x.md"), "");
            fixture.remove(&format!("{DIR}/x.md"));
            fixture.replace(&path, "(x.md)", "(index.md)");
        }
        assert_only(&fixture, rule, value);
    }
    let path = format!("{DIR}/delivery.md");
    let fixture = Fixture::new();
    fixture.replace(&path, "- **Status:** open\n", "- **Status:** &#111;pen\n");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "- **Blocked by:** B-1\n",
        "- **Blocked by:** B&#45;1\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "- **Blocked by:** B-1\n",
        "- **Blocked by:** &#110;one\n",
    );
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    fixture.replace(
        &path,
        "- **Blocked by:** B-1\n",
        "- **Blocked by:** B&#45;9\n",
    );
    assert_only(&fixture, Rule::IdDangling, "an encoded dangling ID");
}

/// An ID reads as what it renders as, wherever it is defined or referenced.
#[test]
fn an_encoded_id_is_the_id_it_renders_as() {
    let acceptance = format!("{DIR}/acceptance.md");
    for heading in [
        "### AC&#45;1: rule\n",
        "### &#65;C-1: rule\n",
        "### &#x41;C&#x2d;1: rule\n",
        "### AC\\-1: rule\n",
        "### <span>AC-1</span>: rule\n",
        "### AC<!-- x -->-1: rule\n",
        "### [AC-1](index.md): rule\n",
        "### **AC-1**: rule\n",
        "### `AC-1`: rule\n",
        "### &shy;AC-1: rule\n",
    ] {
        let fixture = Fixture::new();
        fixture.replace(&acceptance, "### AC-1: rule\n", heading);
        assert!(
            fixture.rules().is_empty(),
            "{heading}: {:?}",
            fixture.rules()
        );
        let fixture = Fixture::new();
        let again = format!("### AC-1: rule\n\n{}{}", "- x\n\n", heading);
        fixture.replace(&acceptance, "### AC-1: rule\n", &again);
        assert!(fixture.rules().contains("ID-DUPLICATE"), "{heading}");
    }
    let design = format!("{DIR}/design.md");
    for reference in [
        "AC&#45;9",
        "&#65;C-9",
        "[AC-9](index.md)",
        "A<!-- x -->C-9",
        "<span>AC-9</span>",
        "AC\\-9",
        "`AC-9`",
    ] {
        let fixture = Fixture::new();
        fixture.replace(&design, "AC-1 needs", &format!("{reference} needs"));
        assert_only(&fixture, Rule::IdDangling, reference);
    }
    let fixture = Fixture::new();
    fixture.replace(&design, "AC-1 needs", "AC1 needs");
    assert_only(&fixture, Rule::IdLegacyForm, "the bare form");
    let fixture = Fixture::new();
    fixture.replace(&design, "AC-1 needs", "A&#67;1 needs");
    assert_only(&fixture, Rule::IdLegacyForm, "the encoded bare form");
    // A link destination is no prose, and neither is an entity a code span keeps.
    let fixture = Fixture::new();
    fixture.replace(&design, "AC-1 needs", "[AC-1](index.md#ac-9) needs");
    fixture.replace(
        &format!("{DIR}/index.md"),
        "## 1. Goal",
        "### AC-9 note\n\n## 1. Goal",
    );
    let rules = fixture.rules();
    assert!(
        !rules.contains("ID-DANGLING") || rules.len() > 1,
        "{rules:?}"
    );
    let fixture = Fixture::new();
    fixture.replace(&design, "AC-1 needs", "`AC&#45;9` and AC-1 need");
    assert!(fixture.rules().is_empty(), "{:?}", fixture.rules());
    // A foreign qualifier renders too.
    for foreign in ["[#1311](index.md) AC-9", "**KEL-9** AC-9", "&#35;1311 AC-9"] {
        let fixture = Fixture::new();
        fixture.replace(&design, "AC-1 needs", &format!("AC-1 and {foreign} need"));
        assert!(
            fixture.rules().is_empty(),
            "{foreign}: {:?}",
            fixture.rules()
        );
    }
}

/// Section titles, the part table and part headers all read their rendered text.
#[test]
fn encoded_structure_reads_as_it_renders() {
    let index = format!("{DIR}/index.md");
    let design = format!("{DIR}/design.md");
    for (path, from, to) in [
        (SINGLE, "## 5. Boundaries", "## 5.&#32;Boundaries"),
        (SINGLE, "## 5. Boundaries", "## 5\\. Bound<!-- x -->aries"),
        (SINGLE, "## 5. Boundaries", "## <b>5. Boundaries</b>"),
        (SINGLE, "# Spec: single", "Spec: single\n==="),
        (
            index.as_str(),
            PART_TABLE_HEADER,
            "| Pa&#114;t | **Owns** | Load&#32;when |",
        ),
        (
            index.as_str(),
            "| [design.md](design.md) |",
            "| [design&#46;md](design&#46;md) |",
        ),
        (
            index.as_str(),
            "| [design.md](design.md) |",
            "| [design.md](design%2Emd) |",
        ),
        (index.as_str(), "## Parts", "## Pa&#114;ts"),
        (
            index.as_str(),
            "## Current blockers",
            "## Current&#32;blockers",
        ),
        (index.as_str(), "- B-1", "- B&#45;1"),
        (
            design.as_str(),
            "Part of: [index](index.md)",
            "Part&#32;of: [index](index.md)",
        ),
        (
            design.as_str(),
            "Part of: [index](index.md)",
            "Part of: [ind&#101;x](index&#46;md)",
        ),
        (design.as_str(), "Owns: decisions", "Own&#115;: decisions"),
    ] {
        let fixture = Fixture::new();
        fixture.replace(path, from, to);
        assert!(fixture.rules().is_empty(), "{to}: {:?}", fixture.rules());
    }
    for (path, from, to, rule) in [
        (
            SINGLE,
            "## 5. Boundaries",
            "## 5.&#160;Boundaries",
            Rule::Sections,
        ),
        (
            SINGLE,
            "## 5. Boundaries",
            "## 5. BOUNDARIES",
            Rule::Sections,
        ),
        (
            SINGLE,
            "# Spec: single",
            "<!--\n# Spec: single\n-->",
            Rule::Fields,
        ),
        (
            index.as_str(),
            PART_TABLE_HEADER,
            "| Part | Owns | Load |",
            Rule::PartTable,
        ),
        (
            design.as_str(),
            "Part of: [index](index.md)",
            "<!-- Part of: [index](index.md) -->",
            Rule::PartHeader,
        ),
        (
            design.as_str(),
            "Part of: [index](index.md)",
            "Part of: index",
            Rule::PartHeader,
        ),
        (
            design.as_str(),
            "Part of: [index](index.md)",
            "Part of: [index](other.md)",
            Rule::PartHeader,
        ),
        (
            design.as_str(),
            "Owns: decisions",
            "Owns: &nbsp;",
            Rule::PartHeader,
        ),
    ] {
        let fixture = Fixture::new();
        fixture.replace(path, from, to);
        assert!(
            fixture.rules().contains(rule.code()),
            "{to}: {:?}",
            fixture.rules()
        );
    }
}

/// A link is checked at the path it renders to, however its text spells the path.
#[test]
fn an_encoded_link_is_checked_at_its_path() {
    for target in [
        "../../../crates/x/src/lib.rs",
        "../../../crates&#47;x/src/lib.rs",
        "../../../crates&sol;x/src/lib.rs",
        "../../../crates%2Fx/src/lib.rs",
        "..%2F..%2F..%2Fcrates%2Fx%2Fsrc%2Flib.rs",
        "%2E%2E/../../crates/x/src/lib.rs",
        "..\\/..\\/..\\/crates/x/src/lib.rs",
        "<../../../crates/x/src/lib.rs>",
        "../../../crates/x/src/lib%2Ers",
    ] {
        let fixture = Fixture::new();
        link_from_design(&fixture, target);
        assert_only(&fixture, Rule::LinkCode, target);
    }
    for block in [
        "[a]: ../../../crates&#47;x/src/lib.rs\n",
        "[a]: ../../../crates%2Fx/src/lib.rs\n",
        "- [a]:\n  ../../../crates/x/src/lib.rs\n",
    ] {
        let fixture = Fixture::new();
        let path = format!("{DIR}/design.md");
        fixture.replace(
            &path,
            "### D-1: choice\n",
            &format!("### D-1: choice\n\n{block}\n"),
        );
        assert_only(&fixture, Rule::LinkCode, block);
    }
    for target in [
        "index&#46;md#1-goal--non-goals",
        "index%2Emd#1%2Dgoal--non-goals",
    ] {
        let fixture = Fixture::new();
        link_from_design(&fixture, target);
        assert!(
            fixture.rules().is_empty(),
            "{target}: {:?}",
            fixture.rules()
        );
    }
    let fixture = Fixture::new();
    link_from_design(&fixture, "%2e%2e/%2e%2e/%2e%2e/%2e%2e/x.md");
    assert_only(
        &fixture,
        Rule::Link,
        "a percent-encoded climb out of the repository",
    );
    let fixture = Fixture::new();
    link_from_design(&fixture, "index.md#no%2Dsuch");
    assert_only(&fixture, Rule::Anchor, "a percent-encoded anchor");
    let fixture = Fixture::new();
    link_from_design(&fixture, "..%2Fresearch%2Fnote.md");
    assert_eq!(
        fixture.run().findings.len(),
        1,
        "the sibling is a real path"
    );
    let fixture = Fixture::new();
    link_from_design(&fixture, "../../research%2Fnote.md");
    let report = fixture.run();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.private_links, 1, "the private checkout, encoded");
    assert_eq!(percent_decoded("a%2Fb%zz%"), "a/b%zz%");
    assert_eq!(percent_decoded("%C3%A9"), "\u{e9}");
    assert_eq!(percent_decoded("%+1"), "%+1", "a sign is no hex digit");
    assert_eq!(percent_decoded("%FF"), "%FF", "not UTF-8");
}

/// A reference no table resolves can render as anything, so a rule refuses to guess.
#[test]
fn an_unresolved_reference_in_a_heading_or_label_is_refused() {
    let acceptance = format!("{DIR}/acceptance.md");
    for (path, from, to) in [
        (SINGLE, "## 5.", "### A note&foo;\n\n## 5."),
        (SINGLE, "## 5. Boundaries", "## 5. Boundaries: &eacute;"),
        (
            acceptance.as_str(),
            "### AC-1: rule",
            "### AC-1: &foo; rule",
        ),
        (
            acceptance.as_str(),
            "### AC-2: rule\n",
            "### AC-2: rule\n\n### &hyphen;AC-3: x\n",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.replace(path, from, to);
        assert!(
            fixture.rules().contains("ENTITY"),
            "{to}: {:?}",
            fixture.rules()
        );
    }
    for label in [
        "&foo;Pin: x\n",
        "Pi&foo;n: x\n",
        "- Pin&foo; x\n",
        "- **Pin&foo;** x\n",
        "- Then&foo;: x\n",
        "- Blocked&foo; by: x\n",
    ] {
        let fixture = Fixture::new();
        fixture.replace(SINGLE, "## 5.", &format!("{label}\n## 5."));
        assert_only(&fixture, Rule::Entity, label);
    }
    for prose in [
        "A note on &foo; in prose.\n",
        "Report&foo;: x\n",
        "\\&foo;Pin: x\n",
        "`&foo;`Pin: x\n",
        "&amp;foo;Pin: x\n",
        "Pin: &foo; costs less\n",
    ] {
        let fixture = Fixture::new();
        fixture.replace(SINGLE, "## 5.", &format!("{prose}\n## 5."));
        let duplicate = prose.starts_with("Pin:");
        let rules = fixture.rules();
        assert!(!rules.contains("ENTITY"), "{prose}: {rules:?}");
        assert_eq!(rules.is_empty(), !duplicate, "{prose}: {rules:?}");
    }
}

/// One table of what the normalizer renders, from the CommonMark 0.31.2 examples.
#[test]
fn the_normalizer_renders_inline_text() {
    let text = |source: &str| render_text(source).text;
    for (source, rendered) in [
        ("&#35; &#1234; &#992; &#0;", "# \u{4d2} \u{3e0} \u{fffd}"),
        ("&#X22; &#XD06; &#xcab;", "\" \u{d06} \u{cab}"),
        ("a&nbsp;b &amp; &copy", "a\u{a0}b & &copy"),
        ("&#xD800; &#x110000;", "\u{fffd} \u{fffd}"),
        ("&#000000065; &#x000000041;", "&#000000065; &#x000000041;"),
        ("&#00000065; &#x00000041;", "A A"),
        ("&amp;amp; &amp;#32;", "&amp; &#32;"),
        ("&x; &#; &#x; &; &nbsp", "&x; &#; &#x; &; &nbsp"),
        (r"\!\#\&\*\[\]\`\\ \a \1", r"!#&*[]`\ \a \1"),
        (r"\&amp; \*a\* \[a\](b)", "&amp; *a* [a](b)"),
        ("`&amp; \\*` x", "&amp; \\* x"),
        ("`` a`b ``", "a`b"),
        ("` a ` `  `", "a"),
        ("*a* **b** ***c*** _d_ __e__", "a b c d e"),
        ("*a **b** c*", "a b c"),
        (
            "snake_case_word 2 * 3 * 4 a*b*c",
            "snake_case_word 2 * 3 * 4 abc",
        ),
        ("*a _b* c_", "a _b c_"),
        ("**a*", "*a"),
        ("* a *", "* a *"),
        ("~~a~~ b ~c~ d", "b  d"),
        ("~~~a~~~ ~b~~", "~~~a~~~ ~b~~"),
        ("x~~ y ~~z", "x~~ y ~~z"),
        ("<b>a</b> <x y=\"1\" z='2' w=3 v>c", "a c"),
        ("<a b, c> <1> < a> <a", "<a b, c> <1> < a> <a"),
        ("a<br>b<BR/>c<br />d", "a b c d"),
        ("a <?p ?> b <!X y> c <![CDATA[d]]> e", "a  b  c  e"),
        (
            "<https://x.y/z> <a@b.co> <m:n>",
            "https://x.y/z a@b.co <m:n>",
        ),
        ("[a](b) ![c](d) [e](f \"t\") [g](<h i>)", "a c e g"),
        ("[a *b*](c) [d](e)f", "a b df"),
        ("[a [b](c) d](e)", "[a b d](e)"),
        ("![a *b* [c](d)](e) f", "a b c f"),
        ("*foo**bar*", "foo**bar"),
        ("***a** b*", "a b"),
        ("[a](b(c) [d](e f) [g]()", "[a](b(c) [d](e f) g"),
        ("[a]&#40;b) \\[c](d)", "[a](b) [c](d)"),
        ("[a][b] [c][] [d]", "[a][b] [c][] [d]"),
        ("**[a](b)** [**c**](d)", "a c"),
        ("[*a](b)*", "*a*"),
        ("a\u{ad}b\u{200b}c\u{2060}d\u{feff}e", "abcde"),
    ] {
        assert_eq!(text(source), rendered, "{source}");
    }
    let links = |source: &str| render_text(source).links;
    assert_eq!(links("[a](b&#47;c) ![d](e%20f)"), ["b/c", "e%20f"]);
    assert_eq!(links(r"[a](b\)c) [d](<e\>f>)"), ["b)c", "e>f"]);
    assert_eq!(
        links("<https://x.y> <a@b.co>"),
        ["https://x.y", "mailto:a@b.co"]
    );
    assert_eq!(
        links("~~[a](b)~~ [c](d)"),
        ["d"],
        "struck text is not binding"
    );
    let referenced = Doc::parse(
        "x.md",
        "[a][r] [b][] [c]\n\n[r]: /u\n[b]: /v\n[c]: /w&#47;x\n",
    );
    assert_eq!(shown(&referenced, 0).as_deref(), Some("a b c"));
    let targets: Vec<&str> = referenced.targets().iter().map(|(_, link)| *link).collect();
    assert_eq!(targets, ["/u", "/v", "/w/x", "/u", "/v", "/w/x"]);
    let unresolved = render_text("&foo; \\&bar; `&baz;` &amp;qux;");
    assert_eq!(unresolved.text, "&foo; &bar; &baz; &qux;");
    assert_eq!(
        unresolved.unresolved,
        [0..5],
        "only the first is a reference"
    );
    let code = render_text("a `b` **`c`** ~~`d`~~");
    assert_eq!(code.code_spans().collect::<Vec<_>>(), ["b", "c"]);
}

/// Inline constructs span lines inside one paragraph, and belong to the line that opens them.
#[test]
fn a_paragraph_renders_as_one_inline_run() {
    assert_eq!(
        shown_lines("a `b\nc` d\n"),
        [Some("a b c".to_owned()), Some("d".to_owned())]
    );
    assert_eq!(
        shown_lines("**a\nb** c\n[d\ne](f)\n"),
        [
            Some("a".to_owned()),
            Some("b c".to_owned()),
            Some("d".to_owned()),
            Some("e".to_owned())
        ]
    );
    assert_eq!(
        shown_lines("a <span\nclass=\"x\">b\n"),
        [Some("a".to_owned()), Some("b".to_owned())]
    );
    assert_eq!(
        shown_lines("a\\\nb\n"),
        [Some("a".to_owned()), Some("b".to_owned())]
    );
    let doc = Doc::parse("x.md", "`a\nb`\n");
    assert_eq!(doc.lines()[0].prose().map(|line| line.code.len()), Some(1));
    // Blocks parse first: a span never joins two blocks.
    assert_eq!(
        shown_lines("`a\n\nb`\n"),
        [Some("`a".to_owned()), None, Some("b`".to_owned())]
    );
    assert_eq!(
        shown_lines("| `a | b` |\n|---|---|\n"),
        [Some("| `a | b` |".to_owned()), None]
    );
}

/// GitHub's block rules past the CommonMark examples, each from `POST /markdown`, 2026-10-31.
#[test]
fn the_lexer_follows_github_on_these_blocks() {
    let last = |text: &str| shown_lines(text).last().cloned().flatten();
    assert_eq!(last("*\n\n    b\n"), None, "a blank ends an empty item");
    assert_eq!(last("> *\n>\n>     b\n"), None, "in a quote too");
    assert_eq!(
        last("- -\n\n  b\n"),
        Some("b".to_owned()),
        "the outer item stays"
    );
    assert_eq!(
        last("-\n  foo\n"),
        Some("foo".to_owned()),
        "no blank, so it holds it"
    );
    assert_eq!(
        last("-\n  foo\n\n  bar\n"),
        Some("bar".to_owned()),
        "the item stays open"
    );
    assert_eq!(last("| a |\n|---|\n    b\n"), None, "code ends a table");
    assert_eq!(
        last("| a |\n|---|\n    b\nc\n"),
        Some("c".to_owned()),
        "text past that code is a paragraph"
    );
    let rows = |text: &str| {
        let doc = Doc::parse("x.md", text);
        doc.lines()
            .iter()
            .filter(|line| line.cells().is_some())
            .count()
    };
    assert_eq!(
        rows("a | b\n--- | ---\n"),
        2,
        "no leading pipe, still a table"
    );
    assert_eq!(
        shown_lines("> a | b\n--- | ---\n")[1].as_deref(),
        Some("--- | ---")
    );
    let heads = |text: &str| Doc::parse("x.md", text).headings().len();
    assert_eq!(heads("| a |\n|---|\n===\n"), 0, "`===` is a row");
    assert_eq!(
        heads("[a]: b\n===\n"),
        0,
        "definitions leave nothing to underline"
    );
    assert_eq!(heads("[a]: b\n---\n"), 0, "a thematic break needs no text");
    assert_eq!(heads("[a]: b\nc\n---\n"), 1, "a definition leaves text");
    assert_eq!(heads("[a]:\n---\n"), 1, "no target, so no definition");
    assert_eq!(last("[a]: b\n===\n").as_deref(), Some("==="));
    assert_eq!(
        last("[a]: b\n---\n").as_deref(),
        Some("---"),
        "cmark keeps it as text"
    );
    assert_eq!(last("[a]: b\n***\n"), None, "this one is a real break");
    let fence = |text: &str| {
        Doc::parse("x.md", text)
            .fences()
            .first()
            .map(|found| (found.language.clone(), found.body.clone()))
    };
    assert_eq!(
        fence("```\n\ta\n```\n"),
        Some((String::new(), "\ta\n".to_owned())),
        "code keeps its tab"
    );
    assert_eq!(
        fence("- ```\n  \ta\n  ```\n"),
        Some((String::new(), "\ta\n".to_owned()))
    );
    assert_eq!(
        fence("  ```\n    a\n b\n  ```\n"),
        Some((String::new(), "  a\nb\n".to_owned())),
        "the fence's own indent leaves each line"
    );
    assert_eq!(
        fence("``` a&#32;b c\n```\n"),
        Some(("a".to_owned(), String::new())),
        "the info string decodes before it splits"
    );
    assert_eq!(
        fence("```\na\n```\u{a0}\n").map(|(_, body)| body.len()),
        Some(8)
    );
    assert_eq!(heads("\u{a0}## a\n"), 0, "a no-break space is text");
    assert!(!Doc::parse("x.md", "\u{a0}\n").line(0).is_blank());
}

/// A block quote is a nested document: it holds headings a census reads and no field.
#[test]
fn a_block_quote_is_read_as_a_quote() {
    let quoted = |text: &str| -> Vec<String> {
        let doc = Doc::parse("x.md", text);
        let heads = doc.quoted_headings().iter();
        heads.map(|heading| heading.text().to_owned()).collect()
    };
    assert_eq!(quoted("> # a\n> > ## b\n"), ["a", "b"]);
    assert_eq!(quoted("> a\n> ---\n> b\n> ===\n"), ["a", "b"]);
    assert_eq!(quoted("> ```\n> # a\n> ```\n"), [""; 0]);
    assert_eq!(quoted("> <!--\n> # a\n> -->\n"), [""; 0]);
    assert_eq!(quoted("> a\n---\n"), [""; 0], "a break ends the quote");
    assert_eq!(quoted("> a\n===\n"), [""; 0], "lazy text");
    assert_eq!(quoted("-   > # a\n    > # b\n"), ["a", "b"]);
    assert_eq!(quoted(">\t# a\n"), ["a"]);
    assert_eq!(quoted(">     # a\n"), [""; 0], "code in the quote");
    let doc = Doc::parse("x.md", "> Pin: x\ny: z\n");
    assert!(doc.line(0).field().is_none(), "a quote holds no field");
    assert!(doc.line(1).field().is_none(), "nor does its lazy line");
    assert!(doc.line(0).prose().is_some());
    let deep = ">".repeat(100_000);
    assert_eq!(Doc::parse("x.md", &format!("{deep} a\n")).line_count(), 1);
    assert!(Doc::parse("x.md", "> ```\n").unclosed_fence.is_none());
    assert!(Doc::parse("x.md", "> <!--\n").unclosed_raw.is_none());
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
    assert_eq!(split_cells(r"| a \| b | c |"), ["a | b", "c"]);
    assert_eq!(split_cells(r"| a \\| b |"), [r"a \\", "b"]);
    let delimiter = |row: &str| {
        let doc = Doc::parse("x.md", row);
        is_table_delimiter(doc.line(0).cells().expect("a row"))
    };
    assert!(delimiter("|---|:--|-:|"));
    assert!(delimiter("| --- | :-: | --- |"));
    assert!(!delimiter("|---|---|"));
    assert!(!delimiter("|x|y|z|"));
}

#[test]
fn links_parse_as_commonmark_destinations() {
    let targets = targets_of;
    assert_eq!(targets(r#"[a](b.md "t")"#), ["b.md"]);
    assert_eq!(targets(r#"[a](b.md "t)") [c](d.md)"#), ["b.md", "d.md"]);
    assert_eq!(targets("[a](<b c.md>)"), ["b c.md"]);
    assert_eq!(targets("[a](b(c).md)"), ["b(c).md"]);
    assert_eq!(targets("![a](b.png)"), ["b.png"]);
    assert_eq!(targets("[a]: b.md"), ["b.md"]);
    assert_eq!(targets("  [a]: <b c.md> 'title'"), ["b c.md"]);
    assert_eq!(targets("[a]: b.md (title)"), ["b.md"]);
    assert!(targets("[^1]: a footnote").is_empty());
    assert!(targets("[^1]: footnote").is_empty());
    assert!(targets("[FACT]: the host owns it").is_empty(), "prose");
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
        id_tokens(&render_text(line).text)
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
    let foreign = |line: &str| {
        id_tokens(&render_text(line).text)
            .iter()
            .all(|token| token.foreign)
    };
    assert!(foreign("[#1311](https://example.com/x) AC-9"));
    assert!(foreign("[KEL-9](u), AC-9"));
    assert!(foreign("([#1311](u) AC-9)"));
    assert!(foreign("(#1311 AC-9)"));
    assert!(foreign("(#1311) AC-9"));
    assert!(foreign("**KEL-9** AC-9"));
    assert!(foreign("`#1311` AC-9"));
    assert!(!foreign("[goal](index.md) AC-9"));
    assert!(!foreign("(goal) AC-9"));
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
    assert_eq!(
        parse_citation("a/b.rs:3@abc@def"),
        cited("a/b.rs", (3, 3), Some("abc@def")),
        "a second pin stays in the commit text"
    );
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

/// github-slugger's loop: a suffix skips every slug a heading already holds.
#[test]
fn a_repeated_heading_skips_a_taken_anchor() {
    let slugs = |text: &str| heading_slugs(&Doc::parse("x.md", text));
    assert_eq!(
        slugs("## Top-1\n\n## Top\n\n## Top\n"),
        BTreeSet::from(["top".to_owned(), "top-1".to_owned(), "top-2".to_owned()])
    );
    assert_eq!(
        slugs("## Top\n\n## Top\n\n## Top-1\n"),
        BTreeSet::from(["top".to_owned(), "top-1".to_owned(), "top-1-1".to_owned()])
    );
    assert_eq!(slugs("## Top\n\n## Top\n\n## Top\n").len(), 3);
    // GitHub's own render of these headings, through `POST /markdown`, on 2026-10-31.
    assert_eq!(
        slugs("## Top\n\n## Top\n\n## Top\n\n## Top-1\n\n## Top-2\n\n## Top\n"),
        ["top", "top-1", "top-2", "top-1-1", "top-2-1", "top-3"]
            .map(str::to_owned)
            .into()
    );
    assert_eq!(
        slugs("## a b\n\n## a-b\n\n## a  b\n\n## A B\n"),
        ["a-b", "a-b-1", "a--b", "a-b-2"].map(str::to_owned).into()
    );
}

#[test]
fn crlf_checkouts_measure_the_same_as_lf() {
    let lf = Doc::parse("x.md", "a\nb\n");
    let crlf = Doc::parse("x.md", "a\r\nb\r\n");
    assert_eq!(measure(&lf), measure(&crlf));
}

/// The adapter against a real repository, including the fail-closed seal.
#[test]
fn a_tracked_path_that_is_not_utf8_is_an_error_not_an_absent_file() {
    let tracked = parse_tracked(b"100644 a 0\ta.md\0120000 b 0\tb.md\0").expect("UTF-8");
    assert_eq!(
        tracked,
        [
            Tracked {
                path: "a.md".to_owned(),
                symlink: false
            },
            Tracked {
                path: "b.md".to_owned(),
                symlink: true
            }
        ]
    );
    let error = parse_tracked(b"100644 a 0\ta.md\0100644 b 0\t\xff.md\0").expect_err("lossy");
    assert!(error.contains("not UTF-8"), "{error}");
}

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

/// Each fact from the mutant round: a label keeps its block role and its value stays its own.
#[test]
fn a_rendered_line_keeps_its_role() {
    let doc = Doc::parse("x.md", "## Pin: x\n\nPin: y\n---\n\n| Pin: z |\n|---|\n");
    assert!(
        doc.line(0).field().is_none(),
        "an ATX heading states no field"
    );
    assert!(
        doc.line(2).field().is_none(),
        "a setext heading states no field"
    );
    assert!(doc.line(5).field().is_none(), "a table row states no field");
    assert_eq!(
        value_of("<b>Pin</b>:", "Pin", Spelling::Header).as_deref(),
        Some("")
    );
    assert_eq!(
        value_of("Pin: <x>", "Pin", Spelling::Header).as_deref(),
        Some("<x>")
    );
    assert_eq!(
        value_of("Pin: <x> y", "Pin", Spelling::Header).as_deref(),
        Some("y")
    );
    assert_eq!(
        value_of("Pin:** y", "Pin", Spelling::Header).as_deref(),
        Some("y")
    );
    let first = |text: &str| {
        let doc = Doc::parse("x.md", text);
        field_statements(&doc, 0, doc.line_count(), None)
    };
    assert_eq!(first("- Then:\n  - a\n    b\n  - c\n"), ["a b", "c"]);
    let wrapped = |text: &str| {
        let doc = Doc::parse("x.md", text);
        field_statements(&doc, 0, doc.line_count(), Some("v".to_owned()))
    };
    assert_eq!(wrapped("- Then: v\n  1. a\n"), ["v", "a"]);
    assert_eq!(
        targets_of("[s]:\n\\\n"),
        ["\\"],
        "a backslash before a line end escapes nothing"
    );
    let table = |text: &str| shown_lines(text);
    assert_eq!(
        table("a\n--- | ---\n")[1].as_deref(),
        Some("--- | ---"),
        "widths differ"
    );
    assert_eq!(table("---\n---\n"), [None, None]);
    assert_eq!(table("a\n---\n")[1], None);
}

/// A title stands first, at level one, and holds text. A part opens with one too.
#[test]
fn a_title_is_a_first_level_one_heading_with_text() {
    let fixture = Fixture::new();
    fixture.replace(SINGLE, "# Spec: single\n", "## Spec: single\n");
    assert!(fixture.rules().contains("FIELDS"), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    fixture.replace(SINGLE, "# Spec: single\n", "#\n");
    assert!(fixture.rules().contains("FIELDS"), "{:?}", fixture.rules());
    let fixture = Fixture::new();
    fixture.replace(SINGLE, "# Spec: single\n", "<!-- x -->\n# Spec: single\n");
    assert!(fixture.rules().contains("FIELDS"), "{:?}", fixture.rules());
    let design = format!("{DIR}/design.md");
    let fixture = Fixture::new();
    fixture.replace(&design, "# Spec: directory: design\n", "### design\n");
    assert!(
        fixture.rules().contains("PART-HEADER"),
        "{:?}",
        fixture.rules()
    );
    let fixture = Fixture::new();
    fixture.replace(
        &design,
        "# Spec: directory: design\n",
        "text\n\n# Spec: directory\n",
    );
    assert!(
        fixture.rules().contains("PART-HEADER"),
        "{:?}",
        fixture.rules()
    );
    let fixture = Fixture::new();
    fixture.replace(
        &design,
        "Part of: [index](index.md)",
        "Part of: [other](index.md)",
    );
    assert_only(
        &fixture,
        Rule::PartHeader,
        "a back-link that names something else",
    );
    let index = format!("{DIR}/index.md");
    let fixture = Fixture::new();
    fixture.replace(&index, PART_TABLE_HEADER, "| Part | Owns |");
    assert!(
        fixture.rules().contains("PART-TABLE"),
        "{:?}",
        fixture.rules()
    );
    let fixture = Fixture::new();
    link_from_design(&fixture, "index.md#1-goal--non-goals");
    fixture.replace(
        &design,
        "## 4. Design",
        "## 4. Design\n\n[^1]: a\n\n[^1]: b\n",
    );
    assert_only(&fixture, Rule::Link, "a repeated footnote label");
}

/// A citation is a rendered code span, wherever it renders and however the line spells it.
#[test]
fn a_citation_is_read_from_every_rendered_span() {
    for span in [
        format!("[`{CITED_FILE}:99`](index.md)"),
        format!("**`{CITED_FILE}:99`**"),
        format!("<span>`{CITED_FILE}:99`</span>"),
        format!("`{CITED_FILE}:99` <!-- x -->"),
    ] {
        let fixture = Fixture::new();
        cite_in_design(&fixture, &span);
        assert_only(&fixture, Rule::CiteLine, &span);
    }
    let fixture = Fixture::new();
    let index = format!("{DIR}/index.md");
    let cell = format!("| `{CITED_FILE}:99` |");
    fixture.replace(&index, "| D-1 and E-1 |", &cell);
    assert_only(&fixture, Rule::CiteLine, "a table cell");
    for span in [
        format!("\\`{CITED_FILE}:99\\`"),
        format!("`{CITED_FILE}&#58;99`"),
        format!("~~`{CITED_FILE}:99`~~"),
        format!("<!-- `{CITED_FILE}:99` -->"),
        format!("&#96;{CITED_FILE}:99&#96;"),
    ] {
        let fixture = Fixture::new();
        cite_in_design(&fixture, &span);
        let report = fixture.run();
        assert!(report.findings.is_empty(), "{span}: {:?}", report.findings);
    }
}
