//! Motion proposals use the shared, feature-neutral candidate lifecycle.
use crate::{editing, model::Construction, source};
pub use fold_project::proposals::{
    Cancellation, Handle, IDLE_LIFETIME, InspectionProof, MAX_LIVE, Receipt,
};
pub type Store = fold_project::proposals::Store<editing::Candidate>;
pub type Error = fold_project::proposals::Error<editing::Error>;
pub type Inspection = fold_project::proposals::Inspection<crate::diff::Change>;
pub type InspectionTask = fold_project::proposals::InspectionTask<editing::Candidate>;
impl fold_project::proposals::Proposal for editing::Candidate {
    type Scope = editing::Scope;
    type Error = editing::Error;
    type Change = crate::diff::Change;
    fn scope(&self) -> Self::Scope {
        self.scope()
    }
    fn base_revision(&self) -> fold_project::Revision {
        self.base_revision()
    }
    fn document(&self) -> &fold_project::Document {
        self.document()
    }
    fn source(&self) -> Result<String, Self::Error> {
        let value =
            Construction::from_document(self.document()).map_err(editing::Error::Invalid)?;
        source::canonical(&value).map_err(editing::Error::Invalid)
    }
    fn diff(&self) -> Result<Vec<Self::Change>, Self::Error> {
        self.diff()
    }
    fn rebase(&self, current: &fold_project::CommittedSnapshot) -> Result<Self, Self::Error> {
        self.rebase(current)
    }
    fn apply(
        self,
        project: &mut fold_project::Project,
    ) -> Result<fold_project::CommittedSnapshot, Self::Error> {
        self.apply(project)
    }
}
