//! Read-only accessors for the pinned CI workflow shape.
//!
//! Kept private to ci_hygiene; semantic validation remains in the parent.

pub(super) fn yaml_content(line: &str) -> Option<(usize, &str)> {
    let indent = line.len() - line.trim_start().len();
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let content = trimmed
        .split_once(" #")
        .map_or(trimmed, |(before, _)| before)
        .trim_end();
    Some((indent, content))
}

pub(super) fn yaml_mapping_key(content: &str) -> Option<(String, &str)> {
    let content = content.strip_prefix("- ").unwrap_or(content);
    let (key, value) = content.split_once(':')?;
    Some((
        key.trim().trim_matches(['\'', '"']).to_owned(),
        value.trim(),
    ))
}

pub(super) fn forbidden_workflow_control_key(text: &str) -> Option<String> {
    let mut run_block_indent = None;
    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if let Some(run_indent) = run_block_indent {
            if indent > run_indent {
                continue;
            }
            run_block_indent = None;
        }
        let Some((key, value)) = yaml_mapping_key(content) else {
            continue;
        };
        if key == "run" && (value.starts_with('|') || value.starts_with('>')) {
            run_block_indent = Some(indent);
            continue;
        }
        if key == "continue-on-error" || key == "defaults" {
            return Some(key);
        }
    }
    None
}

pub(super) fn workflow_has_checkout_fetch_depth_zero(text: &str) -> bool {
    let mut checkout_indent = None;
    let mut with_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content.starts_with("- uses:") {
            checkout_indent = content.contains("actions/checkout@").then_some(indent);
            with_indent = None;
            continue;
        }
        let Some(checkout) = checkout_indent else {
            continue;
        };
        if indent <= checkout {
            checkout_indent = None;
            with_indent = None;
            continue;
        }
        if content == "with:" {
            with_indent = Some(indent);
            continue;
        }
        if let Some(with) = with_indent {
            if indent <= with {
                with_indent = None;
            } else if let Some(value) = content.strip_prefix("fetch-depth:") {
                let value = value
                    .trim()
                    .trim_matches(|character| character == '\'' || character == '"');
                if value == "0" {
                    return true;
                }
            }
        }
    }
    false
}

/// The named step's whole block must carry exactly `run: <command>` and no
/// `if` key at any position: a condition after `run:` skips the step just as
/// one before it does (#650 review).
pub(super) fn workflow_has_unconditional_named_step(
    text: &str,
    step_name: &str,
    command: &str,
) -> bool {
    let expected_name = format!("- name: {step_name}");
    let expected_command = format!("run: {command}");
    let mut step_indent = None;
    let mut found_command = false;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        let Some(step) = step_indent else {
            if content == expected_name {
                step_indent = Some(indent);
            }
            continue;
        };
        if indent <= step {
            break;
        }
        if indent == step + 2 {
            if content == expected_command {
                found_command = true;
            } else if yaml_mapping_key(content).is_some_and(|(key, _)| key == "if") {
                return false;
            }
        }
    }
    found_command
}

pub(super) fn workflow_named_step_has_property(
    text: &str,
    step_name: &str,
    property: &str,
) -> bool {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content == property {
            return true;
        }
    }
    false
}

pub(super) fn workflow_named_step_contains(text: &str, step_name: &str, needle: &str) -> bool {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == expected_name {
            step_indent = Some(indent);
            continue;
        }
        let Some(step) = step_indent else {
            continue;
        };
        if indent <= step {
            return false;
        }
        if content.contains(needle) {
            return true;
        }
    }
    false
}

pub(super) fn workflow_job_block<'a>(text: &'a str, job_name: &str) -> Option<String> {
    let target = format!("{job_name}:");
    let mut found = false;
    let mut block = String::new();

    for line in text.lines() {
        let parsed = yaml_content(line);
        if !found {
            if matches!(parsed, Some((2, ref content)) if content == &target) {
                found = true;
                block.push_str(line);
                block.push('\n');
            }
            continue;
        }
        if matches!(parsed, Some((indent, _)) if indent <= 2) {
            break;
        }
        block.push_str(line);
        block.push('\n');
    }

    found.then_some(block)
}

pub(super) fn workflow_direct_named_step_block(text: &str, step_name: &str) -> Option<String> {
    let expected_name = format!("- name: {step_name}");
    let mut found = false;
    let mut block = String::new();

    for line in text.lines() {
        let parsed = yaml_content(line);
        if !found {
            if matches!(parsed, Some((6, ref content)) if content == &expected_name) {
                found = true;
                block.push_str(line);
                block.push('\n');
            }
            continue;
        }
        if matches!(parsed, Some((indent, _)) if indent <= 6) {
            break;
        }
        block.push_str(line);
        block.push('\n');
    }

    found.then_some(block)
}

