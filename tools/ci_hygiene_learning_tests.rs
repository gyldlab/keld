use super::*;

const LEARNING_COMMANDS: [&str; 2] = [
    "python3 -B tools/learning_records.py check",
    "python3 -B -m unittest discover -s tools -p 'test_learning*.py'",
];

#[test]
fn hosted_learning_validator_and_discovery_are_both_required() {
    for command in LEARNING_COMMANDS {
        let workflow = valid_workflow().replace(&format!("          {command}\n"), "");
        let result = check_agent_context_step(&workflow);
        assert!(result.is_err(), "hosted admission omitted `{command}`");
    }
}

#[test]
fn hosted_learning_commands_cannot_be_inert_or_suppressed() {
    for command in LEARNING_COMMANDS {
        for replacement in [
            format!("# {command}"),
            format!("echo {command}"),
            format!("{command} || true"),
            format!("false && {command}"),
            format!("if false; then {command}; fi"),
        ] {
            let workflow = valid_workflow().replacen(command, &replacement, 1);
            let error = check_agent_context_step(&workflow)
                .expect_err("inert or swallowed learning admission must fail");
            assert!(error.contains("without wrappers"), "{error}");
        }
    }
}

#[test]
fn hosted_learning_step_cannot_be_disabled_or_ignore_failure() {
    let step = "      - name: Agent instruction context budget\n";
    for property in [
        "if: false",
        "continue-on-error: true",
        "shell: bash {0} || true",
    ] {
        let workflow = valid_workflow().replacen(step, &format!("{step}        {property}\n"), 1);
        let error = check_agent_context_step(&workflow)
            .expect_err("learning admission step must preserve failures");
        assert!(error.contains("only its `run` key"), "{error}");
    }
    let workflow = valid_workflow().replacen(
        "  hygiene:\n",
        "  hygiene:\n    continue-on-error: true\n",
        1,
    );
    assert!(check_agent_context_step(&workflow).is_err());
}

#[cfg(unix)]
#[test]
fn real_failed_python_children_stop_hosted_learning_commands() {
    use std::process::Command;

    let workflow = include_str!("../.github/workflows/ci.yml");
    check_agent_context_step(workflow).expect("real workflow admission is guarded");
    let hygiene = workflow_job_block(workflow, "hygiene").expect("hosted hygiene job");
    assert_eq!(
        workflow_job_level_property(&hygiene, "runs-on").as_deref(),
        Some("ubuntu-latest"),
    );
    let commands = workflow_named_step_shell_commands(workflow, "Agent instruction context budget")
        .expect("real hosted run block");
    let selected: Vec<_> = commands
        .into_iter()
        .filter(|command| LEARNING_COMMANDS.contains(&command.as_str()))
        .collect();
    assert_eq!(selected.len(), 2);
    let python = Command::new("python3")
        .arg("--version")
        .output()
        .expect("observe existing Python runtime");
    assert!(python.status.success());
    eprintln!(
        "LEARNING_HOSTED_PYTHON {:?}",
        String::from_utf8_lossy(&python.stdout)
    );

    for (case, validator_exit, failing_test, expected_exit) in [
        ("valid", 0, false, 0),
        ("validator-fails", 7, false, 7),
        ("discovered-test-fails", 0, true, 1),
    ] {
        let fixture = TempDir::new();
        fixture.write(
            "tools/learning_records.py",
            &format!("raise SystemExit({validator_exit})\n"),
        );
        fixture.write(
            "tools/test_learning_hosted_canary.py",
            &format!(
                "import unittest\nclass AdmissionCanary(unittest.TestCase):\n    def test_child_status(self):\n        self.assertFalse({})\n",
                if failing_test { "True" } else { "False" },
            ),
        );
        let script = format!("{}\nprintf reached > after\n", selected.join("\n"));
        fixture.write("hosted.sh", &script);
        let output = Command::new("bash")
            .args(["--noprofile", "--norc", "-e", "hosted.sh"])
            .env_remove("BASH_ENV")
            .env_remove("ENV")
            .current_dir(fixture.path())
            .output()
            .expect("execute real Python commands under fail-fast Bash");
        eprintln!(
            "LEARNING_HOSTED_CANARY case={case} exit={:?} after={} stdout={:?} stderr={:?}",
            output.status.code(),
            fixture.path().join("after").exists(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert_eq!(output.status.code(), Some(expected_exit));
        assert_eq!(fixture.path().join("after").exists(), expected_exit == 0);
    }
}
