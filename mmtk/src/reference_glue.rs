use crate::Art;
use mmtk::{
    util::{ObjectReference, opaque_pointer::VMWorkerThread},
    vm::ReferenceGlue,
};

/// Implements reference glue trait
pub struct ArtReferenceGlue;

impl ReferenceGlue<Art> for ArtReferenceGlue {
    type FinalizableType = ObjectReference;

    fn set_referent(_reference: ObjectReference, _referent: ObjectReference) {
        unimplemented!()
    }

    fn get_referent(_object: ObjectReference) -> ObjectReference {
        unimplemented!()
    }

    fn enqueue_references(_references: &[ObjectReference], _tls: VMWorkerThread) {
        unimplemented!()
    }
}
