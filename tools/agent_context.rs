//! KEL-147 deterministic agent-instruction inventory, routing, and byte-budget gate.
//!
//! This is std-only and outside the Cargo workspace. Compile with:
//! `rustc --edition=2024 -D warnings tools/agent_context.rs`.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

mod markdown_contract;
use markdown_contract::{fence_marker, without_inline_code, without_struck_text};

const MANIFEST: &str = ".agents/instruction-budget.tsv";
const ROOT: &str = "AGENTS.md";
const ROUTER: &str = ".agents/index.md";
#[cfg(test)]
const WORKFLOW: &str = "docs/agents/workflow.md";
const EVIDENCE_LOG: &str = "docs/agents/learnings.md";
const CLAUDE: &str = "CLAUDE.md";
const ROOT_MAX: usize = 16 * 1024;
const NESTED_MAX: usize = 4 * 1024;
const CHAIN_MAX: usize = 24 * 1024;
const ROUTER_MAX: usize = 4 * 1024;
const ROUTED_MAX: usize = 16 * 1024;
const EVIDENCE_MAX: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LoadClass {
    Always,
    Routed,
    Evidence,
}

impl LoadClass {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "always" => Some(Self::Always),
            "routed" => Some(Self::Routed),
            "evidence" => Some(Self::Evidence),
            _ => None,
        }
    }
}

fn is_agent_file(path: &str) -> bool {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "AGENTS.md" || name == "AGENTS.override.md")
}

fn expected_class(path: &str) -> LoadClass {
    if path == CLAUDE || is_agent_file(path) {
        LoadClass::Always
    } else if path == EVIDENCE_LOG {
        LoadClass::Evidence
    } else {
        LoadClass::Routed
    }
}

#[derive(Clone, Debug)]
struct Entry {
    path: String,
    class: LoadClass,
    max_bytes: usize,
    owner: String,
    trigger: String,
}

fn read(root: &Path, relative: &str) -> Result<String, String> {
    let path = root.join(relative);
    fs::read_to_string(&path).map_err(|error| {
        format!(
            "AGENT-CONTEXT: cannot read `{}`: {error}. Restore the tracked instruction owner.",
            path.display()
        )
    })
}

fn parse_manifest(root: &Path) -> Result<Vec<Entry>, String> {
    let text = read(root, MANIFEST)?;
    let mut entries = Vec::new();
    let mut paths = BTreeSet::new();
    let mut owners = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 5 {
            return Err(format!(
                "AGENT-CONTEXT: `{MANIFEST}` line {} must have 5 tab-separated fields: path, class, max_bytes, owner, trigger.",
                index + 1
            ));
        }
        let Some(class) = LoadClass::parse(fields[1]) else {
            return Err(format!(
                "AGENT-CONTEXT: `{MANIFEST}` line {} has unknown class `{}`; use always, routed, or evidence.",
                index + 1,
                fields[1]
            ));
        };
        let expected = expected_class(fields[0]);
        if class != expected {
            return Err(format!(
                "AGENT-CONTEXT: `{}` must be class {expected:?}, not {class:?}; file role determines load class.",
                fields[0]
            ));
        }
        let max_bytes = fields[2].parse::<usize>().map_err(|error| {
            format!(
                "AGENT-CONTEXT: `{MANIFEST}` line {} max_bytes is invalid: {error}.",
                index + 1
            )
        })?;
        if !paths.insert(fields[0].to_owned()) {
            return Err(format!(
                "AGENT-CONTEXT: `{MANIFEST}` duplicates path `{}`. One file has one inventory row.",
                fields[0]
            ));
        }
        if !owners.insert(fields[3].to_owned()) {
            return Err(format!(
                "AGENT-CONTEXT: `{MANIFEST}` duplicates canonical owner `{}`. Split the concept or point consumers at one owner.",
                fields[3]
            ));
        }
        entries.push(Entry {
            path: fields[0].to_owned(),
            class,
            max_bytes,
            owner: fields[3].to_owned(),
            trigger: fields[4].to_owned(),
        });
    }
    if entries.is_empty() {
        return Err(format!(
            "AGENT-CONTEXT: `{MANIFEST}` is empty. Inventory every instruction file."
        ));
    }
    Ok(entries)
}

fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "competitors" | "node_modules" | ".bun" | "docs/research"
    )
}

