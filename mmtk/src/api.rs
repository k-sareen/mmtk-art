use crate::{
    Art,
    ArtEdge,
    ArtUpcalls,
    BUILDER,
    LOG_BYTES_IN_EDGE,
    SINGLETON,
    UPCALLS,
};
use mmtk::{
    AllocationSemantics,
    Mutator,
    MutatorContext,
    scheduler::{
        GCController,
        GCWorker,
    },
    util::{
        alloc::{
            AllocatorSelector,
            BumpPointer,
        },
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

/// Flush a mutator instance in MMTk
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_flush_mutator(mutator: *mut Mutator<Art>) {
    mmtk::memory_manager::flush_mutator(unsafe { &mut *mutator });
}

/// Destroy a mutator instance in MMTk
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_destroy_mutator(mutator: *mut Mutator<Art>) {
    mmtk::memory_manager::destroy_mutator(unsafe { &mut *mutator });
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
    // We only match the default allocator, so the index is always 0
    match selector {
        AllocatorSelector::BumpPointer(0) => {
            let default_allocator = unsafe {
                (*mutator)
                    .allocator_impl_mut::<mmtk::util::alloc::BumpAllocator<Art>>(selector)
            };
            // Copy bump pointer values to the allocator in the mutator
            default_allocator.bump_pointer = bump_pointer;
        },
        AllocatorSelector::Immix(0) => {
            let default_allocator = unsafe {
                (*mutator)
                    .allocator_impl_mut::<mmtk::util::alloc::ImmixAllocator<Art>>(selector)
            };
            // Copy bump pointer values to the allocator in the mutator
            default_allocator.bump_pointer = bump_pointer;
        },
        // XXX(kunals): MarkCompact is currently unsupported due to the extra
        // header word that would need to be added to the object in the inline
        // fast-path
        // AllocatorSelector::MarkCompact(0) => {
        //     let default_allocator = unsafe {
        //         (*mutator)
        //             .allocator_impl_mut::<mmtk::util::alloc::MarkCompactAllocator<Art>>(selector)
        //     };
        //     // Copy bump pointer values to the allocator in the mutator
        //     default_allocator.bump_allocator.bump_pointer = bump_pointer;
        // },
        _ => {
            panic!("No support for thread-local allocation for selector {:?}", selector);
        }
    };
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
    // We only match the default allocator, so the index is always 0
    match selector {
        AllocatorSelector::BumpPointer(0) => {
            let default_allocator = unsafe {
                (*mutator)
                    .allocator_impl_mut::<mmtk::util::alloc::BumpAllocator<Art>>(selector)
            };
            // Return the current bump pointer values
            default_allocator.bump_pointer
        },
        AllocatorSelector::Immix(0) => {
            let default_allocator = unsafe {
                (*mutator)
                    .allocator_impl_mut::<mmtk::util::alloc::ImmixAllocator<Art>>(selector)
            };
            // Return the current bump pointer values
            default_allocator.bump_pointer
        },
        // XXX(kunals): MarkCompact is currently unsupported due to the extra
        // header word that would need to be added to the object in the inline
        // fast-path
        // AllocatorSelector::MarkCompact(0) => {
        //     let default_allocator = unsafe {
        //         (*mutator)
        //             .allocator_impl_mut::<mmtk::util::alloc::MarkCompactAllocator<Art>>(selector)
        //     };
        //     // Return the current bump pointer values
        //     default_allocator.bump_allocator.bump_pointer
        // },
        _ => {
            panic!("No support for thread-local allocation for selector {:?}", selector);
        }
    }
}

/// Return if an object is allocated by MMTk
#[no_mangle]
pub extern "C" fn mmtk_is_object_in_heap_space(object: ObjectReference) -> bool {
    mmtk::memory_manager::is_in_mmtk_spaces::<Art>(
        object,
    )
}

/// Check if given object is marked. This is used to implement the heap visitor
#[no_mangle]
pub extern "C" fn mmtk_is_object_marked(object: ObjectReference) -> bool {
    // XXX(kunals): This may not really be an object...
    object.is_reachable()
}

/// Check if the given object is movable by MMTk
#[no_mangle]
pub extern "C" fn mmtk_is_object_movable(object: ObjectReference) -> bool {
    object.is_movable()
}

/// Check if the given object has been forwarded by MMTk
#[no_mangle]
pub extern "C" fn mmtk_is_object_forwarded(object: ObjectReference) -> bool {
    object.get_forwarded_object().is_some()
}

/// Get the forwarding address of the given object if it has been forwarded.
/// Returns `nullptr` if the object hasn't been forwarded.
#[no_mangle]
pub extern "C" fn mmtk_get_forwarded_object(object: ObjectReference) -> ObjectReference {
    let forwarded_object = object.get_forwarded_object();
    match forwarded_object {
        Some(obj) => obj,
        None => ObjectReference::NULL,
    }
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

/// Get total bytes available to the runtime
#[no_mangle]
pub extern "C" fn mmtk_get_total_bytes() -> usize {
    mmtk::memory_manager::total_bytes(&SINGLETON)
}

/// Get free bytes
#[no_mangle]
pub extern "C" fn mmtk_get_free_bytes() -> usize {
    mmtk::memory_manager::free_bytes(&SINGLETON)
}

/// Get bytes allocated
#[no_mangle]
pub extern "C" fn mmtk_get_used_bytes() -> usize {
    mmtk::memory_manager::used_bytes(&SINGLETON)
}

/// Set the image space address and size to make MMTk aware of the boot image
#[no_mangle]
#[allow(mutable_transmutes)]
pub extern "C" fn mmtk_set_image_space(boot_image_start_address: Address, boot_image_size: usize) {
    let mmtk: &mmtk::MMTK<Art> = &SINGLETON;
    let mmtk_mut: &mut mmtk::MMTK<Art> = unsafe { std::mem::transmute(mmtk) };
    mmtk::memory_manager::set_vm_space(mmtk_mut, boot_image_start_address, boot_image_size);
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

/// Return if current collection is an emergency collection
#[no_mangle]
pub extern "C" fn mmtk_is_emergency_collection() -> bool {
    SINGLETON.is_emergency_collection()
}

/// Perform a pre-write barrier for a given source, slot, and target. Only call
/// this before the object has been modified
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_object_reference_write_pre(
    mutator: *mut Mutator<Art>,
    src: ObjectReference,
    slot: ArtEdge,
    target: ObjectReference,
) {
    unsafe {
        (*mutator)
            .barrier()
            .object_reference_write_pre(src, slot, target);
    }
}

/// Perform a post-write barrier for a given source, slot, and target. Only call
/// this after the object has been modified
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_object_reference_write_post(
    mutator: *mut Mutator<Art>,
    src: ObjectReference,
    slot: ArtEdge,
    target: ObjectReference,
) {
    unsafe {
        (*mutator)
            .barrier()
            .object_reference_write_post(src, slot, target);
    }
}

/// Perform a pre-array copy barrier for given source, target, and count. Only
/// call this before the array has been copied
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_array_copy_pre(
    mutator: *mut Mutator<Art>,
    src: Address,
    dst: Address,
    count: usize,
) {
    let bytes: usize = count << LOG_BYTES_IN_EDGE;
    unsafe {
        (*mutator)
            .barrier()
            .memory_region_copy_pre((src..src + bytes).into(), (dst..dst + bytes).into());
    }
}

/// Perform a post-array copy barrier for given source, target, and count. Only
/// call this before the array has been copied
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_array_copy_post(
    mutator: *mut Mutator<Art>,
    src: Address,
    dst: Address,
    count: usize,
) {
    let bytes: usize = count << LOG_BYTES_IN_EDGE;
    unsafe {
        (*mutator)
            .barrier()
            .memory_region_copy_post((src..src + bytes).into(), (dst..dst + bytes).into());
    }
}

/// Generic hook to allow benchmarks to be harnessed. We perform a full-heap GC
/// and then enable collecting statistics inside MMTk
#[no_mangle]
pub extern "C" fn mmtk_harness_begin(tls: VMMutatorThread) {
    mmtk::memory_manager::harness_begin(
        &SINGLETON,
        tls,
    )
}

/// Generic hook to allow benchmarks to be harnessed. We stop collecting
/// statistics and print stats values
#[no_mangle]
pub extern "C" fn mmtk_harness_end() {
    mmtk::memory_manager::harness_end(&SINGLETON)
}
