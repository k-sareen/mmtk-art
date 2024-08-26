use crate::{
    Art,
    ArtEdge,
    NodesClosure,
    RustBuffer,
    ScanObjectClosure,
    SlotsClosure,
    TraceObjectClosure,
    UPCALLS,
    RefProcessingPhase,
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
use std::sync::Mutex;

/// Current weak reference processing phase we are executing
static CURRENT_WEAK_REF_PHASE: Mutex<RefProcessingPhase> = Mutex::new(RefProcessingPhase::Phase1);

/// Implements scanning trait
pub struct ArtScanning;

impl Scanning<Art> for ArtScanning {
    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<Art>,
        _factory: impl RootsWorkFactory<ArtEdge>,
    ) {}

    fn scan_vm_specific_roots(_tls: VMWorkerThread, mut factory: impl RootsWorkFactory<ArtEdge>) {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).scan_all_roots)(to_slots_closure(&mut factory));
        }
    }

    fn scan_object<EV: EdgeVisitor<ArtEdge>>(
        _tls: VMWorkerThread,
        object: ObjectReference,
        edge_visitor: &mut EV,
    ) {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).scan_object)(
                object,
                to_scan_object_closure::<EV>(edge_visitor),
            );
        }
    }

    fn process_weak_refs(
        worker: &mut GCWorker<Art>,
        tracer_context: impl mmtk::vm::ObjectTracerContext<Art>,
    ) -> bool {
        if crate::api::mmtk_is_nursery_collection() {
            // We need to sweep system weaks after a nursery GC as well to avoid having broken
            // objects inside intern tables (for example) when we sweep system weaks in a global GC
            // SAFETY: Assumes upcalls is valid
            unsafe { ((*UPCALLS).sweep_system_weaks)() };
            false
        } else {
            let mut current_phase = CURRENT_WEAK_REF_PHASE.lock().unwrap();
            match *current_phase {
                RefProcessingPhase::Phase1 => {
                    // SAFETY: Assumes upcalls is valid
                    unsafe {
                        ((*UPCALLS).process_references)(
                            worker.tls,
                            TraceObjectClosure::from_rust_closure(&mut |object| {
                                tracer_context.with_tracer(worker, |tracer| {
                                    tracer.trace_object(object)
                                })
                            }),
                            RefProcessingPhase::Phase1,
                            crate::api::mmtk_is_emergency_collection(),
                        );
                    }
                    *current_phase = RefProcessingPhase::Phase2;
                    true
                },
                RefProcessingPhase::Phase2 => {
                    // SAFETY: Assumes upcalls is valid
                    unsafe {
                        ((*UPCALLS).process_references)(
                            worker.tls,
                            TraceObjectClosure::from_rust_closure(&mut |object| {
                                tracer_context.with_tracer(worker, |tracer| {
                                    tracer.trace_object(object)
                                })
                            }),
                            RefProcessingPhase::Phase2,
                            crate::api::mmtk_is_emergency_collection(),
                        );
                    }
                    *current_phase = RefProcessingPhase::Phase3;
                    true
                },
                RefProcessingPhase::Phase3 => {
                    // SAFETY: Assumes upcalls is valid
                    unsafe {
                        ((*UPCALLS).process_references)(
                            worker.tls,
                            TraceObjectClosure::from_rust_closure(&mut |object| {
                                tracer_context.with_tracer(worker, |tracer| {
                                    tracer.trace_object(object)
                                })
                            }),
                            RefProcessingPhase::Phase3,
                            crate::api::mmtk_is_emergency_collection(),
                        );
                    }
                    *current_phase = RefProcessingPhase::Phase1;
                    false
                },
            }
        }
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
#[allow(unused)]
extern "C" fn report_nodes_and_renew_buffer<F: RootsWorkFactory<ArtEdge>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        // SAFETY: Assumes ptr is valid
        let buf = unsafe { Vec::<ObjectReference>::from_raw_parts(std::mem::transmute(ptr), length, capacity) };
        // SAFETY: Assumes factory_ptr is valid
        let factory: &mut F = unsafe { &mut *(factory_ptr as *mut F) };
        factory.create_process_pinning_roots_work(buf);
    }
    let (ptr, len, capacity) = {
        // TODO: Use Vec::into_raw_parts() when the method is available.
        use std::mem::ManuallyDrop;
        let new_vec = Vec::with_capacity(WORK_PACKET_CAPACITY);
        let mut me = ManuallyDrop::new(new_vec);
        (me.as_mut_ptr(), me.len(), me.capacity())
    };
    RustBuffer { ptr, len, capacity }
}

/// Create a buffer of size `length` and capacity `capacity`. This buffer is
/// used for reporting slots to MMTk. The C++ code should store slots in the
/// buffer carefully to avoid segfaulting.
extern "C" fn report_slots_and_renew_buffer<F: RootsWorkFactory<ArtEdge>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        // SAFETY: Assumes ptr is valid
        let buf = unsafe { Vec::<ArtEdge>::from_raw_parts(std::mem::transmute(ptr), length, capacity) };
        // SAFETY: Assumes factory_ptr is valid
        let factory: &mut F = unsafe { &mut *(factory_ptr as *mut F) };
        factory.create_process_edge_roots_work(buf);
    }
    let (ptr, len, capacity) = {
        // TODO: Use Vec::into_raw_parts() when the method is available.
        use std::mem::ManuallyDrop;
        let new_vec = Vec::with_capacity(WORK_PACKET_CAPACITY);
        let mut me = ManuallyDrop::new(new_vec);
        (me.as_mut_ptr(), me.len(), me.capacity())
    };
    RustBuffer { ptr, len, capacity }
}

/// Function that allows C/C++ code to invoke a `ScanObjectClosure` closure
extern "C" fn scan_object_fn<EV: EdgeVisitor<ArtEdge>>(
    edge: ArtEdge,
    edge_visitor_ptr: *mut libc::c_void
) {
    // SAFETY: Assumes edge_visitor_ptr is valid
    let edge_visitor: &mut EV = unsafe { &mut *(edge_visitor_ptr as *mut EV) };
    edge_visitor.visit_edge(edge);
}

/// Convert a `RootsWorkFactory` into a `NodesClosure`
#[allow(unused)]
pub(crate) fn to_nodes_closure<F: RootsWorkFactory<ArtEdge>>(
    factory: &mut F
) -> NodesClosure {
    NodesClosure {
        func: report_nodes_and_renew_buffer::<F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}

/// Convert a `RootsWorkFactory` into a `SlotsClosure`
pub(crate) fn to_slots_closure<F: RootsWorkFactory<ArtEdge>>(
    factory: &mut F
) -> SlotsClosure {
    SlotsClosure {
        func: report_slots_and_renew_buffer::<F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}

/// Convert an `EdgeVisitor` to a `ScanObjectClosure`
pub(crate) fn to_scan_object_closure<EV: EdgeVisitor<ArtEdge>>(
    edge_visitor: &mut EV
) -> ScanObjectClosure {
    ScanObjectClosure {
        func: scan_object_fn::<EV>,
        data: edge_visitor as *mut EV as *mut libc::c_void,
    }
}