fn walk(root: &Path, directory: &Path, files: &mut BTreeSet<String>) -> Result<(), String> {
    for item in fs::read_dir(directory).map_err(|error| {
        format!(
            "AGENT-CONTEXT: cannot scan `{}`: {error}. Fix permissions before claiming the inventory is complete.",
            directory.display()
        )
    })? {
        let item = item.map_err(|error| format!("AGENT-CONTEXT: directory entry failed: {error}"))?;
        let path = item.path();
        let file_type = item.file_type().map_err(|error| {
            format!(
                "AGENT-CONTEXT: cannot inspect `{}`: {error}. Fix the filesystem entry before checking instruction inventory.",
                path.display()
            )
        })?;
        if file_type.is_symlink() {
            return Err(format!(
                "AGENT-CONTEXT: symlinked path `{}` is forbidden because instruction discovery must not escape or loop. Use a real repository path.",
                path.display()
            ));
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| format!("AGENT-CONTEXT: path escape: {error}"))?;
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        if file_type.is_dir() {
            if should_skip_dir(&relative_text)
                || path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(should_skip_dir)
            {
                continue;
            }
            walk(root, &path, files)?;
            continue;
        }
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let instruction = name == "AGENTS.md"
            || name == "AGENTS.override.md"
            || relative_text == CLAUDE
            || (relative_text.starts_with(".agents/")
                && (name.ends_with(".md") || name.ends_with(".txt")))
            || (relative_text.starts_with("docs/agents/") && name.ends_with(".md"));
        if instruction {
            files.insert(relative_text);
        }
    }
    Ok(())
}

fn discover(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut files = BTreeSet::new();
    walk(root, root, &mut files)?;
    Ok(files)
}

fn strip_html_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        output.push_str(&rest[..start]);
        let comment = &rest[start + 4..];
        let Some(end) = comment.find("-->") else {
            output.extend(comment.chars().filter(|character| *character == '\n'));
            return output;
        };
        output.extend(
            comment[..end]
                .chars()
                .filter(|character| *character == '\n'),
        );
        rest = &comment[end + 3..];
    }
    output.push_str(rest);
    output
}

