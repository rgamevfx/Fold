//! Provider measurements over immutable documents; App owns scheduling/rendering.
use crate::proposals::Failure;
use fold_foundation::{ObjectId, Time};
use fold_media::Cancel;
use fold_project::{Document, Revision, proposals::Handle};
use serde_json::Value;
pub struct Request {
    pub epoch: ObjectId,
    pub revision: Revision,
    pub candidate: Option<Handle>,
    pub output: String,
    pub times: Vec<Time>,
    pub build: String,
    pub target: String,
}
pub trait Observations: Send + Sync {
    fn measure_with_images(
        &self,
        document: &Document,
        request: &Request,
        cancel: &Cancel,
        _images: &dyn crate::image_sampling::Resolver,
    ) -> Result<Value, Failure> {
        self.measure(document, request, cancel)
    }
    fn frame_provenance_with_images(
        &self,
        document: &Document,
        request: &Request,
        cancel: &Cancel,
        _images: &dyn crate::image_sampling::Resolver,
    ) -> Result<Value, Failure> {
        self.frame_provenance(document, request, cancel)
    }

    fn measure(
        &self,
        document: &Document,
        request: &Request,
        cancel: &Cancel,
    ) -> Result<Value, Failure>;
    fn frame_provenance(
        &self,
        document: &Document,
        request: &Request,
        cancel: &Cancel,
    ) -> Result<Value, Failure>;
}
