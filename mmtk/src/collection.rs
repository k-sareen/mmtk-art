use crate::{Art, UPCALLS};
use mmtk::{
    Mutator,
    MutatorContext,
    util::opaque_pointer::*,
    vm::{Collection, GCThreadContext},
};

/// Implements collection trait
pub struct ArtCollection;

impl Collection<Art> for ArtCollection {
    fn stop_all_mutators<F>(_tls: VMWorkerThread, _mutator_visitor: F)
    where
        F: FnMut(&'static mut Mutator<Art>),
    {
        unimplemented!()
    }

    fn resume_mutators(_tls: VMWorkerThread) {
        unimplemented!()
    }

    fn block_for_gc(tls: VMMutatorThread) {
        unsafe { ((*UPCALLS).block_for_gc)(tls) }
    }

    fn spawn_gc_thread(_tls: VMThread, _ctx: GCThreadContext<Art>) {}

    fn prepare_mutator<T: MutatorContext<Art>>(
        _tls_w: VMWorkerThread,
        _tls_m: VMMutatorThread,
        _mutator: &T,
    ) {
        unimplemented!()
    }
}
