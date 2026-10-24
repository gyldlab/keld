#!/usr/bin/env python3
"""Read-only, attempt-aware GitHub Actions timing report from API snapshots.

Do not interpret the rerun's final green result as first-attempt success.
GitHub's per-attempt jobs endpoint can show entries whose created_at follows
started_at. Preserve those records with a warning and exclude them from
the *new* attempt's queue/execution accounting.
Cancelled jobs without runner assignment may have synthetic started_at and
completed_at values. Preserve the cancelled result and raw clocks, but report
queue/execution seconds as unobserved until runner assignment is evidenced.

Example (capture all pages when the endpoint reports more than 100 jobs):
  gh api repos/gyldlab/keld/actions/runs/RUN_ID > run.json
  gh api 'repos/gyldlab/keld/actions/runs/RUN_ID/attempts/1/jobs?per_page=100' > jobs1.json
  python3 -B tools/ci_attempt_metrics.py --run run.json --jobs 1:jobs1.json

The tool never invokes GitHub or handles credentials. It does not infer a
workflow dependency graph, paid minutes or selected-lane applicability.
"""

import argparse
import datetime as dt
import json
import pathlib
import re
import sys


def _positive_int(value, field):
    if type(value) is not int or value <= 0:
        raise ValueError(f"{field} must be a positive integer")
    return value


def _timestamp(value, field):
    if value is None:
        return None
    if not isinstance(value, str):
        raise ValueError(f"{field} must be an ISO 8601 timestamp or null")
    try:
        parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ValueError(f"{field}: invalid timestamp") from exc
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise ValueError(f"{field}: timezone offset required")
    return parsed.astimezone(dt.timezone.utc)


def _elapsed(start, end):
    return round((end - start).total_seconds(), 3)


def _ordered_timestamps(record, prefix):
    created = _timestamp(record.get("created_at"), f"{prefix}.created_at")
    started = _timestamp(record.get("started_at"), f"{prefix}.started_at")
    ended = _timestamp(record.get("completed_at"), f"{prefix}.completed_at")
    if created is None or started is None or ended is None:
        return "missing_timestamp", created, started, ended
    if not created <= started <= ended:
        return "inconsistent_timestamp_order", created, started, ended
    return "ordered", created, started, ended


def _job_measurements(job, attempt, run_id, head_sha):
    job_id = _positive_int(job.get("id"), "job.id")
    if type(job.get("run_id")) is not int or job["run_id"] != run_id:
        raise ValueError(f"job {job_id}: run_id does not match the workflow run")
    if not isinstance(job.get("head_sha"), str) or job["head_sha"] != head_sha:
        raise ValueError(f"job {job_id}: head_sha does not match the workflow revision")
    if type(job.get("run_attempt")) is not int or job["run_attempt"] != attempt:
        raise ValueError(f"job {job_id}: run_attempt does not match requested page")
    name = job.get("name")
    if not isinstance(name, str) or not name:
        raise ValueError(f"job {job_id}: missing name")
    quality, created, started, ended = _ordered_timestamps(job, f"job[{job_id}]")
    steps = []
    step_records = job.get("steps")
    if step_records is not None and not isinstance(step_records, list):
        raise ValueError(f"job {job_id}: steps must be a list")
    for step in step_records or []:
        if not isinstance(step, dict):
            raise ValueError(f"job {job_id}: steps must contain objects")
        start = _timestamp(step.get("started_at"), f"job[{job_id}].step.started_at")
        finish = _timestamp(step.get("completed_at"), f"job[{job_id}].step.completed_at")
        if start is None or finish is None:
            step_quality = "missing_timestamp"
        elif start > finish:
            step_quality = "inconsistent_timestamp_order"
        elif started is not None and ended is not None and started <= ended and (
            start < started or finish > ended
        ):
            step_quality = "outside_job_interval"
        elif started is None or ended is None or started > ended:
            step_quality = "parent_interval_unverified"
        else:
            step_quality = "ordered"
        steps.append({
            "name": step.get("name"),
            "number": step.get("number"),
            "status": step.get("status"),
            "conclusion": step.get("conclusion"),
            "time_quality": step_quality,
            "seconds": _elapsed(start, finish) if step_quality == "ordered" else None,
        })
    # GitHub may give a cancelled, never-assigned job synthetic started_at and
    # completed_at values. Job timestamps alone are not evidence of runner work.
    runner_name = job.get("runner_name")
    runner_id = job.get("runner_id")
    runner_assignment_observed = (
        isinstance(runner_name, str) and bool(runner_name.strip())
        or type(runner_id) is int and runner_id > 0
        or any(step["name"] == "Set up job" and
               step["time_quality"] == "ordered" for step in steps)
    )
    executed = (
        quality == "ordered" and job.get("status") == "completed"
        and job.get("conclusion") not in (None, "skipped")
        and runner_assignment_observed
    )
    return {
        "id": job_id,
        "run_id": run_id,
        "head_sha": head_sha,
        "name": name,
        "status": job.get("status"),
        "conclusion": job.get("conclusion"),
        "time_quality": quality,
        "created_at": job.get("created_at"),
        "started_at": job.get("started_at"),
        "completed_at": job.get("completed_at"),
        # No evidence of an assigned runner means queue and execution unknown,
        # even if synthetic start/end timestamps happen to be ordered.
        "queued_seconds": _elapsed(created, started) if executed else None,
        "execution_seconds": _elapsed(started, ended) if executed else None,
        "runner_name": runner_name,
        "runner_id": runner_id,
        "runner_assignment_observed": runner_assignment_observed,
        "labels": job.get("labels"),
        "inherited_previous_attempt": False,
        "steps": steps,
    }


