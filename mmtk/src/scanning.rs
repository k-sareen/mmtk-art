use crate::{
    Art,
    ArtEdge,
    EdgesClosure,
    RustBuffer,
    UPCALLS,
};
use mmtk::{
    Mutator,
    util::{
        Address,
        ObjectReference,
        opaque_pointer::VMWorkerThread,
    },
    vm::*,
};

/// Implements scanning trait
pub struct ArtScanning;

impl Scanning<Art> for ArtScanning {
    // ART doesn't differentiate between different roots (or well it does, but
    // since it uses a visitor pattern, it's a bit annoying to differentiate
    // between different kinds of roots for MMTk). Hence, we scan and report all
    // roots in the `scan_vm_specific_roots` function. Since ART uses the same
    // visitor for all roots, it is safe to do so as we retain the semantics
    // that all roots are treated the same.
    const SCAN_MUTATORS_IN_SAFEPOINT: bool = false;

    fn scan_roots_in_all_mutator_threads(
        _tls: VMWorkerThread,
        _factory: impl RootsWorkFactory<ArtEdge>
    ) {}

    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<Art>,
        _factory: impl RootsWorkFactory<ArtEdge>,
    ) {}

    fn scan_vm_specific_roots(_tls: VMWorkerThread, mut factory: impl RootsWorkFactory<ArtEdge>) {
        unsafe {
            ((*UPCALLS).scan_all_roots)(to_edges_closure(&mut factory));
        }
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

/// Maximum capacity of work packet
const WORK_PACKET_CAPACITY: usize = 4096;

/// Create a buffer of size `length` and capacity `capacity`. This buffer is
/// used for reporting edges to MMTk. The C++ code should store slots in the
/// buffer carefully to avoid segfaulting.
extern "C" fn report_edges_and_renew_buffer<F: RootsWorkFactory<ArtEdge>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        let buf = unsafe { Vec::<ArtEdge>::from_raw_parts(std::mem::transmute(ptr), length, capacity) };
        let factory: &mut F = unsafe { &mut *(factory_ptr as *mut F) };
        factory.create_process_edge_roots_work(buf);
    }
    let (ptr, _, capacity) = {
        // TODO: Use Vec::into_raw_parts() when the method is available.
        use std::mem::ManuallyDrop;
        let new_vec = Vec::with_capacity(WORK_PACKET_CAPACITY);
        let mut me = ManuallyDrop::new(new_vec);
        (me.as_mut_ptr(), me.len(), me.capacity())
    };
    RustBuffer { ptr, capacity }
}

/// Convert a RootsWorkFactory into an EdgesClosure
pub(crate) fn to_edges_closure<F: RootsWorkFactory<ArtEdge>>(factory: &mut F) -> EdgesClosure {
    EdgesClosure {
        func: report_edges_and_renew_buffer::<F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}
