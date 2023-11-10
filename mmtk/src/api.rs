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
        alloc::BumpPointer,
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
        GCTriggerSelector::DynamicHeapSize(min, max)
    };
    builder.options.gc_trigger.set(policy)
}

/// Start the GC Controller thread. We trust the `gc_controller` pointer is valid
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_start_gc_controller_thread(
    tls: VMWorkerThread,
    gc_controller: *mut GCController<Art>,
) {
    let mut gc_controller = unsafe { Box::from_raw(gc_controller) };
    mmtk::memory_manager::start_control_collector(&SINGLETON, tls, &mut gc_controller);
}

/// Start a GC Worker thread. We trust the `worker` pointer is valid
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_start_gc_worker_thread(
    tls: VMWorkerThread,
    worker: *mut GCWorker<Art>
) {
    let mut worker = unsafe { Box::from_raw(worker) };
    mmtk::memory_manager::start_worker::<Art>(&SINGLETON, tls, &mut worker)
}

/// Release a RustBuffer by dropping it
///
/// # Safety
/// Caller needs to make sure the `ptr` is a valid vector pointer.
#[no_mangle]
pub unsafe extern "C" fn mmtk_release_rust_buffer(
    ptr: *mut Address,
    length: usize,
    capacity: usize,
) {
    let vec = Vec::<Address>::from_raw_parts(ptr, length, capacity);
    drop(vec);
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
    offset: usize,
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

/// Set the thread-local cursor and limit for the default allocator for the
/// given mutator thread
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_set_default_thread_local_cursor_limit(
    mutator: *mut Mutator<Art>,
    bump_pointer: BumpPointer
) {
    let selector = mmtk::memory_manager::get_allocator_mapping(
        &SINGLETON,
        AllocationSemantics::Default,
    );
    let default_allocator = unsafe {
        (*mutator)
            .allocator_impl_mut::<mmtk::util::alloc::ImmixAllocator<Art>>(selector)
    };
    // Copy bump pointer values to the allocator in the mutator
    default_allocator.bump_pointer = bump_pointer;
}

/// Get the thread-local cursor and limit for the default allocator for the
/// given mutator thread
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_get_default_thread_local_cursor_limit(
    mutator: *mut Mutator<Art>
) -> BumpPointer {
    let selector = mmtk::memory_manager::get_allocator_mapping(
        &SINGLETON,
        AllocationSemantics::Default,
    );
    let default_allocator = unsafe {
        (*mutator)
            .allocator_impl_mut::<mmtk::util::alloc::ImmixAllocator<Art>>(selector)
    };
    // Return current bump pointer values
    default_allocator.bump_pointer
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

/// Get bytes allocated
#[no_mangle]
pub extern "C" fn mmtk_get_used_bytes() -> usize {
    mmtk::memory_manager::used_bytes(&SINGLETON)
}

/// Check if given object is marked. This is used to implement the heap visitor
#[no_mangle]
pub extern "C" fn mmtk_is_object_marked(object: ObjectReference) -> bool {
    // XXX(kunals): This may not really be an object...
    object.is_reachable()
}

/// Handle user collection request
#[no_mangle]
pub extern "C" fn mmtk_handle_user_collection_request(
    tls: VMMutatorThread,
    force: bool,
    exhaustive: bool
) {
    mmtk::memory_manager::handle_user_collection_request(
        &SINGLETON,
        tls,
        force,
        exhaustive
    )
}
