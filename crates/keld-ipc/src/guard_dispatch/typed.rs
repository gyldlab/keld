//! Privileged dispatch for the closed set of Guard queries.

use keld_guard::{
    Decision, DenyReason, GuardQuery, PermissionsManifest, Principal, evaluate_query,
    json_pointer_for,
};

use super::filesystem_dispatch_path_is_valid;

/// Evaluates the query and lends its own permit to the handler once on Allow.
///
/// The caller supplies the authenticated principal and the Host's current window
/// selection. This function performs no authentication or transport I/O.
///
/// # Errors
///
/// Returns the Guard denial or a native filesystem path grammar denial. Native
/// principal refusal precedes path grammar, which precedes ordinary grant errors.
///
/// A renderer query cannot invoke a native permit handler:
///
/// ```compile_fail,E0631
/// # use keld_guard::{PermissionsManifest, Principal, RendererChannelQuery, ScopePermit};
/// # use keld_ipc::guard_dispatch::typed::dispatch_privileged;
/// # fn check(m: &PermissionsManifest, p: Principal, q: RendererChannelQuery<'_, '_>) {
/// let _ = dispatch_privileged(m, p, q, ScopePermit::grant_index);
/// # }
/// ```
///
/// A native query cannot invoke a renderer permit handler:
///
/// ```compile_fail,E0631
/// # use keld_guard::{NativeCapabilityQuery, PermissionsManifest, Principal, RendererChannelPermit};
/// # use keld_ipc::guard_dispatch::typed::dispatch_privileged;
/// # fn check(m: &PermissionsManifest, p: Principal, q: NativeCapabilityQuery<'_>) {
/// let _ = dispatch_privileged(m, p, q, |_: &RendererChannelPermit| ());
/// # }
/// ```
///
/// A callback cannot return either kind of permit borrow:
///
/// ```compile_fail
/// # use keld_guard::{NativeCapabilityQuery, PermissionsManifest, Principal};
/// # use keld_ipc::guard_dispatch::typed::dispatch_privileged;
/// # fn check(m: &PermissionsManifest, p: Principal, q: NativeCapabilityQuery<'_>) {
/// let _escaped = dispatch_privileged(m, p, q, |permit| permit);
/// # }
/// ```
///
/// ```compile_fail
/// # use keld_guard::{PermissionsManifest, Principal, RendererChannelQuery};
/// # use keld_ipc::guard_dispatch::typed::dispatch_privileged;
/// # fn check(m: &PermissionsManifest, p: Principal, q: RendererChannelQuery<'_, '_>) {
/// let _escaped = dispatch_privileged(m, p, q, |permit| permit);
/// # }
/// ```
pub fn dispatch_privileged<Q: GuardQuery, T>(
    manifest: &PermissionsManifest,
    principal: Principal,
    query: Q,
    handler: impl for<'permit> FnOnce(&'permit Q::Permit) -> T,
) -> Result<T, DenyReason> {
    let decision = match evaluate_query(manifest, principal, query) {
        Decision::Deny(reason @ DenyReason::NotAppProcess { .. }) => return Err(reason),
        decision => decision,
    };
    if let Some(native) = query.native_capability()
        && !filesystem_dispatch_path_is_valid(native.operation, native.path)
    {
        return Err(DenyReason::OutOfScope {
            capability: native.operation.to_owned(),
            scope: "absolute normalized filesystem request".to_owned(),
            json_pointer: json_pointer_for(native.operation),
            requested: native.path.to_owned(),
        });
    }
    match decision {
        Decision::Allow(permit) => Ok(handler(&permit)),
        Decision::Deny(reason) => Err(reason),
    }
}
