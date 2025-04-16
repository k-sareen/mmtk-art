use crate::{
    unlikely, Art, ArtSlot, NodesClosure, RefProcessingPhase, RustBuffer, ScanObjectClosure,
    SlotsClosure, TraceObjectClosure, UPCALLS,
};
use mmtk::{
    scheduler::GCWorker,
    util::{opaque_pointer::VMWorkerThread, Address, ObjectReference},
    vm::*,
    Mutator,
};
#[cfg(feature = "single_worker")]
use mmtk::vm::slot::Slot;

// use std::sync::{atomic::Ordering, Mutex};
use std::sync::Mutex;

/// Current weak reference processing phase we are executing
static CURRENT_WEAK_REF_PHASE: Mutex<RefProcessingPhase> = Mutex::new(RefProcessingPhase::Phase1);

/// Implements scanning trait
pub struct ArtScanning;

/// Return the forwarding bits for a given `ObjectReference`.
fn get_forwarding_status<VM: VMBinding>(object: ObjectReference) -> u8 {
    VM::VMObjectModel::LOCAL_FORWARDING_BITS_SPEC.load_atomic::<VM, u8>(
        object,
        None,
        std::sync::atomic::Ordering::Relaxed,
    )
}

fn is_not_forwarded<VM: VMBinding>(object: ObjectReference) -> bool {
    get_forwarding_status::<VM>(object) == 0b00
}

