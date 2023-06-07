use crate::{
    Art,
    ArtUpcalls,
    BUILDER,
    SINGLETON,
    UPCALLS,
};
use mmtk::{
    AllocationSemantics,
    Mutator,
    scheduler::{
        GCController,
        GCWorker,
    },
    util::{
        Address,
        ObjectReference,
        opaque_pointer::*,
    },
};
use std::sync::atomic::Ordering;

/// Initialize MMTk instance
#[no_mangle]
pub extern "C" fn mmtk_init(upcalls: *const ArtUpcalls) {
    unsafe { UPCALLS = upcalls };
    // Make sure that we haven't initialized MMTk (by accident) yet
    assert!(!crate::IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    // Make sure we initialize MMTk here
    lazy_static::initialize(&SINGLETON)
}

/// Initialize collection for MMTk
#[no_mangle]
pub extern "C" fn mmtk_initialize_collection(tls: VMThread) {
    mmtk::memory_manager::initialize_collection(&SINGLETON, tls)
}

/// Set the min and max heap size
#[no_mangle]
pub extern "C" fn mmtk_set_heap_size(min: usize, max: usize) -> bool {
    use mmtk::util::options::GCTriggerSelector;
    let mut builder = BUILDER.lock().unwrap();
    let policy = if min == max {
        GCTriggerSelector::FixedHeapSize(min)
    } else {
        // TODO(kunals): Allow dynamic heap sizes when we are able to collect garbage
        // GCTriggerSelector::DynamicHeapSize(min, max)
        GCTriggerSelector::FixedHeapSize(max)
    };
    builder.options.gc_trigger.set(policy)
}

/// Start the GC Controller thread. We trust the `gc_collector` pointer is valid
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_start_gc_controller_thread(
    tls: VMWorkerThread,
    gc_controller: *mut GCController<Art>,
) {
    let mut gc_controller = unsafe { Box::from_raw(gc_controller) };
    mmtk::memory_manager::start_control_collector(&SINGLETON, tls, &mut gc_controller);
}

/// Start a GC Worker thread. We trust the `worker` pointer is valid.
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_start_gc_worker_thread(
    tls: VMWorkerThread,
    worker: *mut GCWorker<Art>
) {
    let mut worker = unsafe { Box::from_raw(worker) };
    mmtk::memory_manager::start_worker::<Art>(&SINGLETON, tls, &mut worker)
}

/// Bind a mutator thread in MMTk
#[no_mangle]
pub extern "C" fn mmtk_bind_mutator(tls: VMMutatorThread) -> *mut Mutator<Art> {
    Box::into_raw(mmtk::memory_manager::bind_mutator(&SINGLETON, tls))
}

/// Allocate an object of `size` using MMTk
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_alloc(
    mutator: *mut Mutator<Art>,
    size: usize,
    align: usize,
    offset: isize,
    allocator: AllocationSemantics,
) -> Address {
    mmtk::memory_manager::alloc::<Art>(
        unsafe { &mut *mutator },
        size,
        align,
        offset,
        allocator
    )
}

/// Set relevant object metadata in MMTk
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_post_alloc(
    mutator: *mut Mutator<Art>,
    object: ObjectReference,
    size: usize,
    allocator: AllocationSemantics,
) {
    mmtk::memory_manager::post_alloc::<Art>(
        unsafe { &mut *mutator },
        object,
        size,
        allocator
    )
}

/// Return if an object is allocated by MMTk
#[no_mangle]
pub extern "C" fn mmtk_is_object_in_heap_space(object: ObjectReference) -> bool {
    mmtk::memory_manager::is_in_mmtk_spaces::<Art>(
        object,
    )
}

/// Get starting heap address
#[no_mangle]
pub extern "C" fn mmtk_get_heap_start() -> Address {
    mmtk::memory_manager::starting_heap_address()
}

/// Get ending heap address
#[no_mangle]
pub extern "C" fn mmtk_get_heap_end() -> Address {
    mmtk::memory_manager::last_heap_address()
}

/// Check if given address is a valid object
#[no_mangle]
pub extern "C" fn mmtk_is_valid_object(addr: Address) -> bool {
    mmtk::memory_manager::is_mmtk_object(addr)
}