def _complete_jobs(attempt, pages, run_id, head_sha):
    if not isinstance(pages, list) or not pages:
        raise ValueError(f"attempt {attempt}: incomplete jobs pages (none provided)")
    expected_count = None
    jobs = []
    ids = set()
    for page in pages:
        count = page.get("total_count")
        if type(count) is not int or count < 0:
            raise ValueError(f"attempt {attempt}: invalid total_count")
        if expected_count is None:
            expected_count = count
        if expected_count != count:
            raise ValueError(f"attempt {attempt}: inconsistent jobs page total_count")
        rows = page.get("jobs")
        if not isinstance(rows, list):
            raise ValueError(f"attempt {attempt}: jobs must be an array")
        for job in rows:
            jid = _positive_int(job.get("id"), "job.id")
            if jid in ids:
                raise ValueError(f"attempt {attempt}: duplicate job id {jid}")
            ids.add(jid)
            jobs.append(_job_measurements(job, attempt, run_id, head_sha))
    if len(jobs) != expected_count:
        raise ValueError(
            f"attempt {attempt}: incomplete jobs pages "
            f"({len(jobs)} of {expected_count} captured)"
        )
    return jobs


def _attempt_summary(number, pages, run_id, head_sha, seen_before):
    jobs = _complete_jobs(number, pages, run_id, head_sha)
    for job in jobs:
        original = seen_before.get(job["id"])
        if original is not None and all(
            job[key] == original[key] for key in
            ("created_at", "started_at", "completed_at", "name", "conclusion")
        ):
            # A rerun API page can carry a previous successful job without
            # executing it again. Retain its status, but do not bill work twice.
            job["inherited_previous_attempt"] = True
            job["queued_seconds"] = None
            job["execution_seconds"] = None
    ordered = [item for item in jobs if item["time_quality"] == "ordered"]
    valid = [item for item in ordered if item["execution_seconds"] is not None]
    gates = [item for item in jobs if item["name"] == "CI required"]
    gate = gates[0] if len(gates) == 1 and gates[0]["time_quality"] == "ordered" else None
    result = gate["conclusion"] if gate else None
    observed_success = (
        result == "success" and gate["status"] == "completed"
        if gate is not None else None
    )
    created = [_timestamp(item["created_at"], "job.created_at") for item in valid]
    ended = [_timestamp(item["completed_at"], "job.completed_at") for item in valid]
    # This span is not the source-defined critical-path DAG.
    wall = _elapsed(min(created), max(ended)) if valid else None
    return {
        "attempt": number,
        "job_count": len(jobs),
        "timed_job_count": len(valid),
        "timing_anomalies": len(jobs) - len(ordered),
        "skipped_job_count": sum(item["conclusion"] == "skipped" for item in jobs),
        "unassigned_cancelled_job_count": sum(
            item["conclusion"] == "cancelled"
            and not item["runner_assignment_observed"] for item in jobs
        ),
        "step_timing_anomalies": sum(
            sum(step["time_quality"] != "ordered" for step in job["steps"])
            for job in jobs
        ),
        "observed_parallel_wall_seconds": wall,
        "observed_queued_seconds": round(
            sum(item["queued_seconds"] for item in valid), 3
        ),
        "observed_runner_seconds": round(
            sum(item["execution_seconds"] for item in valid), 3
        ),
        "ci_required_result_observed": result,
        "ci_required_success_observed": observed_success,
        "ci_required_completed_at": gate["completed_at"] if gate else None,
        "jobs": jobs,
    }


