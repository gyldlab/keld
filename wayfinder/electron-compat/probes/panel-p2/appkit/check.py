"""PANEL-P2 AppKit harness checker. Run: python3 -I check.py <transcripts dir>
Evaluates every transcript against criteria C1..C7, prints the matrix, ordering
signatures (determinism across runs) and the re-entrancy probe table, and writes
<dir>/_results.json."""
import collections, glob, json, os, sys

D = sys.argv[1]
EXPECT_FAIL = {  # criteria a scenario is designed to violate
    "n1_sync_yes_while_dirty": {"C1", "C4"},
    "n2_timeout_autoclose": {"C1", "C2"},
    "n3_second_close_during_modal_ungated": {"C3"},
    "s3d_terminate_later_default_mode_delivery": {"C5", "C7"},
}
CRIT = {
    "C1": "fail-closed: a dirty window is destroyed only after an explicit role 'allow' and never after a synchronous YES",
    "C2": "no auto-close on timeout: no destroy follows a close-timeout without an intervening role 'allow'",
    "C3": "single prompt: never two dialogs open at once; at most one dialog per close txn",
    "C4": "tombstone-before-closed: every 'closed' is preceded by that window's 'tombstone'",
    "C5": "quit ordering: before-quit < serialized closes < reply; will-quit/quit iff reply YES; reply NO keeps app alive",
    "C6": "quit serialization: the next window's close starts only after the previous window's outcome",
    "C7": "no hang: the run ends by quit or harness-end, not the watchdog",
}

def load(p):
    return [json.loads(l) for l in open(p) if l.strip()]

def check(ev):
    r = {}
    dirty = {e["win"]: e["dirty"] for e in ev if e["ev"] == "window-created"}
    # C1
    ok = True
    for i, e in enumerate(ev):
        if e["ev"] == "destroy" and dirty.get(e["win"]):
            w = e["win"]
            prior = [x for x in ev[:i] if x.get("win") == w]
            if any(x["ev"] == "close-return" and x.get("returned") is True for x in prior):
                ok = False
            replies = [x for x in prior if x["ev"] == "role-reply"]
            if not replies or replies[-1]["reply"] != "allow":
                ok = False
    r["C1"] = ok
    # C2
    ok = True
    for i, e in enumerate(ev):
        if e["ev"] == "destroy":
            w = e["win"]
            seen_timeout = False
            for x in ev[:i]:
                if x.get("win") != w:
                    continue
                if x["ev"] == "close-timeout":
                    seen_timeout = True
                if x["ev"] == "role-reply" and x["reply"] == "allow":
                    seen_timeout = False
            if seen_timeout:
                ok = False
    r["C2"] = ok
    # C3
    ok, open_n, per_txn = True, 0, collections.Counter()
    for e in ev:
        if e["ev"] == "dialog" and e["phase"] == "open":
            if open_n > 0:
                ok = False
            open_n += 1
            per_txn[(e["win"], e["txn"])] += 1
        elif e["ev"] == "dialog" and e["phase"] == "result":
            open_n -= 1
    if any(v > 1 for v in per_txn.values()):
        ok = False
    r["C3"] = ok
    # C4
    ok = True
    for i, e in enumerate(ev):
        if e["ev"] == "closed":
            if not any(x["ev"] == "tombstone" and x.get("win") == e["win"] for x in ev[:i]):
                ok = False
        if e["ev"] == "tombstone-before-closed" and not e["ok"]:
            ok = False
    r["C4"] = ok
    # C5 / C6 (quit scenarios only)
    names = [e["ev"] for e in ev]
    if "before-quit" in names:
        ok = True
        bq = names.index("before-quit")
        reps = [e for e in ev if e["ev"] == "reply-to-should-terminate"]
        if len(reps) != 1:
            ok = False
        else:
            rep_i = ev.index(reps[0])
            v = reps[0]["value"]
            if any(e["ev"] == "destroy" and not (bq < k < rep_i) for k, e in enumerate(ev)):
                ok = False
            if v:
                if not ("will-quit" in names and "quit" in names and rep_i < names.index("will-quit") < names.index("quit")):
                    ok = False
            else:
                if "will-quit" in names or "quit" in names or "terminate-returned" not in names:
                    ok = False
                if "quit-cancelled" not in names:
                    ok = False
        r["C5"] = ok
        ok = True
        qreq = [k for k, e in enumerate(ev) if e["ev"] == "close-request" and e.get("source", "").startswith("quit-serializer")]
        for a, b in zip(qreq, qreq[1:]):
            wa = ev[a]["win"]
            if not any(e["ev"] == "close-outcome" and e.get("win") == wa for e in ev[a:b]):
                ok = False
        r["C6"] = ok
    else:
        r["C5"] = None
        r["C6"] = None
    r["C7"] = ("watchdog" not in names) and ("quit" in names or "harness-end" in names)
    return r

SIG_EVS = {"before-quit", "close", "close-coalesced", "close-timeout", "role-reply", "dialog", "tombstone",
           "destroy", "closed", "tombstone-before-closed", "stale-reply-dropped", "reply-to-should-terminate",
           "quit-serial-stop", "quit-cancelled", "terminate-returned", "will-quit", "quit", "watchdog"}