pub(super) fn workflow_direct_named_step_count(text: &str, step_name: &str) -> usize {
    let expected_name = format!("- name: {step_name}");
    text.lines()
        .filter_map(yaml_content)
        .filter(|(indent, content)| *indent == 6 && *content == expected_name)
        .count()
}

pub(super) fn workflow_job_sequence_values(block: &str, key: &str) -> Option<Vec<String>> {
    let expected_key = format!("{key}:");
    let mut sequence_indent = None;
    let mut values = Vec::new();

    for line in block.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if sequence_indent.is_none() {
            if indent == 4 && content == expected_key {
                sequence_indent = Some(indent);
            }
            continue;
        }
        let sequence = sequence_indent?;
        if indent <= sequence {
            break;
        }
        if indent == sequence + 2 {
            values.push(content.strip_prefix("- ")?.to_owned());
        }
    }

    sequence_indent.map(|_| values)
}

pub(super) fn workflow_named_step_mapping(
    text: &str,
    step_name: &str,
    mapping: &str,
) -> Option<Vec<(String, String)>> {
    let block = workflow_direct_named_step_block(text, step_name)?;
    let mut found = false;
    let mut entries = Vec::new();
    for line in block.lines().skip(1) {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if !found {
            if indent == 8 {
                let (key, value) = yaml_mapping_key(content)?;
                if key == mapping {
                    if !value.is_empty() {
                        return None;
                    }
                    found = true;
                }
            }
            continue;
        }
        if indent <= 8 {
            break;
        }
        if indent != 10 {
            return None;
        }
        let (key, value) = yaml_mapping_key(content)?;
        if entries.iter().any(|(existing, _)| existing == &key) {
            return None;
        }
        entries.push((key, value.trim_matches(['\'', '"']).to_owned()));
    }
    found.then_some(entries)
}

pub(super) fn workflow_named_step_shell_commands(
    text: &str,
    step_name: &str,
) -> Option<Vec<String>> {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;
    let mut run_indent = None;
    let mut command_text = None::<String>;
    let mut commands = Vec::new();

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if step_indent.is_none() {
            if content == expected_name {
                step_indent = Some(indent);
            }
            continue;
        }
        let step = step_indent?;
        if indent <= step {
            break;
        }
        if run_indent.is_none() {
            if content.starts_with("run: |") {
                run_indent = Some(indent);
                continue;
            }
            continue;
        }
        let run = run_indent?;
        if indent <= run {
            break;
        }
        if let Some(current) = command_text.as_mut() {
            current.push(' ');
            current.push_str(content.trim_end_matches('\\').trim_end());
            if !content.ends_with('\\') {
                commands.push(command_text.take()?);
            }
            continue;
        }
        let continued = content.ends_with('\\');
        let command = content.trim_end_matches('\\').trim_end().to_owned();
        if continued {
            command_text = Some(command);
        } else {
            commands.push(command);
        }
    }
    if let Some(command) = command_text {
        if command.is_empty() {
            return None;
        }
        commands.push(command);
    }
    run_indent.map(|_| commands)
}

pub(super) fn workflow_job_level_property(block: &str, key: &str) -> Option<String> {
    let expected_key = format!("{key}:");
    for line in block.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == "steps:" {
            return None;
        }
        if indent == 4 {
            if let Some(value) = content.strip_prefix(&expected_key) {
                return Some(value.trim().to_owned());
            }
        }
    }
    None
}

pub(super) fn workflow_named_step_direct_keys(text: &str, step_name: &str) -> Option<Vec<String>> {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;
    let mut keys = Vec::new();

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if step_indent.is_none() {
            if content == expected_name {
                step_indent = Some(indent);
            }
            continue;
        }
        let step = step_indent?;
        if indent <= step {
            break;
        }
        if indent == step + 2 {
            let (key, _) = content.split_once(':')?;
            keys.push(key.trim().trim_matches(['\'', '"']).to_owned());
        }
    }
    step_indent.map(|_| keys)
}

pub(super) fn workflow_named_step_direct_value(
    text: &str,
    step_name: &str,
    key: &str,
) -> Option<String> {
    let expected_name = format!("- name: {step_name}");
    let mut step_indent = None;

    for line in text.lines() {
        let Some((indent, content)) = yaml_content(line) else {
            continue;
        };
        if step_indent.is_none() {
            if content == expected_name {
                step_indent = Some(indent);
            }
            continue;
        }
        let step = step_indent?;
        if indent <= step {
            break;
        }
        if indent == step + 2 {
            let (candidate, value) = yaml_mapping_key(content)?;
            if candidate == key {
                return Some(value.to_owned());
            }
        }
    }
    None
}

pub(super) fn workflow_job_level_if(block: &str) -> Option<String> {
    for line in block.lines() {
        let Some((_indent, content)) = yaml_content(line) else {
            continue;
        };
        if content == "steps:" {
            return None;
        }
        if let Some(value) = content.strip_prefix("if:") {
            return Some(value.trim().to_owned());
        }
    }
    None
}