fn active_table_lines(text: &str, heading: &str) -> Result<Vec<String>, String> {
    let uncommented = strip_html_comments(text);
    let mut in_section = false;
    let mut seen_header = false;
    let mut seen_delimiter = false;
    let mut fence: Option<(u8, usize)> = None;
    let mut lines = Vec::new();
    let mut block_quote = false;
    for raw in uncommented.lines() {
        let trimmed = raw.trim_start();
        let marker = fence_marker(raw);
        if let Some((active, width)) = fence {
            if marker.is_some_and(|(candidate, candidate_width, closing)| {
                candidate == active && candidate_width >= width && closing
            }) {
                fence = None;
            }
            continue;
        }
        if let Some((opening, width, _)) = marker {
            if seen_delimiter && !lines.is_empty() {
                break;
            }
            fence = Some((opening, width));
            continue;
        }
        if trimmed.is_empty() {
            block_quote = false;
            if seen_delimiter && !lines.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.starts_with('>') {
            if seen_delimiter && !lines.is_empty() {
                break;
            }
            block_quote = true;
            continue;
        }
        if block_quote {
            continue;
        }
        if raw == heading {
            in_section = true;
            continue;
        }
        if in_section && raw.starts_with("## ") {
            break;
        }
        if in_section && (trimmed.contains('<') || trimmed.contains('>')) {
            return Err(format!(
                "AGENT-CONTEXT: `{ROUTER}` task-routing section must not use raw HTML; hidden content is not an executable route."
            ));
        }
        if !in_section {
            continue;
        }
        if !seen_header {
            if trimmed == "| Task or path | Read |" {
                seen_header = true;
                continue;
            }
            return Err(format!(
                "AGENT-CONTEXT: `{ROUTER}` has no visible row in one canonical `| Task or path | Read |` routing table."
            ));
        }
        if !seen_delimiter {
            if trimmed == "|---|---|" {
                seen_delimiter = true;
                continue;
            }
            return Err(format!(
                "AGENT-CONTEXT: `{ROUTER}` has no visible row after the canonical routing-table delimiter."
            ));
        }
        if trimmed.starts_with('|') {
            lines.push(without_inline_code(&without_struck_text(trimmed)));
        } else {
            break;
        }
    }
    if !seen_header || !seen_delimiter || lines.is_empty() {
        return Err(format!(
            "AGENT-CONTEXT: `{ROUTER}` has no visible row in one contiguous canonical task-routing table."
        ));
    }
    Ok(lines)
}

fn route_visible(router: &str, entry: &Entry) -> Result<bool, String> {
    let table = active_table_lines(router, "## Task routing")?;
    let destination = if let Some(relative) = entry.path.strip_prefix(".agents/") {
        relative.to_owned()
    } else if entry.path.starts_with("docs/agents/") {
        format!("../{}", entry.path)
    } else {
        entry.path.clone()
    };
    let exact_link = format!("({destination})");
    let labels = entry
        .trigger
        .strip_prefix("route:")
        .unwrap_or(&entry.trigger)
        .split("-or-")
        .collect::<Vec<_>>();
    Ok(table.iter().any(|line| {
        let normalized = line.to_ascii_lowercase();
        line.contains(&exact_link) && labels.iter().any(|label| normalized.contains(label))
    }))
}

fn valid_trigger_slug(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn validate_routed_trigger(root: &Path, router: &str, entry: &Entry) -> Result<(), String> {
    if entry.path == ROUTER {
        return (entry.trigger == "root:instruction-loading")
            .then_some(())
            .ok_or_else(|| {
                format!(
                    "AGENT-CONTEXT: `{ROUTER}` must use exact trigger `root:instruction-loading`."
                )
            });
    }

    if entry.path.starts_with(".agents/skills/") {
        let label = entry.trigger.strip_prefix("skill:").ok_or_else(|| {
            format!(
                "AGENT-CONTEXT: skill `{}` must use closed `skill:<name>` trigger format.",
                entry.path
            )
        })?;
        if !valid_trigger_slug(label) {
            return Err(format!(
                "AGENT-CONTEXT: skill `{}` has invalid trigger `{}`; use lowercase letters, digits, and hyphens.",
                entry.path, entry.trigger
            ));
        }
        if entry.path.ends_with("/SKILL.md") {
            let text = read(root, &entry.path)?;
            let name_line = format!("name: {label}");
            if !text.lines().any(|line| line.trim() == name_line) {
                return Err(format!(
                    "AGENT-CONTEXT: skill `{}` trigger `{}` does not match its frontmatter `name:`.",
                    entry.path, entry.trigger
                ));
            }
            return Ok(());
        }

        let path = Path::new(&entry.path);
        let parent = path.parent().ok_or_else(|| {
            format!(
                "AGENT-CONTEXT: skill reference `{}` has no owner directory.",
                entry.path
            )
        })?;
        let owner_name = parent
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "AGENT-CONTEXT: skill reference `{}` has no UTF-8 owner.",
                    entry.path
                )
            })?;
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "AGENT-CONTEXT: skill reference `{}` has no UTF-8 filename.",
                    entry.path
                )
            })?;
        let owner_path = parent.join("SKILL.md");
        let owner_relative = owner_path.to_string_lossy().replace('\\', "/");
        let owner = read(root, &owner_relative)?;
        let direct_link = format!("({filename})");
        let relative_link = format!("(./{filename})");
        if !label.starts_with(owner_name)
            || !(owner.contains(&direct_link) || owner.contains(&relative_link))
        {
            return Err(format!(
                "AGENT-CONTEXT: skill reference `{}` trigger `{}` must name and be linked by owning `{owner_relative}`.",
                entry.path, entry.trigger
            ));
        }
        return Ok(());
    }

    let label = entry.trigger.strip_prefix("route:").ok_or_else(|| {
        format!(
            "AGENT-CONTEXT: routed `{}` must use closed `route:<task>` trigger format.",
            entry.path
        )
    })?;
    if !label.split("-or-").all(valid_trigger_slug) || !route_visible(router, entry)? {
        return Err(format!(
            "AGENT-CONTEXT: routed `{}` trigger `{}` has no visible row with a matching exact link/task in `{ROUTER}`.",
            entry.path, entry.trigger
        ));
    }
    Ok(())
}

fn logical_markdown_block(lines: &[&str], index: usize) -> String {
    fn list_item(line: &str) -> bool {
        let trimmed = line.trim_start();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
            return true;
        }
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        digits > 0 && trimmed[digits..].starts_with(". ")
    }

    let mut start = index;
    if !list_item(lines[start]) {
        while start > 0 && !lines[start - 1].trim().is_empty() {
            start -= 1;
            if list_item(lines[start]) {
                break;
            }
        }
    }
    let mut end = index + 1;
    while end < lines.len() && !lines[end].trim().is_empty() {
        if end > start && list_item(lines[end]) {
            break;
        }
        end += 1;
    }
    lines[start..end].join(" ")
}

