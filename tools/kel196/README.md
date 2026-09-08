# KEL-196 disposable hosted diagnostic

Do not merge this experiment branch. It owns no product behavior or required CI gate.
The push-only workflow checks out the exact historical failing source separately
from these diagnostic tools, compares pristine and instrumented copies on separate
fresh macos-latest runners, and retains original test exit statuses. Actions are
pinned, credentials are not persisted, permissions are read-only, no secrets are used.

Source under test: a95043b714bd447930796159d874d7978e766874 (KEL-194 PR 190).
Local GUI baseline: 65e5036; window backend and no_flag_macos source are identical
at those two commits. Current main 37d14c4 adds fixture dark-background HTML; it is
not substituted for the historical source.

Decision atoms: (1) in-process tao/AppKit state; (2) independent CG all/on-screen
membership for that PID/window; (3) observer metadata access; (4) runner GUI session;
(5) unmodified test predicate/exit. One atom does not substitute for another.
Compare construction, Init, readiness and cleanup using monotonic clocks. An
observed absence of title is not an absent window; no GUI session is not an empty
window list. Classify unknown historical behaviors explicitly.

No activation policy, focus setter, product process groups, retry configuration,
assertions, test scheduling, or existing deadlines are changed. Diagnostic sampling
is bounded and not a pass condition. The host trace has synchronous I/O overhead;
the pristine runner is its independent behavioral control. No timeout is added to
the test subprocess. The 45-minute job timeout matches the original CI lane.

The local independent review found no presentation setter or FFI lifetime defect.
A diagnostic file sink avoids filling the fixture's undrained stderr pipe. Full
release and normal debug compilation do not activate the patch's trace. The patch
is applied only to the instrumented temporary source checkout; these tools do not
modify the shipping branch or KEL-194's workflow.

Both jobs include external observational overhead. The census stops after 3,600
samples; an observer failure or exhausted interval is missing evidence, not an
empty census. Output sidecar timestamps mean line receipt, not the original event
time: nextest can buffer test output until the test exits.
