"""Failure-preserving controls for the GitHub Actions attempt reporter."""

import datetime as dt
import unittest

from ci_attempt_metrics import analyse_run


START = dt.datetime(2026, 10, 20, 11, 47, 5, tzinfo=dt.timezone.utc)


def stamp(seconds):
    return (START + dt.timedelta(seconds=seconds)).isoformat().replace("+00:00", "Z")


def run(attempts=1, final="success"):
    return {
        "id": 39417427592,
        "run_attempt": attempts,
        "head_sha": "a" * 40,
        "html_url": "https://github.com/gyldlab/keld/actions/runs/39417427592",
        "created_at": stamp(0),
        "run_started_at": stamp(5000 if attempts > 1 else 0),
        "updated_at": stamp(6100 if attempts > 1 else 600),
        "status": "completed",
        "conclusion": final,
    }


def job(job_id, name, created, started, completed, conclusion="success", attempt=1,
        runner_name="GitHub Actions 1000000001", runner_id=1000000001):
    return {
        "id": job_id,
        "run_id": 39417427592,
        "head_sha": "a" * 40,
        "name": name,
        "run_attempt": attempt,
        "created_at": stamp(created),
        "started_at": stamp(started),
        "completed_at": stamp(completed),
        "status": "completed",
        "conclusion": conclusion,
        "runner_name": runner_name,
        "runner_id": runner_id,
        "steps": [],
    }


def page(*jobs, total=None):
    return {"total_count": len(jobs) if total is None else total, "jobs": list(jobs)}


