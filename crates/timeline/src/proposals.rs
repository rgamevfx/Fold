//! Timeline proposal guards shared by agents and cross-document placement.
use crate::Sequence;
use fold_foundation::ObjectId;
use fold_media::Cancel;
use fold_platform::proposals::*;
use fold_project::Document;
use serde_json::{Value, json};

pub(crate) static CAPABILITY: AuthoringProposals<Records> =
    AuthoringProposals::new(author, crate::automation::catalog);
fn author(d: &Document, edits: &[Value], cancel: &Cancel) -> Result<Document, String> {
    cancel.check()?;
    crate::automation::author(d, edits)
}
pub(crate) struct Records(Sequence);
fn same<T: serde::Serialize>(a: &T, b: &T) -> bool {
    json!(a) == json!(b)
}
impl fold_platform::proposals::Records for Records {
    fn undo_label(&self) -> &'static str {
        "Edit Timeline"
    }
    fn decode(d: &Document) -> Result<Self, Failure> {
        Sequence::from_document(d)
            .map(Self)
            .map_err(Failure::invalid)
    }
    fn validate_change(before: &Self, after: &Self, _: Scope) -> Result<(), Failure> {
        let protected = || {
            Failure::new(
                FailureKind::ProtectedEdit,
                "Cannot change a locked track or discard Timeline extensions",
            )
        };
        if before.0.extensions != after.0.extensions {
            return Err(protected());
        }
        for track in &before.0.tracks {
            let next = after.0.tracks.iter().find(|v| v.id == track.id);
            if next.is_some_and(|v| v.extensions != track.extensions) {
                return Err(protected());
            }
            if track.locked {
                if !same(&Some(track), &next) {
                    return Err(protected());
                }
                let clips = |s: &Sequence| {
                    json!(
                        s.clips
                            .iter()
                            .filter(|v| v.track == track.id)
                            .collect::<Vec<_>>()
                    )
                };
                if clips(&before.0) != clips(&after.0) {
                    return Err(protected());
                }
                let transitions = |s: &Sequence| {
                    json!(
                        s.transitions
                            .iter()
                            .filter(|t| before.0.clips.iter().any(|c| c.track == track.id
                                && (c.id == t.outgoing || c.id == t.incoming)))
                            .collect::<Vec<_>>()
                    )
                };
                if transitions(&before.0) != transitions(&after.0) {
                    return Err(protected());
                }
            }
        }
        for clip in &before.0.clips {
            if let Some(next) = after.0.clips.iter().find(|v| v.id == clip.id) {
                if next.extensions != clip.extensions {
                    return Err(protected());
                }
                for effect in &clip.effects {
                    if let Some(next) = next.effects.iter().find(|v| v.id == effect.id)
                        && next.extensions != effect.extensions
                    {
                        return Err(protected());
                    }
                }
            }
        }
        Ok(())
    }
    fn reconcile(_: &Self, _: &Self, _: &Self, _: Scope) -> Result<Self, Failure> {
        Err(Failure::new(
            FailureKind::Conflict,
            "Edited Timeline changed",
        ))
    }
    fn document(&self, original: &Document) -> Result<Document, Failure> {
        let mut d = self.0.document(original.id).map_err(Failure::invalid)?;
        d.extensions = original.extensions.clone();
        Ok(d)
    }
    fn source(&self, _: Scope) -> Result<DocumentInspection, Failure> {
        Ok(DocumentInspection::Native(json!({"native":self.0})))
    }
    fn diff(before: Option<&Self>, after: &Self, scope: Scope) -> Result<Vec<Value>, Failure> {
        json_diff(before.map(|v| &v.0), &after.0, scope)
    }
    fn affected(before: Option<&Self>, after: &Self, _: Scope) -> Result<Vec<ObjectId>, Failure> {
        Ok(before
            .into_iter()
            .chain(std::iter::once(after))
            .flat_map(|s| {
                s.0.tracks
                    .iter()
                    .map(|v| v.id)
                    .chain(s.0.clips.iter().map(|v| v.id))
                    .chain(s.0.transitions.iter().map(|v| v.id))
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect())
    }
}
