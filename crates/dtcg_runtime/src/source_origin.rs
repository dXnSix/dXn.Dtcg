use crate::ResourceId;

/// An authored JSON location. A missing resource identifies the inline source
/// supplied to `DtcgRuntime::inspect_source`, rather than a catalog document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceOrigin {
    pub(crate) resource: Option<ResourceId>,
    pub(crate) pointer: String,
}

impl SourceOrigin {
    pub fn resource(&self) -> Option<&ResourceId> {
        self.resource.as_ref()
    }

    /// JSON Pointer within the originating document (empty for its root).
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    pub(crate) fn child(&self, name: &str) -> Self {
        Self {
            resource: self.resource.clone(),
            pointer: format!(
                "{}/{}",
                self.pointer,
                name.replace('~', "~0").replace('/', "~1")
            ),
        }
    }
}
