//! The existing graph is the Runtime preparer's retained source, not a new
//! selector, admission policy, readiness flag, or recovery owner.
use crate::app_session::AdmittedStageGraph;

impl keld_runtime::primary::RoleSourceRecheck for AdmittedStageGraph {
    fn recheck(&self) -> std::io::Result<()> {
        // The same canonical graph/root/selected-file checks used at startup
        // run before every attempted generation, including recovered Bun.
        AdmittedStageGraph::recheck(self).map_err(|error| std::io::Error::other(error.to_string()))
    }
}