class AttemptMetricTests(unittest.TestCase):
    def test_parallel_job_work_is_not_wall_clock_or_push_to_green(self):
        j1 = job(1, "clippy + test (windows-latest)", 0, 60, 360)
        j2 = job(2, "clippy + test (macos-latest)", 0, 60, 480)
        gate = job(3, "CI required", 480, 480, 540)
        report = analyse_run(run(), {1: [page(j1, j2, gate)]})
        attempt = report["attempts"][0]
        self.assertEqual(attempt["observed_runner_seconds"], 780)
        self.assertEqual(attempt["observed_parallel_wall_seconds"], 540)
        self.assertEqual(attempt["observed_queued_seconds"], 120)
        self.assertEqual(report["latest_time_to_ci_required_seconds"], 540)
        self.assertTrue(report["first_attempt_ci_required_success_observed"])
        self.assertEqual(attempt["timing_anomalies"], 0)

    def test_rerun_carryover_job_does_not_become_negative_queue_or_new_work(self):
        initial = page(
            job(1, "clippy + test (macos-latest)", 0, 15, 600, "failure"),
            job(2, "CI required", 600, 600, 610, "failure"),
        )
        # Real GH rerun payloads had attempt=2 and created_at=rerun time
        # even when started_at/completed_at predated that time.
        reused = job(3, "gitleaks", 5000, 10, 25, attempt=2)
        rerun = page(
            reused,
            job(4, "clippy + test (macos-latest)", 5000, 5300, 6000, attempt=2),
            job(5, "CI required", 6000, 6000, 6020, attempt=2),
        )
        report = analyse_run(run(attempts=2), {1: [initial], 2: [rerun]})
        self.assertFalse(report["first_attempt_ci_required_success_observed"])
        self.assertEqual(report["latest_time_to_ci_required_seconds"], 6020)
        self.assertEqual(report["attempts"][1]["timing_anomalies"], 1)
        self.assertEqual(report["attempts"][1]["observed_runner_seconds"], 720)
        self.assertIsNone(report["attempts"][1]["jobs"][0]["queued_seconds"])
        self.assertEqual(report["attempts"][1]["jobs"][0]["time_quality"], "inconsistent_timestamp_order")

    def test_cancelled_or_skipped_required_gate_never_claims_green(self):
        for result in ("cancelled", "skipped", "failure", "timed_out"):
            with self.subTest(result=result):
                payload = page(job(1, "CI required", 0, 10, 20, result))
                report = analyse_run(run(final=result), {1: [payload]})
                self.assertFalse(report["first_attempt_ci_required_success_observed"])
                self.assertIsNone(report["latest_time_to_ci_required_seconds"])

    def test_missing_required_gate_must_not_be_silent_success(self):
        report = analyse_run(run(), {1: [page(job(1, "gitleaks", 0, 2, 6))]})
        self.assertIsNone(report["first_attempt_ci_required_success_observed"])
        self.assertIsNone(report["latest_time_to_ci_required_seconds"])

    def test_incoherent_required_gate_is_unknown_not_green(self):
        payload = page(job(1, "CI required", 500, 10, 20))
        report = analyse_run(run(), {1: [payload]})
        self.assertIsNone(report["first_attempt_ci_required_success_observed"])
        self.assertEqual(report["attempts"][0]["timing_anomalies"], 1)

    def test_missing_or_duplicate_job_page_refused(self):
        j = job(1, "CI required", 0, 10, 20)
        with self.assertRaisesRegex(ValueError, "incomplete"):
            analyse_run(run(), {1: [page(j, total=2)]})
        with self.assertRaisesRegex(ValueError, "duplicate"):
            analyse_run(run(), {1: [page(j), page(j)]})

    def test_missing_attempt_refused_not_relabelled_first_pass(self):
        with self.assertRaisesRegex(ValueError, "missing attempt"):
            analyse_run(run(attempts=2), {2: [page(job(1, "CI required", 0, 1, 2, attempt=2))]})

    def test_job_run_attempt_mismatch_refused(self):
        payload = page(job(1, "CI required", 0, 1, 2, attempt=2))
        with self.assertRaisesRegex(ValueError, "run_attempt"):
            analyse_run(run(), {1: [payload]})

    def test_boolean_attempt_identity_is_not_accepted_as_attempt_one(self):
        # In Python, True == 1, but a JSON boolean is not a GitHub run ID.
        payload = page(job(1, "CI required", 0, 1, 2, attempt=True))
        with self.assertRaisesRegex(ValueError, "run_attempt"):
            analyse_run(run(), {1: [payload]})

    def test_job_from_another_workflow_run_cannot_attest_required_success(self):
        alien = job(1, "CI required", 0, 10, 20)
        alien["run_id"] = 123456789
        with self.assertRaisesRegex(ValueError, "run_id"):
            analyse_run(run(), {1: [page(alien)]})

    def test_skipped_job_is_recorded_but_never_charged_as_runner_work(self):
        selected = job(1, "clippy + test (windows-latest)", 0, 10, 110,
                       conclusion="skipped")
        gate = job(2, "CI required", 110, 110, 120)
        report = analyse_run(run(), {1: [page(selected, gate)]})
        attempt = report["attempts"][0]
        self.assertEqual(attempt["skipped_job_count"], 1)
        self.assertEqual(attempt["observed_runner_seconds"], 10)
        self.assertIsNone(attempt["jobs"][0]["execution_seconds"])

    def test_cross_revision_job_must_not_attest_required_success(self):
        alien = job(1, "CI required", 0, 10, 20)
        alien["head_sha"] = "f" * 40
        with self.assertRaisesRegex(ValueError, "head_sha"):
            analyse_run(run(), {1: [page(alien)]})

    def test_required_gate_outside_workflow_clock_cannot_claim_success(self):
        for impossible in [
            job(1, "CI required", -100, -90, -80),
            job(1, "CI required", 0, 100, 10000),
        ]:
            with self.subTest(impossible=impossible["id"]), self.assertRaisesRegex(ValueError, "CI required.*outside"):
                analyse_run(run(), {1: [page(impossible)]})

    def test_prior_attempt_job_cannot_be_billed_twice_as_new_work(self):
        initial = page(
            job(1, "gitleaks", 0, 10, 25),
            job(2, "CI required", 30, 30, 45, conclusion="failure"),
        )
        same_job = job(1, "gitleaks", 0, 10, 25, attempt=2)
        rerun = page(
            same_job,
            job(3, "clippy + test (macos-latest)", 5000, 5010, 5060, attempt=2),
            job(4, "CI required", 5060, 5061, 5080, attempt=2),
        )
        report = analyse_run(run(attempts=2), {1: [initial], 2: [rerun]})
        second = report["attempts"][1]
        self.assertTrue(second["jobs"][0]["inherited_previous_attempt"])
        self.assertIsNone(second["jobs"][0]["execution_seconds"])
        self.assertEqual(second["observed_runner_seconds"], 69)
        self.assertEqual(second["observed_parallel_wall_seconds"], 80)

    def test_inherited_required_gate_does_not_fake_second_attempt_success(self):
        # A rerun jobs endpoint may repeat a completed required job from attempt
        # one, with the same ID and timestamps but run_attempt set to two.
        first_gate = job(2, "CI required", 30, 31, 45)
        original = page(
            job(1, "change router", 0, 10, 25),
            first_gate,
        )
        carried_gate = job(2, "CI required", 30, 31, 45, attempt=2)
        second = page(
            carried_gate,
            job(3, "clippy + test (macos-latest)", 5000, 5001, 5050,
                attempt=2),
        )
        report = analyse_run(run(attempts=2), {1: [original], 2: [second]})
        self.assertTrue(report["first_attempt_ci_required_success_observed"])
        self.assertTrue(report["attempts"][1]["jobs"][0]["inherited_previous_attempt"])
        self.assertIsNone(report["attempts"][1]["jobs"][0]["execution_seconds"])
        self.assertIsNone(report["attempts"][1]["ci_required_result_observed"])
        self.assertIsNone(report["attempts"][1]["ci_required_success_observed"])
        self.assertIsNone(report["attempts"][1]["ci_required_completed_at"])
        self.assertIsNone(report["latest_time_to_ci_required_seconds"])
        self.assertEqual(report["attempts"][1]["observed_runner_seconds"], 49)

    def test_invalid_step_payload_fails_cleanly_and_boolean_attempt_keys_are_refused(self):
        invalid = job(1, "CI required", 0, 10, 20)
        invalid["steps"] = {"instead": "of an array"}
        with self.assertRaisesRegex(ValueError, "steps"):
            analyse_run(run(), {1: [page(invalid)]})
        with self.assertRaisesRegex(ValueError, "attempt"):
            analyse_run(run(), {True: [page(job(1, "CI required", 0, 10, 20))]})

    def test_genuine_queue_delay_is_not_counted_as_runner_work(self):
        payload = page(job(1, "CI required", 0, 4740, 4800))
        snapshot = run()
        snapshot["updated_at"] = stamp(4900)
        report = analyse_run(snapshot, {1: [payload]})
        attempt = report["attempts"][0]
        self.assertEqual(attempt["observed_queued_seconds"], 4740)
        self.assertEqual(attempt["observed_runner_seconds"], 60)

    def test_bad_step_times_are_explicit_anomalies_not_valid_durations(self):
        gate = job(1, "CI required", 0, 10, 60)
        gate["steps"] = [
            {"name": "backwards", "number": 1, "status": "completed",
             "conclusion": "success", "started_at": stamp(40), "completed_at": stamp(20)},
            {"name": "outside job", "number": 2, "status": "completed",
             "conclusion": "success", "started_at": stamp(2), "completed_at": stamp(12)},
            {"name": "ordinary", "number": 3, "status": "completed",
             "conclusion": "success", "started_at": stamp(20), "completed_at": stamp(40)},
        ]
        report = analyse_run(run(), {1: [page(gate)]})
        attempt = report["attempts"][0]
        self.assertEqual(attempt["step_timing_anomalies"], 2)
        steps = attempt["jobs"][0]["steps"]
        self.assertEqual([s["time_quality"] for s in steps], [
            "inconsistent_timestamp_order", "outside_job_interval", "ordered",
        ])
        self.assertEqual([s["seconds"] for s in steps], [None, None, 20])

    def test_successful_report_does_not_certify_skipped_selected_lane(self):
        selected = job(1, "clippy + test (windows-latest)", 0, 10, 20,
                       conclusion="skipped")
        gate = job(2, "CI required", 20, 21, 22)
        report = analyse_run(run(), {1: [page(selected, gate)]})
        self.assertTrue(report["first_attempt_ci_required_success_observed"])
        self.assertFalse(report["independent_required_gate_provenance_verified"])

    def test_real_cancelled_unassigned_job_has_no_runner_work(self):
        # Exact first-attempt CodeQL placeholder shape observed in GH run
        # 39760253424, job 119763025079: no runner or steps were assigned.
        revision = "960692e8e02f2850b5078061fd1fa9e92d568824"
        snapshot = {
            "id": 39760253424,
            "run_attempt": 1,
            "head_sha": revision,
            "created_at": "2026-10-22T10:03:55Z",
            "status": "completed",
            "conclusion": "cancelled",
        }
        cancelled = {
            "id": 119763025079,
            "run_id": 39760253424,
            "run_attempt": 1,
            "head_sha": revision,
            "name": "CodeQL (rust)",
            "created_at": "2026-10-22T10:05:27Z",
            "started_at": "2026-10-22T10:05:27Z",
            "completed_at": "2026-10-22T10:05:27Z",
            "status": "completed",
            "conclusion": "cancelled",
            "runner_id": None,
            "runner_name": None,
            "steps": [],
        }
        report = analyse_run(snapshot, {1: [page(cancelled)]})
        attempt = report["attempts"][0]
        observed = attempt["jobs"][0]
        self.assertEqual(observed["conclusion"], "cancelled")
        self.assertEqual(observed["time_quality"], "ordered")
        self.assertEqual(observed["created_at"], cancelled["created_at"])
        self.assertEqual(observed["started_at"], cancelled["started_at"])
        self.assertEqual(observed["completed_at"], cancelled["completed_at"])
        self.assertEqual(attempt["timed_job_count"], 0)
        self.assertIsNone(observed["queued_seconds"])
        self.assertIsNone(observed["execution_seconds"])
        self.assertEqual(attempt["observed_queued_seconds"], 0)
        self.assertEqual(attempt["observed_runner_seconds"], 0)
        self.assertIsNone(attempt["observed_parallel_wall_seconds"])
        self.assertEqual(attempt["unassigned_cancelled_job_count"], 1)
        self.assertFalse(observed["runner_assignment_observed"])
        self.assertIsNone(report["first_attempt_ci_required_success_observed"])

    def test_unassigned_cancelled_placeholder_cannot_extend_work_or_queue(self):
        active = job(1, "change router", 0, 2, 30)
        gate = job(2, "CI required", 30, 31, 35, "failure")
        # Job-level workflow cancellation can report synthetic started_at.
        pending = job(3, "CodeQL (rust)", 40, 50, 500, "cancelled",
                      runner_name=None, runner_id=None)
        report = analyse_run(run(final="failure"), {1: [page(active, gate, pending)]})
        attempt = report["attempts"][0]
        self.assertEqual(attempt["jobs"][2]["conclusion"], "cancelled")
        self.assertIsNone(attempt["jobs"][2]["queued_seconds"])
        self.assertIsNone(attempt["jobs"][2]["execution_seconds"])
        self.assertEqual(attempt["observed_runner_seconds"], 32)
        self.assertEqual(attempt["observed_queued_seconds"], 3)
        self.assertEqual(attempt["observed_parallel_wall_seconds"], 35)
        self.assertEqual(attempt["timed_job_count"], 2)
        self.assertEqual(attempt["unassigned_cancelled_job_count"], 1)
        self.assertFalse(report["first_attempt_ci_required_success_observed"])

    def test_assigned_cancelled_job_preserves_real_queue_and_runner_execution(self):
        assigned = job(1, "change router", 0, 5, 25, "cancelled")
        gate = job(2, "CI required", 25, 26, 30, "failure")
        report = analyse_run(run(final="failure"), {1: [page(assigned, gate)]})
        attempt = report["attempts"][0]
        self.assertTrue(attempt["jobs"][0]["runner_assignment_observed"])
        self.assertEqual(attempt["jobs"][0]["conclusion"], "cancelled")
        self.assertEqual(attempt["jobs"][0]["queued_seconds"], 5)
        self.assertEqual(attempt["jobs"][0]["execution_seconds"], 20)
        self.assertEqual(attempt["observed_runner_seconds"], 24)
        self.assertEqual(attempt["observed_queued_seconds"], 6)
        self.assertEqual(attempt["unassigned_cancelled_job_count"], 0)

    def test_runner_id_is_assignment_evidence_without_name_or_steps(self):
        assigned = job(1, "clippy + test (macos-latest)", 0, 10, 30,
                       conclusion="cancelled", runner_name=None, runner_id=12345)
        report = analyse_run(run(final="cancelled"), {1: [page(assigned)]})
        measured = report["attempts"][0]["jobs"][0]
        self.assertTrue(measured["runner_assignment_observed"])
        self.assertEqual(measured["queued_seconds"], 10)
        self.assertEqual(measured["execution_seconds"], 20)

    def test_setup_job_step_proves_assignment_without_runner_metadata(self):
        assigned = job(1, "clippy + test (macos-latest)", 0, 10, 30,
                       conclusion="cancelled", runner_name=None, runner_id=None)
        assigned["steps"] = [{
            "number": 1,
            "name": "Set up job",
            "status": "completed",
            "conclusion": "success",
            "started_at": stamp(10),
            "completed_at": stamp(12),
        }]
        report = analyse_run(run(final="cancelled"), {1: [page(assigned)]})
        measured = report["attempts"][0]["jobs"][0]
        self.assertTrue(measured["runner_assignment_observed"])
        self.assertEqual(measured["queued_seconds"], 10)
        self.assertEqual(measured["execution_seconds"], 20)


if __name__ == "__main__":
    unittest.main()