def sig(ev):
    out = []
    for e in ev:
        n = e["ev"]
        if n not in SIG_EVS:
            continue
        s = n
        if n == "close":
            s += "(%s,%s)" % (e["win"], e["entry"])
        elif n == "role-reply":
            s += "(%s:%s)" % (e["win"], e["reply"])
        elif n == "dialog":
            s += "-%s(%s%s)" % (e["phase"], e["win"], (":" + e["response"]) if e["phase"] == "result" else "")
        elif n == "tombstone-before-closed":
            s += "(%s:%s)" % (e["win"], e["ok"])
        elif n == "reply-to-should-terminate":
            s += "(%s)" % e["value"]
        elif "win" in e:
            s += "(%s)" % e["win"]
        out.append(s)
    return " > ".join(out)

files = sorted(glob.glob(os.path.join(D, "*.run*.jsonl")))
by_s = collections.defaultdict(list)
for f in files:
    ev = load(f)
    s = ev[0]["scenario"]
    by_s[s].append((f, ev))

results = {"criteria": CRIT, "scenarios": {}}
print("criteria:", json.dumps(CRIT, indent=1))
for s in sorted(by_s):
    runs = by_s[s]
    mat = [check(ev) for _, ev in runs]
    sigs = collections.Counter(sig(ev) for _, ev in runs)
    agg = {}
    for c in CRIT:
        vals = [m[c] for m in mat]
        if all(v is None for v in vals):
            agg[c] = "n/a"
        else:
            p = sum(1 for v in vals if v)
            agg[c] = "%d/%d pass" % (p, len(vals))
    exp = EXPECT_FAIL.get(s, set())
    verdict = []
    for c in CRIT:
        vals = [m[c] for m in mat]
        if all(v is None for v in vals):
            continue
        if c in exp:
            verdict.append("%s:%s" % (c, "FAILS-AS-REQUIRED" if all(v is False for v in vals) else "UNEXPECTED-PASS"))
        elif not all(vals):
            verdict.append("%s:UNEXPECTED-FAIL" % c)
    results["scenarios"][s] = {"runs": len(runs), "criteria": agg, "expected_fail": sorted(exp),
                               "verdict": verdict or ["all applicable criteria pass"],
                               "distinct_orderings": len(sigs), "ordering": list(sigs.keys())}
    print("\n==", s, "runs=%d" % len(runs), "distinct_orderings=%d" % len(sigs))
    print("  ", agg)
    print("   verdict:", verdict or "all applicable criteria pass")
    for k, v in sigs.items():
        print("   [%dx] %s" % (v, k))

# re-entrancy probe table
print("\n== re-entrancy probes (fired while modal/terminateLater wait active, out of runs)")
probe_tab = {}
for s in sorted(by_s):
    if not any(e["ev"] == "probes-armed" for _, ev in by_s[s] for e in ev):
        continue
    t = collections.defaultdict(lambda: {"during": 0, "after": 0, "never": 0, "modes": set()})
    names = None
    for _, ev in by_s[s]:
        fired = {}
        for e in ev:
            if e["ev"] == "probe":
                fired[e["probe"]] = e
        armed_names = ["timer-defaultMode", "timer-commonModes", "timer-modalPanelMode", "perform-afterDelay(default)",
                       "gcd-main-asyncAfter", "bg->gcd-main-async", "bg->performSelectorOnMainThread(common)",
                       "bg->CFRunLoopPerformBlock(default)", "bg->CFRunLoopPerformBlock(common)",
                       "bg->CFRunLoopPerformBlock(modalPanel)"]
        names = armed_names
        for n in armed_names:
            e = fired.get(n)
            if e is None:
                t[n]["never"] += 1
            elif e["in_dialog"] or e["in_terminate_later"]:
                t[n]["during"] += 1
                t[n]["modes"].add(e["rl_mode"])
            else:
                t[n]["after"] += 1
                t[n]["modes"].add(e["rl_mode"])
    probe_tab[s] = {n: {"during": t[n]["during"], "after": t[n]["after"], "never_before_exit": t[n]["never"],
                        "rl_modes": sorted(t[n]["modes"])} for n in names}
    print(" ", s)
    for n in names:
        print("    %-42s during=%d after=%d never=%d modes=%s" % (n, t[n]["during"], t[n]["after"], t[n]["never"], sorted(t[n]["modes"])))
results["probes"] = probe_tab

# latency of role replies and dialog durations
lat = collections.defaultdict(list)
for s, runs in by_s.items():
    for _, ev in runs:
        for e in ev:
            if e["ev"] == "role-reply":
                lat["role-reply rl_mode"].append(e["rl_mode"])
results["role_reply_modes"] = dict(collections.Counter(lat["role-reply rl_mode"]))
print("\nrole-reply delivery modes:", results["role_reply_modes"])
json.dump(results, open(os.path.join(D, "_results.json"), "w"), indent=1, sort_keys=True)