impl Scanning<Art> for ArtScanning {
    fn scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<Art>,
        _factory: impl RootsWorkFactory<ArtSlot>,
    ) {
    }

    #[cfg(feature = "single_worker")]
    fn single_threaded_scan_roots_in_mutator_thread(
        _tls: VMWorkerThread,
        _mutator: &'static mut Mutator<Art>,
        _closure: impl ObjectGraphTraversal<ArtSlot>,
    ) {
    }

    fn scan_vm_space_objects(_tls: VMWorkerThread, mut closure: impl FnMut(Vec<ObjectReference>)) {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).scan_vm_space_objects)(to_vm_space_nodes_closure(&mut closure));
        }
    }

    fn scan_vm_specific_roots(_tls: VMWorkerThread, mut factory: impl RootsWorkFactory<ArtSlot>) {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).scan_all_roots)(to_slots_closure(&mut factory));
        }
    }

    #[cfg(feature = "single_worker")]
    fn single_threaded_scan_vm_specific_roots(
        _tls: VMWorkerThread,
        mut closure: impl ObjectGraphTraversal<ArtSlot>,
    ) {
        // SAFETY: Assumes upcalls is valid
        unsafe {
            ((*UPCALLS).scan_all_roots)(object_graph_traversal_to_slots_closure(&mut closure));
        }
    }

    fn scan_object<SV: SlotVisitor<ArtSlot>>(
        tls: VMWorkerThread,
        object: ObjectReference,
        slot_visitor: &mut SV,
    ) {
        debug_assert!(
            is_not_forwarded::<Art>(object),
            "Object {} has forwarding bits {:#02b}",
            object,
            get_forwarding_status::<Art>(object),
        );
        if unlikely(cfg!(feature = "simple_scan_object")) {
            // SAFETY: Assumes upcalls is valid
            unsafe {
                ((*UPCALLS).scan_object)(object, to_scan_object_closure::<SV>(slot_visitor));
            }
        } else {
            crate::object_scanning::scan_object::</* kVisitNativeRoots= */ true, SV>(
                tls,
                object,
                slot_visitor,
            )
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
            let tls = worker.tls;
            // Always clear soft references if we are the Zygote process
            let clear_soft_references =
                crate::api::mmtk_is_emergency_collection() || crate::api::mmtk_is_zygote_process();
            let mut current_phase = CURRENT_WEAK_REF_PHASE.lock().unwrap();
            match *current_phase {
                RefProcessingPhase::Phase1 => {
                    tracer_context.with_tracer(worker, |tracer| {
                        // SAFETY: Assumes upcalls is valid
                        unsafe {
                            ((*UPCALLS).process_references)(
                                tls,
                                TraceObjectClosure::from_rust_closure(&mut |object| {
                                    tracer.trace_object(object)
                                }),
                                RefProcessingPhase::Phase1,
                                clear_soft_references,
                            );
                        }
                    });
                    *current_phase = RefProcessingPhase::Phase2;
                    true
                }
                RefProcessingPhase::Phase2 => {
                    tracer_context.with_tracer(worker, |tracer| {
                        // SAFETY: Assumes upcalls is valid
                        unsafe {
                            ((*UPCALLS).process_references)(
                                tls,
                                TraceObjectClosure::from_rust_closure(&mut |object| {
                                    tracer.trace_object(object)
                                }),
                                RefProcessingPhase::Phase2,
                                clear_soft_references,
                            );
                        }
                    });
                    *current_phase = RefProcessingPhase::Phase3;
                    true
                }
                RefProcessingPhase::Phase3 => {
                    tracer_context.with_tracer(worker, |tracer| {
                        // SAFETY: Assumes upcalls is valid
                        unsafe {
                            ((*UPCALLS).process_references)(
                                tls,
                                TraceObjectClosure::from_rust_closure(&mut |object| {
                                    tracer.trace_object(object)
                                }),
                                RefProcessingPhase::Phase3,
                                clear_soft_references,
                            );
                        }
                    });
                    *current_phase = RefProcessingPhase::Phase1;
                    false
                }
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
const WORK_PACKET_CAPACITY: usize = mmtk::scheduler::EDGES_WORK_BUFFER_SIZE;

/// Create a buffer of size `length` and capacity `capacity`. This buffer is
/// used for reporting nodes to MMTk. The C++ code should store nodes in the
/// buffer carefully to avoid segfaulting.
#[allow(unused)]
extern "C" fn report_nodes_and_renew_buffer<F: RootsWorkFactory<ArtSlot>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        // SAFETY: Assumes ptr is valid
        let buf = unsafe {
            Vec::<ObjectReference>::from_raw_parts(std::mem::transmute(ptr), length, capacity)
        };
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
extern "C" fn report_slots_and_renew_buffer<F: RootsWorkFactory<ArtSlot>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    factory_ptr: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        // SAFETY: Assumes ptr is valid
        let buf =
            unsafe { Vec::<ArtSlot>::from_raw_parts(std::mem::transmute(ptr), length, capacity) };
        // SAFETY: Assumes factory_ptr is valid
        let factory: &mut F = unsafe { &mut *(factory_ptr as *mut F) };
        factory.create_process_roots_work(buf);
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

#[cfg(feature = "single_worker")]
extern "C" fn object_graph_traversal_report_roots<S: Slot, C: ObjectGraphTraversal<S>>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    traverse_func: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        // SAFETY: Assumes ptr is valid
        let buf = unsafe { Vec::<S>::from_raw_parts(ptr as _, length, capacity) };
        // SAFETY: Assumes traverse_func is valid
        let traverse_func: &mut C = unsafe { &mut *(traverse_func as *mut C) };
        traverse_func.report_roots(buf);
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

#[cfg(feature = "single_worker")]
pub(crate) fn object_graph_traversal_to_slots_closure<S: Slot, C: ObjectGraphTraversal<S>>(
    closure: &mut C,
) -> SlotsClosure {
    SlotsClosure {
        func: object_graph_traversal_report_roots::<S, C>,
        data: closure as *mut C as *mut libc::c_void,
    }
}

/// Create a buffer of size `length` and capacity `capacity`. This buffer is
/// used for reporting VM space objects to MMTk. The C++ code should store slots
/// in the buffer carefully to avoid segfaulting.
extern "C" fn report_vm_space_nodes_and_renew_buffer<F: FnMut(Vec<ObjectReference>)>(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
    closure_ptr: *mut libc::c_void,
) -> RustBuffer {
    if !ptr.is_null() {
        // SAFETY: Assumes ptr is valid
        let buf = unsafe {
            Vec::<ObjectReference>::from_raw_parts(std::mem::transmute(ptr), length, capacity)
        };
        // SAFETY: Assumes closure_ptr is valid
        let closure: &mut F = unsafe { &mut *(closure_ptr as *mut F) };
        closure(buf);
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
extern "C" fn scan_object_fn<SV: SlotVisitor<ArtSlot>>(
    slot: ArtSlot,
    slot_visitor_ptr: *mut libc::c_void,
) {
    // SAFETY: Assumes slot_visitor_ptr is valid
    let slot_visitor: &mut SV = unsafe { &mut *(slot_visitor_ptr as *mut SV) };
    slot_visitor.visit_slot(slot);
}

/// Convert a `RootsWorkFactory` into a `NodesClosure`
#[allow(unused)]
pub(crate) fn to_nodes_closure<F: RootsWorkFactory<ArtSlot>>(factory: &mut F) -> NodesClosure {
    NodesClosure {
        func: report_nodes_and_renew_buffer::<F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}

/// Convert a `RootsWorkFactory` into a `NodesClosure` for VM space objects
pub(crate) fn to_vm_space_nodes_closure<F: FnMut(Vec<ObjectReference>)>(
    closure: &mut F,
) -> NodesClosure {
    NodesClosure {
        func: report_vm_space_nodes_and_renew_buffer::<F>,
        data: closure as *mut F as *mut libc::c_void,
    }
}

/// Convert a `RootsWorkFactory` into a `SlotsClosure`
pub(crate) fn to_slots_closure<F: RootsWorkFactory<ArtSlot>>(factory: &mut F) -> SlotsClosure {
    SlotsClosure {
        func: report_slots_and_renew_buffer::<F>,
        data: factory as *mut F as *mut libc::c_void,
    }
}

/// Convert an `SlotVisitor` to a `ScanObjectClosure`
pub(crate) fn to_scan_object_closure<SV: SlotVisitor<ArtSlot>>(
    slot_visitor: &mut SV,
) -> ScanObjectClosure {
    ScanObjectClosure {
        func: scan_object_fn::<SV>,
        data: slot_visitor as *mut SV as *mut libc::c_void,
    }
}
