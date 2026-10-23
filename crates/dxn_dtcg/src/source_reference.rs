use crate::{DocumentReference, SourceOrigin};

/// An authored resolver-source `$ref`, excluding token aliases and unknown
/// extension data. The origin points to the reference string itself.
#[derive(Clone, Debug)]
pub struct SourceReference {
    pub(crate) origin: SourceOrigin,
    pub(crate) reference: DocumentReference,
}

impl SourceReference {
    pub fn origin(&self) -> &SourceOrigin {
        &self.origin
    }

    pub fn reference(&self) -> &DocumentReference {
        &self.reference
    }
}
