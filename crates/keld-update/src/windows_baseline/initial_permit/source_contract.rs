//! Source-only negative controls for D-46's one-use effect boundary.
//!
//! The native issuer is not commissioned, so an executable positive issuer
//! fixture would counterfeit the missing KEL-53 repository lease. These tests
//! inspect only the real permit methods and reject single-fault mutations.
//! They cannot establish real OS identity, cross-install exclusion, or journal
//! durability; those require the commissioned native issuer and native tests.

const SOURCE: &str = include_str!("../initial_permit.rs");

fn code(source: &str) -> String {
    source
        .lines()
        .map(|line| line.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn method<'a>(source: &'a str, name: &str) -> Result<(&'a str, &'a str), &'static str> {
    let signature = format!("pub(super) fn {name}(");
    let start = source.find(&signature).ok_or("D-46 method missing")?;
    let open = source[start..]
        .find('{')
        .map(|at| start + at)
        .ok_or("D-46 method body missing")?;
    let mut depth = 0;
    for (offset, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((&source[start..open], &source[open + 1..open + offset]));
                }
            }
            _ => {}
        }
    }
    Err("D-46 method body incomplete")
}

fn fields<'a>(source: &'a str, name: &str) -> Result<&'a str, &'static str> {
    let signature = format!("pub(super) struct {name}<'lease> {{");
    let start = source.find(&signature).ok_or("D-46 affine type missing")?;
    let open = start + signature.len();
    let close = source[open..]
        .find('}')
        .map(|at| open + at)
        .ok_or("D-46 affine type incomplete")?;
    Ok(&source[open..close])
}

fn required_order(body: &str, steps: &[&str]) -> Result<(), &'static str> {
    let mut remaining = body;
    for step in steps {
        let at = remaining.find(step).ok_or("D-46 boundary order changed")?;
        remaining = &remaining[at + step.len()..];
    }
    Ok(())
}

fn check_boundary(source: &str) -> Result<(), &'static str> {
    for name in ["WindowsInitialInstallPermit", "CreationArmedEffectGate"] {
        let actual = code(fields(source, name)?);
        if !actual.contains("issuer: &'lease mut dyn NativeInitialAdmissionIssuer")
            || !actual.contains("_worker_only: PhantomData<Rc<()>>")
        {
            return Err("D-46 gate lost the unique writer borrow or thread bound");
        }
    }

    let (journal_signature, journal_body) = method(source, "consume_for_admission_journal")?;
    if !code(journal_signature).contains("( self, )") {
        return Err("D-46 permit is not consumed at journal entry");
    }
    let journal = code(journal_body);
    required_order(
        &journal,
        &[
            "thread::current().id() != self.worker",
            "self.issuer.revalidate_under_repository_lease()?;",
            "self.issuer.publish_and_read_back_creation_armed()?;",
            "Ok(CreationArmedEffectGate {",
        ],
    )?;
    if journal
        .matches("self.issuer.revalidate_under_repository_lease()?;")
        .count()
        != 1
        || journal.contains("acquire_bootstrap_lock")
    {
        return Err("D-46 journal entry can perform an effect or skip its only recheck");
    }

    let (effect_signature, effect_body) = method(source, "enter_bootstrap_effect")?;
    if !code(effect_signature).contains("(self, roots: &Roots)") {
        return Err("D-46 armed effect gate is reusable");
    }
    let effect = code(effect_body);
    required_order(
        &effect,
        &[
            "thread::current().id() != self.worker",
            "self.issuer.revalidate_under_repository_lease()?;",
            "self.issuer.require_durable_creation_armed()?;",
            "self.issuer.require_bound_existing_scaffold(roots)?;",
            "self.issuer.revalidate_under_repository_lease()?;",
            "let lock = initialize::acquire_bootstrap_lock(roots)?;",
            "initialize::recheck_bootstrap_scaffold(roots)?;",
            "self.issuer.revalidate_bootstrap_lock(&lock)?;",
            "initialize::recheck_bootstrap_scaffold(roots)?;",
            "self.issuer.publish_and_read_back_bootstrap_owned(&lock)?;",
        ],
    )?;
    if effect
        .matches("self.issuer.revalidate_under_repository_lease()?;")
        .count()
        != 2
        || effect
            .matches("initialize::acquire_bootstrap_lock(roots)?;")
            .count()
            != 1
        || effect
            .matches("initialize::recheck_bootstrap_scaffold(roots)?;")
            .count()
            != 2
        || !effect.contains(
            "self.issuer.revalidate_under_repository_lease()?; let lock = initialize::acquire_bootstrap_lock(roots)?;",
        )
    {
        return Err("D-46 effect-time checks, final handoff or single bootstrap create changed");
    }
    Ok(())
}

#[test]
fn permit_boundary_is_one_use_thread_bound_and_checked_at_effect_time() {
    check_boundary(SOURCE).expect("closed D-46 source contract");
}

#[test]
fn single_fault_permit_mutations_fail_before_a_native_issuer_exists() {
    let mutations = [
        (
            "journal permit replay",
            "pub(super) fn consume_for_admission_journal(\n        self,",
            "pub(super) fn consume_for_admission_journal(\n        &self,",
        ),
        (
            "journal publication removed",
            "self.issuer.publish_and_read_back_creation_armed()?;",
            "let _ = &self.issuer;",
        ),
        (
            "effect gate replay",
            "pub(super) fn enter_bootstrap_effect(self, roots: &Roots)",
            "pub(super) fn enter_bootstrap_effect(&mut self, roots: &Roots)",
        ),
        (
            "durable fence readback removed",
            "self.issuer.require_durable_creation_armed()?;",
            "let _ = &self.issuer;",
        ),
        (
            "foreign scaffold unchecked",
            "self.issuer.require_bound_existing_scaffold(roots)?;",
            "let _ = roots;",
        ),
        (
            "final recheck moved after creation",
            "self.issuer.revalidate_under_repository_lease()?;\n        let lock = initialize::acquire_bootstrap_lock(roots)?;",
            "let lock = initialize::acquire_bootstrap_lock(roots)?;\n        self.issuer.revalidate_under_repository_lease()?;",
        ),
        (
            "bootstrap owned readback removed",
            "self.issuer.publish_and_read_back_bootstrap_owned(&lock)?;",
            "let _ = &lock;",
        ),
        (
            "worker identity inversion at effect",
            "thread::current().id() != self.worker {\n            return Err(error(\n                \"initial install admission\",\n                \"actual worker changed before first protected effect\"",
            "thread::current().id() == self.worker {\n            return Err(error(\n                \"initial install admission\",\n                \"actual worker changed before first protected effect\"",
        ),
        (
            "callback inserted after final revalidation",
            "self.issuer.revalidate_under_repository_lease()?;\n        let lock = initialize::acquire_bootstrap_lock(roots)?;",
            "self.issuer.revalidate_under_repository_lease()?;\n        unrelated_callback();\n        let lock = initialize::acquire_bootstrap_lock(roots)?;",
        ),
    ];

    for (label, before, after) in mutations {
        assert_eq!(
            SOURCE.matches(before).count(),
            1,
            "{label}: exact mutation anchor"
        );
        let altered = SOURCE.replacen(before, after, 1);
        assert_ne!(altered, SOURCE, "{label}: mutation must change real source");
        assert!(
            check_boundary(&altered).is_err(),
            "{label}: mutation was admitted"
        );
    }
}