def analyse_run(run, jobs_pages_by_attempt):
    """Compute attempt-bound measures without any latest-green shortcut.

    All attempts and all jobs pages must be provided; missing evidence refuses
    calculation rather than silently changing which attempt is first.
    """
    run_id = _positive_int(run.get("id"), "run.id")
    maximum = _positive_int(run.get("run_attempt"), "run.run_attempt")
    head_sha = run.get("head_sha")
    if not isinstance(head_sha, str) or not re.fullmatch(r"[0-9a-f]{40,64}", head_sha):
        raise ValueError("run.head_sha must identify an exact commit")
    created = _timestamp(run.get("created_at"), "run.created_at")
    if created is None:
        raise ValueError("run.created_at is required")
    updated = _timestamp(run.get("updated_at"), "run.updated_at")
    if updated is not None and updated < created:
        raise ValueError("run.updated_at precedes run.created_at")
    if any(type(attempt) is not int or attempt <= 0 for attempt in jobs_pages_by_attempt):
        raise ValueError("jobs attempt keys must be positive integers")
    expected = set(range(1, maximum + 1))
    if set(jobs_pages_by_attempt) != expected:
        raise ValueError(
            f"missing attempt or extra attempt: expected {sorted(expected)}, "
            f"received {sorted(jobs_pages_by_attempt)}"
        )
    attempts = []
    seen_before = {}
    for number in sorted(expected):
        attempt = _attempt_summary(number, jobs_pages_by_attempt[number], run_id,
                                   head_sha, seen_before)
        for job in attempt["jobs"]:
            seen_before[job["id"]] = job
            if job["name"] == "CI required" and job["time_quality"] == "ordered":
                gate_created = _timestamp(job["created_at"], "CI required.created_at")
                gate_end = _timestamp(job["completed_at"], "CI required.completed_at")
                if gate_created < created or (updated is not None and gate_end > updated):
                    raise ValueError("CI required completed outside the workflow run's clock bounds")
        attempts.append(attempt)
    latest = attempts[-1]
    latest_finished = (
        _timestamp(latest["ci_required_completed_at"], "ci_required_completed_at")
        if latest["ci_required_completed_at"] else None
    )
    latest_seconds = None
    if (
        latest["ci_required_success_observed"] is True
        and run.get("status") == "completed"
        and run.get("conclusion") == "success"
        and latest_finished is not None
        and latest_finished >= created
    ):
        latest_seconds = _elapsed(created, latest_finished)
    return {
        "run_id": run_id,
        "url": run.get("html_url"),
        "head_sha": run.get("head_sha"),
        "run_created_at": run["created_at"],
        "latest_run_started_at": run.get("run_started_at"),
        "run_updated_at": run.get("updated_at"),
        "github_reported_final_conclusion": run.get("conclusion"),
        "attempt_count": maximum,
        "first_attempt_ci_required_success_observed": attempts[0][
            "ci_required_success_observed"
        ],
        # GitHub's reported CI required result alone is not an independent
        # attestation of every selected lane's provenance or correctness.
        "independent_required_gate_provenance_verified": False,
        "latest_time_to_ci_required_seconds": latest_seconds,
        "attempts": attempts,
        "limitations": [
            "CI required is a GitHub-reported job result, not independent "
            "verification of selected-lane provenance (KEL-169 remains open)",
            "jobs API lacks the complete workflow dependency graph; observed "
            "parallel wall time is NOT a proven critical-path DAG",
            "sum of observed runner execution seconds is NOT billed runner-minutes",
            "timestamp-inconsistent rerun records are retained but excluded "
            "from that attempt's work/queue totals; attribution unknown",
            "cancelled jobs without runner assignment retain their result "
            "and raw clocks, but have no observed queue/execution duration",
            "workflow-run created_at is not necessarily developer push time",
            "single-run reports do not establish statistical performance gains",
        ],
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", required=True, type=pathlib.Path,
                        help="GitHub workflow-run JSON snapshot")
    parser.add_argument("--jobs", required=True, action="append", metavar="ATTEMPT:JSON_FILE",
                        help="One jobs endpoint page; repeat for every page and attempt")
    parser.add_argument("--output", type=pathlib.Path,
                        help="Write JSON report here instead of stdout")
    args = parser.parse_args(argv)
    try:
        run = json.loads(args.run.read_text(encoding="utf-8"))
        pages = {}
        for value in args.jobs:
            number_text, sep, path_text = value.partition(":")
            if not sep or not path_text:
                raise ValueError("--jobs expects ATTEMPT:JSON_FILE")
            number = _positive_int(int(number_text), "jobs.attempt")
            pages.setdefault(number, []).append(
                json.loads(pathlib.Path(path_text).read_text(encoding="utf-8"))
            )
        report = analyse_run(run, pages)
    except (OSError, ValueError, TypeError) as exc:
        parser.error(str(exc))
    encoded = json.dumps(report, sort_keys=True, indent=2) + "\n"
    if args.output:
        args.output.write_text(encoded, encoding="utf-8")
    else:
        sys.stdout.write(encoded)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
