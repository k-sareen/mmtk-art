use crate::{Art, ArtEdge};
use mmtk::{
    Mutator,
    util::{ObjectReference, opaque_pointer::VMWorkerThread},
    vm::*,
};

/// Implements scanning trait
pub struct ArtScanning;

impl Scanning<Art> for ArtScanning {
    fn scan_thread_roots(_tls: VMWorkerThread, _factory: impl RootsWorkFactory<ArtEdge>) {
        unimplemented!()
    }

    fn scan_thread_root(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<Art>,
        _factory: impl RootsWorkFactory<ArtEdge>,
    ) {
        unimplemented!()
    }

    fn scan_vm_specific_roots(_tls: VMWorkerThread, _factory: impl RootsWorkFactory<ArtEdge>) {
        unimplemented!()
    }

    fn scan_object<EV: EdgeVisitor<ArtEdge>>(
        _tls: VMWorkerThread,
        _object: ObjectReference,
        _edge_visitor: &mut EV,
    ) {
        unimplemented!()
    }

    fn notify_initial_thread_scan_complete(_partial_scan: bool, _tls: VMWorkerThread) {
        unimplemented!()
    }

    fn supports_return_barrier() -> bool {
        unimplemented!()
    }

    fn prepare_for_roots_re_scanning() {
        unimplemented!()
    }
}
