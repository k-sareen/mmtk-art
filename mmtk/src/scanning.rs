use crate::{
    Art,
    ArtEdge,
    NodesClosure,
    RustBuffer,
    UPCALLS,
};
use mmtk::{
    scheduler::GCWorker,
    Mutator,
    util::{
        Address,
        ObjectReference,
        opaque_pointer::VMWorkerThread,
    },
    vm::*,
};
use std::cell::UnsafeCell;

/// Implements scanning trait
pub struct ArtScanning;

thread_local! {
    /// Thread-local instance of the EdgeVisitor closure
    static CLOSURE: UnsafeCell<*mut u8> = UnsafeCell::new(std::ptr::null_mut());
}

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
        object: ObjectReference,
        edge_visitor: &mut EV,
    ) {
        unsafe {
            CLOSURE.with(|x| *x.get() = edge_visitor as *mut EV as *mut u8);
            ((*UPCALLS).scan_object)(
                object,
                scan_object_fn::<EV> as *const unsafe extern "C" fn(edge: ArtEdge),
            );
        }
    }

    fn process_weak_refs(
        worker: &mut GCWorker<Art>,
        _tracer_context: impl mmtk::vm::ObjectTracerContext<Art>,
    ) -> bool {
        // System weaks have to be swept after the transitive closure, but
        // before GC reclaims objects
        unsafe {
            ((*UPCALLS).sweep_system_weaks)(worker.tls);
        }
        false
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

/// Function that allows C/C++ code to invoke the `EdgeVisitor` closure
///
/// # Safety
/// This function is unsafe if the function pointer to the closure is not to an
/// `EdgeVisitor`. It is the responsibility of the caller of this function to
/// ensure that the closure has been registered properly.
pub unsafe extern "C" fn scan_object_fn<EV: EdgeVisitor<ArtEdge>>(edge: ArtEdge) {
    let ptr: *mut u8 = CLOSURE.with(|x| *x.get());
    let closure = &mut *(ptr as *mut EV);
    closure.visit_edge(edge);
}