fn has_meaningful_instruction(text: &str) -> bool {
    let uncommented = strip_html_comments(text);
    let mut front_matter = false;
    let mut front_matter_seen = false;
    let mut fence: Option<(u8, usize)> = None;
    for raw in uncommented.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !front_matter_seen && trimmed == "---" {
            front_matter = true;
            front_matter_seen = true;
            continue;
        }
        if front_matter {
            if trimmed == "---" {
                front_matter = false;
            }
            continue;
        }
        front_matter_seen = true;
        let marker = fence_marker(raw);
        if let Some((active, opening_width)) = fence {
            if marker.is_some_and(|(candidate, candidate_width, closing)| {
                candidate == active && candidate_width >= opening_width && closing
            }) {
                fence = None;
            }
            continue;
        }
        if let Some((opening, width, _)) = marker {
            fence = Some((opening, width));
            continue;
        }
        if !trimmed.starts_with('#') && trimmed != "---" {
            return true;
        }
    }
    false
}

fn validate_entry(root: &Path, entry: &Entry) -> Result<usize, String> {
    if entry.max_bytes == 0 || entry.owner.trim().is_empty() || entry.trigger.trim().is_empty() {
        return Err(format!(
            "AGENT-CONTEXT: `{}` has a hollow manifest contract. Set owner, trigger, and a positive budget.",
            entry.path
        ));
    }
    match entry.class {
        LoadClass::Always => {
            if entry.path == ROOT && entry.max_bytes > ROOT_MAX {
                return Err(format!(
                    "AGENT-CONTEXT: root budget {} exceeds hard {} bytes; route conditional policy instead of widening.",
                    entry.max_bytes, ROOT_MAX
                ));
            }
            if entry.path.ends_with("/AGENTS.md") && entry.max_bytes > NESTED_MAX {
                return Err(format!(
                    "AGENT-CONTEXT: nested `{}` budget {} exceeds hard {} bytes.",
                    entry.path, entry.max_bytes, NESTED_MAX
                ));
            }
        }
        LoadClass::Routed if entry.max_bytes > ROUTED_MAX => {
            return Err(format!(
                "AGENT-CONTEXT: routed `{}` budget {} exceeds hard {} bytes. Use progressive disclosure.",
                entry.path, entry.max_bytes, ROUTED_MAX
            ));
        }
        LoadClass::Evidence if entry.max_bytes > EVIDENCE_MAX => {
            return Err(format!(
                "AGENT-CONTEXT: evidence `{}` budget {} exceeds hard {} bytes. Compact/archive it.",
                entry.path, entry.max_bytes, EVIDENCE_MAX
            ));
        }
        _ => {}
    }
    let text = read(root, &entry.path)?;
    let bytes = text.len();
    if entry.path != CLAUDE && (bytes < 32 || !has_meaningful_instruction(&text)) {
        return Err(format!(
            "AGENT-CONTEXT: `{}` is hollow ({bytes} bytes or no meaningful instruction outside comments/headings/fences). Remove it or add a real invariant/route.",
            entry.path
        ));
    }
    if bytes > entry.max_bytes {
        return Err(format!(
            "AGENT-CONTEXT: `{}` is {bytes} bytes, over its {} byte budget. Move conditional detail; do not raise the budget without approved measured evidence.",
            entry.path, entry.max_bytes
        ));
    }
    Ok(bytes)
}

