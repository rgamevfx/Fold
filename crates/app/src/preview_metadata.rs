//! UI queries over immutable roots. Retain only committed/transient roots and
//! bounded output descriptions; time and transport state are never cached.
use fold_foundation::DocumentId;
use fold_project::{DocumentRef, Snapshot};
use std::collections::{BTreeMap, VecDeque};

struct Root {
    snapshot: Snapshot,
    content: BTreeMap<DocumentId, Result<String, String>>,
    outputs: Vec<(DocumentRef, Result<fold_media::VideoInfo, String>)>,
}
#[derive(Default)]
pub(super) struct Metadata {
    roots: VecDeque<Root>,
}
impl Metadata {
    fn root(&mut self, snapshot: &Snapshot) -> &mut Root {
        if let Some(index) = self
            .roots
            .iter()
            .position(|root| std::ptr::eq(root.snapshot.state(), snapshot.state()))
        {
            let root = self.roots.remove(index).unwrap();
            self.roots.push_front(root);
        } else {
            self.roots.truncate(1);
            self.roots.push_front(Root {
                snapshot: snapshot.clone(),
                content: BTreeMap::new(),
                outputs: Vec::new(),
            });
        }
        self.roots.front_mut().unwrap()
    }
    pub fn content(&mut self, snapshot: &Snapshot, document: DocumentId) -> Result<String, String> {
        let root = self.root(snapshot);
        if !root.content.contains_key(&document) && root.content.len() == 128 {
            root.content.pop_first();
        }
        root.content
            .entry(document)
            .or_insert_with(|| crate::media_workflow::content_for(snapshot, document))
            .clone()
    }
    pub fn output(
        &mut self,
        snapshot: &Snapshot,
        reference: &DocumentRef,
    ) -> Result<fold_media::VideoInfo, String> {
        let root = self.root(snapshot);
        if let Some((_, info)) = root.outputs.iter().find(|(key, _)| key == reference) {
            return info.clone();
        }
        let info = crate::packages::builtins().output_ref(snapshot, reference);
        if root.outputs.len() == 128 {
            let _ = root.outputs.remove(0);
        }
        root.outputs.push((reference.clone(), info.clone()));
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_project::{EditBatch, Mutation, Project};
    #[test]
    fn transient_roots_with_the_same_revision_and_undo_do_not_reuse_stale_queries() {
        let mut project = Project::new(8);
        let id = DocumentId::new();
        let motion = fold_motion::Motion::empty();
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(motion.document(id).unwrap())],
            })
            .unwrap();
        let committed = project.snapshot();
        let output = DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        };
        let mut cache = Metadata::default();
        let original = cache.content(&committed, id).unwrap();
        let original_info = cache.output(&committed, &output).unwrap();
        let mut edit = project.begin_edit();
        let mut changed = motion.clone();
        changed.info.width += 1;
        project
            .update_edit(
                &mut edit,
                vec![Mutation::PutDocument(changed.document(id).unwrap())],
            )
            .unwrap();
        let transient = edit.snapshot();

        assert_ne!(original, cache.content(&transient, id).unwrap());
        assert_eq!(
            cache.output(&transient, &output).unwrap().width,
            original_info.width + 1
        );
        let first_edit = cache.content(&transient, id).unwrap();
        changed.info.width += 1;
        project
            .update_edit(
                &mut edit,
                vec![Mutation::PutDocument(changed.document(id).unwrap())],
            )
            .unwrap();
        let revised_edit = edit.snapshot();
        assert_eq!(transient.revision(), revised_edit.revision());
        assert_ne!(cache.content(&revised_edit, id).unwrap(), first_edit);
        assert_eq!(
            cache.output(&revised_edit, &output).unwrap().width,
            original_info.width + 2
        );
        assert_eq!(cache.content(&committed, id).unwrap(), original);
        project
            .commit(EditBatch {
                base: committed.revision(),
                mutations: vec![Mutation::PutDocument(changed.document(id).unwrap())],
            })
            .unwrap();
        assert_ne!(cache.content(&project.snapshot(), id).unwrap(), original);
        project.undo().unwrap();
        assert_eq!(cache.content(&project.snapshot(), id).unwrap(), original);
        assert_eq!(
            cache.output(&project.snapshot(), &output).unwrap().width,
            original_info.width
        );
        assert_eq!(cache.roots.len(), 2);
    }
}
