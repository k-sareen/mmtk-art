use crate::{
    Art,
    ArtEdge,
    NodesClosure,
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
    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<Art>,
        _factory: impl RootsWorkFactory<ArtEdge>,
    ) {}

    fn scan_vm_specific_roots(_tls: VMWorkerThread, mut factory: impl RootsWorkFactory<ArtEdge>) {
        unsafe {
            ((*UPCALLS).scan_all_roots)(to_nodes_closure(&mut factory));
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
/// used for reporting nodes to MMTk. The C++ code should store nodes in the
/// buffer carefully to avoid segfaulting.
extern "C" fn report_nodes_and_renew_buffer<F: RootsWorkFactory<ArtEdge>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        let buf = unsafe { Vec::<ObjectReference>::from_raw_parts(std::mem::transmute(ptr), length, capacity) };
        let factory: &mut F = unsafe { &mut *(factory_ptr as *mut F) };
        factory.create_process_pinning_roots_work(buf);
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

/// Convert a RootsWorkFactory into an NodesClosure
pub(crate) fn to_nodes_closure<F: RootsWorkFactory<ArtEdge>>(factory: &mut F) -> NodesClosure {
    NodesClosure {
        func: report_nodes_and_renew_buffer::<F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}