fn check(root: &Path) -> Result<Vec<(Entry, usize)>, String> {
    let entries = parse_manifest(root)?;
    let manifest_paths = entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<BTreeSet<_>>();
    let discovered = discover(root)?;
    if let Some(path) = discovered.iter().find(|path| {
        Path::new(path.as_str())
            .file_name()
            .and_then(|name| name.to_str())
            == Some("AGENTS.override.md")
    }) {
        return Err(format!(
            "AGENT-CONTEXT: `{path}` is forbidden. Fix the canonical root/nested owner; an override can silently replace its automatic instruction chain."
        ));
    }
    if manifest_paths != discovered {
        let missing = discovered
            .difference(&manifest_paths)
            .cloned()
            .collect::<Vec<_>>();
        let stale = manifest_paths
            .difference(&discovered)
            .cloned()
            .collect::<Vec<_>>();
        return Err(format!(
            "AGENT-CONTEXT: instruction inventory mismatch. Add unknown files to `{MANIFEST}` or remove stale rows. unknown={missing:?} stale={stale:?}"
        ));
    }
    let router = read(root, ROUTER)?;
    if router.len() > ROUTER_MAX {
        return Err(format!(
            "AGENT-CONTEXT: `{ROUTER}` is {} bytes, over hard {ROUTER_MAX}; keep it routing-only.",
            router.len()
        ));
    }
    let mut measured = Vec::new();
    let mut sizes = BTreeMap::new();
    for entry in entries {
        let bytes = validate_entry(root, &entry)?;
        if entry.class == LoadClass::Routed {
            validate_routed_trigger(root, &router, &entry)?;
        }
        sizes.insert(entry.path.clone(), bytes);
        measured.push((entry, bytes));
    }
    let root_bytes = *sizes.get(ROOT).ok_or_else(|| {
        format!("AGENT-CONTEXT: `{MANIFEST}` must inventory root `{ROOT}` as always.")
    })?;
    for path in sizes.keys() {
        if path.ends_with("/AGENTS.md") {
            let directory = Path::new(path).parent().ok_or_else(|| {
                format!("AGENT-CONTEXT: nested instruction `{path}` has no parent.")
            })?;
            let chain_bytes = root_bytes
                + sizes
                    .iter()
                    .filter(|(candidate, _)| {
                        candidate.ends_with("/AGENTS.md")
                            && Path::new(candidate.as_str())
                                .parent()
                                .is_some_and(|parent| directory.starts_with(parent))
                    })
                    .map(|(_, bytes)| *bytes)
                    .sum::<usize>();
            if chain_bytes > CHAIN_MAX {
                return Err(format!(
                    "AGENT-CONTEXT: automatic chain ending at `{path}` is {chain_bytes} bytes, over hard {CHAIN_MAX}. Every root-to-working-directory instruction chain must fit without raising Codex discovery limits."
                ));
            }
        }
    }
    if read(root, CLAUDE)?.trim() != "@AGENTS.md" {
        return Err(format!(
            "AGENT-CONTEXT: `{CLAUDE}` must remain the single `@AGENTS.md` alias, not a second policy owner."
        ));
    }
    let evidence = measured
        .iter()
        .filter(|(entry, _)| entry.class == LoadClass::Evidence)
        .map(|(entry, _)| entry.path.as_str())
        .collect::<Vec<_>>();
    for (consumer, _) in &measured {
        if consumer.class == LoadClass::Evidence {
            continue;
        }
        let text = read(root, &consumer.path)?;
        let lines = text.lines().collect::<Vec<_>>();
        for path in &evidence {
            for (index, _) in lines
                .iter()
                .enumerate()
                .filter(|(_, line)| line.contains(path))
            {
                let normalized = logical_markdown_block(&lines, index).to_ascii_lowercase();
                let read_or_load = normalized.contains("read") || normalized.contains("load");
                let whole_file = ["full", "complete", "entire", "whole"]
                    .iter()
                    .any(|needle| normalized.contains(needle));
                let forbidden_full_read = read_or_load && whole_file;
                let bounded = [
                    "query",
                    "relevant-area",
                    "relevant area",
                    "grep",
                    "search",
                    "slice",
                    "never full-read",
                    "append",
                    "belongs",
                    "not default context",
                ]
                .iter()
                .any(|needle| normalized.contains(needle));
                if forbidden_full_read || !bounded {
                    return Err(format!(
                        "AGENT-CONTEXT: `{}` makes evidence `{path}` look like a full read near line {}. Route a bounded query/slice instead.",
                        consumer.path,
                        index + 1
                    ));
                }
            }
        }
    }
    Ok(measured)
}

fn run_cli() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        "AGENT-CONTEXT: missing command. Run `agent-context check [workspace]`.".to_owned()
    })?;
    let root = args
        .next()
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    if args.next().is_some() || command != "check" {
        return Err("AGENT-CONTEXT: use exactly `agent-context check [workspace]`.".to_owned());
    }
    let measured = check(&root)?;
    for (entry, bytes) in measured {
        println!("{bytes}\t{:?}\t{}", entry.class, entry.path);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run_cli() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                env::temp_dir().join(format!("keld-agent-context-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create fixture root");
            Self { path }
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.path.join(relative);
            fs::create_dir_all(path.parent().expect("fixture parent")).expect("mkdir fixture");
            fs::write(path, contents).expect("write fixture");
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn manifest(root_max: usize, routed_max: usize) -> String {
        format!(
            "# path\tclass\tmax_bytes\towner\ttrigger\n\
AGENTS.md\talways\t{root_max}\troot\talways\n\
CLAUDE.md\talways\t64\tclaude\tclaude-session\n\
crates/example/AGENTS.md\talways\t4096\texample\tpath:crates/example\n\
.agents/index.md\trouted\t4096\trouter\troot:instruction-loading\n\
.agents/testing.md\trouted\t{routed_max}\ttesting\troute:testing\n\
docs/agents/workflow.md\trouted\t4096\tworkflow\troute:coordination\n\
docs/agents/learnings.md\tevidence\t4096\tlearnings\tquery:area\n"
        )
    }

    fn fixture() -> TempDir {
        let temp = TempDir::new();
        temp.write(MANIFEST, &manifest(1024, 1024));
        temp.write(
            ROOT,
            &format!(
                "# Root\n\nquery relevant-area `{}` evidence\n",
                "docs/agents/learnings.md"
            ),
        );
        temp.write(CLAUDE, "@AGENTS.md\n");
        temp.write(
            "crates/example/AGENTS.md",
            "# Example\n\nPath-local invariant that cannot be hollow and must remain independently enforceable.\n",
        );
        temp.write(
            ROUTER,
            "# Router\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| Tests | [`testing.md`](testing.md) |\n| Coordination | [`workflow.md`](../docs/agents/workflow.md) |\n\nquery relevant-area `docs/agents/learnings.md` evidence\n",
        );
        temp.write(
            ".agents/testing.md",
            "# Testing\n\nA routed test invariant with enough content to be real.\n",
        );
        temp.write(
            WORKFLOW,
            "# Workflow\n\nquery relevant-area `docs/agents/learnings.md`; append only deduped facts.\n",
        );
        temp.write(
            "docs/agents/learnings.md",
            "# Learnings\n\n- [test] evidence entry.\n",
        );
        temp
    }

    #[test]
    fn complete_inventory_passes() {
        let temp = fixture();
        check(&temp.path).expect("complete fixture");
    }

    #[test]
    fn root_max_plus_one_fails() {
        let temp = fixture();
        temp.write(ROOT, &"x".repeat(1025));
        let error = check(&temp.path).expect_err("over budget root");
        assert!(error.contains("over its 1024 byte budget"), "{error}");
    }

    #[test]
    fn unknown_and_stale_files_fail() {
        let temp = fixture();
        temp.write(
            ".agents/untracked.md",
            "# Unknown\n\nA real-looking hidden instruction.\n",
        );
        let error = check(&temp.path).expect_err("unknown file");
        assert!(error.contains("unknown"), "{error}");

        let temp = fixture();
        fs::remove_file(temp.path.join(".agents/testing.md")).expect("remove routed owner");
        let error = check(&temp.path).expect_err("stale row");
        assert!(error.contains("stale"), "{error}");

        let temp = fixture();
        temp.write(
            ".agents/evil.txt",
            "This linked text is still agent instruction content and needs a budget row.\n",
        );
        let error = check(&temp.path).expect_err("unknown text instruction");
        assert!(error.contains("unknown"), "{error}");
    }

    #[test]
    fn missing_or_hidden_route_fails() {
        let temp = fixture();
        temp.write(
            ROUTER,
            "# Router\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n",
        );
        let error = check(&temp.path).expect_err("missing route");
        assert!(error.contains("no visible row"), "{error}");

        for (label, decoy) in [
            ("fenced route", "```text\n| Tests | testing.md |\n```"),
            ("commented route", "<!-- | Tests | testing.md | -->"),
            ("inline commented route", "| Tests | <!-- testing.md --> |"),
            ("quoted route", "> historical\n| Tests | testing.md |"),
        ] {
            let temp = fixture();
            temp.write(
                ROUTER,
                &format!(
                    "# Router\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n{decoy}\n"
                ),
            );
            let error = check(&temp.path).expect_err(label);
            assert!(error.contains("no visible row"), "{error}");
        }

        let temp = fixture();
        temp.write(
            ROUTER,
            "# Router\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| Tests | [`not testing`](not-testing.md) |\n| Coordination | [`workflow`](../docs/agents/workflow.md) |\n\nquery relevant-area `docs/agents/learnings.md` evidence\n",
        );
        let error = check(&temp.path).expect_err("substring route to wrong file");
        assert!(error.contains("no visible row"), "{error}");

        let temp = fixture();
        temp.write(
            ROUTER,
            "# Router\n\n## Task routing\n\n````\n````not-a-valid-closing-fence\n| Tests | [`testing`](testing.md) |\n````\n| Task or path | Read |\n|---|---|\n| Coordination | [`workflow`](../docs/agents/workflow.md) |\n\nquery relevant-area `docs/agents/learnings.md` evidence\n",
        );
        let error = check(&temp.path).expect_err("invalid fence closer cannot expose route");
        assert!(error.contains("no visible row"), "{error}");

        let temp = fixture();
        temp.write(
            ROUTER,
            "# Router\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| Coordination | [`workflow`](../docs/agents/workflow.md) |\n\n| historical testing | [`testing`](testing.md) |\n",
        );
        let error = check(&temp.path).expect_err("stray row outside canonical table");
        assert!(error.contains("no visible row"), "{error}");

        for (label, hidden_link) in [
            ("struck route", "~~[`testing`](testing.md)~~"),
            ("inline-code route", "`[testing](testing.md)`"),
        ] {
            let temp = fixture();
            temp.write(
                ROUTER,
                &format!(
                    "# Router\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| Tests | {hidden_link} |\n| Coordination | [`workflow`](../docs/agents/workflow.md) |\n"
                ),
            );
            let error = check(&temp.path).expect_err(label);
            assert!(error.contains("no visible row"), "{error}");
        }
    }

    #[test]
    fn duplicate_owner_and_bad_class_budget_fail() {
        let temp = fixture();
        let duplicate = manifest(1024, 1024).replace("testing\troute", "root\troute");
        temp.write(MANIFEST, &duplicate);
        let error = check(&temp.path).expect_err("duplicate owner");
        assert!(error.contains("duplicates canonical owner"), "{error}");

        let temp = fixture();
        temp.write(MANIFEST, &manifest(ROOT_MAX + 1, 1024));
        let error = check(&temp.path).expect_err("hard root ceiling");
        assert!(error.contains("exceeds hard"), "{error}");

        let temp = fixture();
        let drifted = manifest(1024, 1024).replace(
            "AGENTS.md\talways\t1024\troot",
            "AGENTS.md\trouted\t1024\troot",
        );
        temp.write(MANIFEST, &drifted);
        let error = check(&temp.path).expect_err("automatic file class drift");
        assert!(error.contains("file role determines load class"), "{error}");

        let temp = fixture();
        let trigger_drift = manifest(1024, 1024).replace("route:testing", "route:docs");
        temp.write(MANIFEST, &trigger_drift);
        let error = check(&temp.path).expect_err("trigger must match owning route row");
        assert!(error.contains("no visible row"), "{error}");

        let temp = fixture();
        let open_trigger = manifest(1024, 1024).replace("route:testing", "testing");
        temp.write(MANIFEST, &open_trigger);
        let error = check(&temp.path).expect_err("non-prefixed routed trigger");
        assert!(error.contains("closed `route:<task>`"), "{error}");

        let temp = fixture();
        let mut skill_manifest = manifest(1024, 1024);
        skill_manifest
            .push_str(".agents/skills/sample/SKILL.md\trouted\t1024\tsample-skill\tskill:wrong\n");
        temp.write(MANIFEST, &skill_manifest);
        temp.write(
            ".agents/skills/sample/SKILL.md",
            "---\nname: sample\ndescription: Sample review skill.\n---\n\n# Sample\n\nApply one real instruction.\n",
        );
        let error = check(&temp.path).expect_err("skill trigger must match frontmatter");
        assert!(error.contains("does not match its frontmatter"), "{error}");
    }

    #[test]
    fn hollow_file_and_alias_drift_fail() {
        let temp = fixture();
        temp.write(".agents/testing.md", "# x\n");
        let error = check(&temp.path).expect_err("hollow routed file");
        assert!(error.contains("hollow"), "{error}");

        let temp = fixture();
        temp.write(
            ".agents/testing.md",
            "# Comment shell\n\n<!-- This comment is deliberately longer than thirty-two bytes. -->\n",
        );
        let error = check(&temp.path).expect_err("comment-only routed file");
        assert!(error.contains("hollow"), "{error}");

        let temp = fixture();
        temp.write(CLAUDE, "Second policy owner\n");
        let error = check(&temp.path).expect_err("alias drift");
        assert!(error.contains("single `@AGENTS.md` alias"), "{error}");
    }

    #[test]
    fn evidence_must_be_queried_not_full_read() {
        let temp = fixture();
        temp.write(
            ROOT,
            "# Root\n\nBefore work read `docs/agents/learnings.md` completely.\n",
        );
        let error = check(&temp.path).expect_err("full evidence read");
        assert!(error.contains("look like a full read"), "{error}");

        let temp = fixture();
        temp.write(
            ROOT,
            "# Root\n\nRead `docs/agents/learnings.md` in full as evidence.\n",
        );
        let error = check(&temp.path).expect_err("evidence word cannot mask full read");
        assert!(error.contains("look like a full read"), "{error}");

        let temp = fixture();
        temp.write(
            ".agents/testing.md",
            "# Testing\n\nRead `docs/agents/learnings.md` completely before tests; evidence is required.\n",
        );
        let error = check(&temp.path).expect_err("routed consumer full read");
        assert!(error.contains(".agents/testing.md"), "{error}");

        let temp = fixture();
        temp.write(
            ROOT,
            "# Root\n\nRead `docs/agents/learnings.md` completely, then search it for evidence.\n",
        );
        let error = check(&temp.path).expect_err("bounded verb cannot mask whole-file read");
        assert!(error.contains("look like a full read"), "{error}");
    }

    #[test]
    fn renamed_instruction_cannot_escape_inventory() {
        let temp = fixture();
        fs::rename(
            temp.path.join(".agents/testing.md"),
            temp.path.join(".agents/renamed.md"),
        )
        .expect("rename instruction");
        let error = check(&temp.path).expect_err("renamed file");
        assert!(error.contains("inventory mismatch"), "{error}");
    }

    #[test]
    fn override_and_raw_html_routes_are_forbidden() {
        let temp = fixture();
        temp.write(
            "crates/example/AGENTS.override.md",
            "# Override\n\nThis would replace the canonical automatic instruction owner.\n",
        );
        let error = check(&temp.path).expect_err("override must not replace canonical owner");
        assert!(error.contains("is forbidden"), "{error}");

        let temp = fixture();
        temp.write(
            ROUTER,
            "# Router\n\n## Task routing\n\n| Task or path | Read |\n|---|---|\n| Tests | <span hidden>testing.md</span> |\n| Coordination | workflow.md |\n\nquery relevant-area `docs/agents/learnings.md` evidence\n",
        );
        let error = check(&temp.path).expect_err("hidden HTML route must fail");
        assert!(error.contains("must not use raw HTML"), "{error}");
    }

    #[test]
    fn complete_automatic_chain_not_pairwise_sum_is_bounded() {
        let temp = fixture();
        let mut expanded = manifest(ROOT_MAX, 1024);
        for (index, path) in [
            "crates/example/one/AGENTS.md",
            "crates/example/one/two/AGENTS.md",
            "crates/example/one/two/three/AGENTS.md",
            "crates/example/one/two/three/four/AGENTS.md",
        ]
        .iter()
        .enumerate()
        {
            expanded.push_str(&format!(
                "{path}\talways\t4096\tnested-{index}\tpath:nested-{index}\n"
            ));
            temp.write(
                path,
                &format!("# Nested {index}\n\n{}\n", "binding rule ".repeat(290)),
            );
        }
        temp.write(MANIFEST, &expanded);
        temp.write(
            ROOT,
            &format!(
                "# Root\n\nquery relevant-area `docs/agents/learnings.md` evidence\n{}\n",
                "universal rule ".repeat(850)
            ),
        );
        let error = check(&temp.path).expect_err("full automatic chain must exceed budget");
        assert!(error.contains("automatic chain ending"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_instruction_files_and_directory_loops_are_rejected() {
        use std::os::unix::fs::symlink;

        let temp = fixture();
        symlink(
            temp.path.join(".agents/testing.md"),
            temp.path.join(".agents/testing-link.md"),
        )
        .expect("create instruction symlink");
        let error = check(&temp.path).expect_err("symlinked instruction file");
        assert!(error.contains("symlinked path"), "{error}");

        let temp = fixture();
        symlink(temp.path.join(".agents"), temp.path.join(".agents/loop"))
            .expect("create directory loop");
        let error = check(&temp.path).expect_err("symlinked directory loop");
        assert!(error.contains("symlinked path"), "{error}");
    }
}
