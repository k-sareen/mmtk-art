#[cfg(target_os = "android")]
extern crate android_logger;

use crate::{
    Art,
    ArtEdge,
    ArtUpcalls,
    BUILDER,
    IS_MMTK_INITIALIZED,
    LOG_BYTES_IN_EDGE,
    SINGLETON,
    UPCALLS,
};
#[cfg(target_os = "android")]
use android_logger::Config;
use mmtk::{
    AllocationSemantics,
    Mutator,
    MutatorContext,
    scheduler::GCWorker,
    util::{
        alloc::{
            AllocatorSelector,
            BumpPointer,
        },
        opaque_pointer::*,
        options::PlanSelector,
        Address,
        ObjectReference,
    },
};
use std::sync::atomic::Ordering;

/// Initialize MMTk instance
#[no_mangle]
pub extern "C" fn mmtk_init(
    upcalls: *const ArtUpcalls,
    plan: PlanSelector,
    is_zygote_process: bool,
) {
    // SAFETY: Assumes upcalls is valid
    unsafe { UPCALLS = upcalls };
    // Set the plan
    {
        let mut builder = BUILDER.lock().unwrap();
        builder.options.plan.set(plan);
    }
    // Set the is_zygote_process option
    {
        let mut builder = BUILDER.lock().unwrap();
        builder.options.is_zygote_process.set(is_zygote_process);
        debug_assert!(
            (*builder.options.is_zygote_process) == is_zygote_process,
            "Was unable to set the is_zygote_process option to {}",
            is_zygote_process,
        );
    }
    // Make sure that we haven't initialized MMTk (by accident) yet
    assert!(!crate::IS_MMTK_INITIALIZED.load(Ordering::SeqCst));

    // Attempt to init a logger for MMTk
    #[cfg(target_os = "android")]
    {
        #[cfg(target_pointer_width = "32")]
        let tag = "mmtk-art32";
        #[cfg(target_pointer_width = "64")]
        let tag = "mmtk-art64";
        android_logger::init_once(
            Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag(tag),
        );

        info!(
            "Initializing MMTk with{} Zygote space",
            if !is_zygote_process {
                "out"
            } else {
                ""
            },
        );
    }

    // Make sure we initialize MMTk here
    lazy_static::initialize(&SINGLETON)
}

/// Initialize collection for MMTk
#[no_mangle]
pub extern "C" fn mmtk_initialize_collection(tls: VMThread) {
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    mmtk::memory_manager::initialize_collection(&SINGLETON, tls);
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

/// Start a GC Worker thread. We trust the `worker` pointer is valid
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_start_gc_worker_thread(
    tls: VMWorkerThread,
    worker: *mut GCWorker<Art>
) {
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    // SAFETY: Assumes worker is valid
    let worker = unsafe { Box::from_raw(worker) };
    mmtk::memory_manager::start_worker::<Art>(&SINGLETON, tls, worker)
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
    // SAFETY: Assumes ptr is valid
    unsafe {
        let vec = Vec::<Address>::from_raw_parts(ptr, length, capacity);
        drop(vec);
    }
}

/// Bind a mutator thread in MMTk
#[no_mangle]
pub extern "C" fn mmtk_bind_mutator(tls: VMMutatorThread) -> *mut Mutator<Art> {
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    Box::into_raw(mmtk::memory_manager::bind_mutator(&SINGLETON, tls))
}

/// Flush a mutator instance in MMTk
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_flush_mutator(mutator: *mut Mutator<Art>) {
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    // SAFETY: Assumes mutator is valid
    mmtk::memory_manager::flush_mutator(unsafe { &mut *mutator });
}

/// Destroy a mutator instance in MMTk
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn mmtk_destroy_mutator(mutator: *mut Mutator<Art>) {
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    // SAFETY: Assumes mutator is valid
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
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    mmtk::memory_manager::alloc::<Art>(
        // SAFETY: Assumes mutator is valid
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
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    mmtk::memory_manager::post_alloc::<Art>(
        // SAFETY: Assumes mutator is valid
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
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    let selector = mmtk::memory_manager::get_allocator_mapping(
        &SINGLETON,
        AllocationSemantics::Default,
    );
    // We only match the default allocator, so the index is always 0
    match selector {
        AllocatorSelector::BumpPointer(0) => {
            // SAFETY: Assumes mutator is valid
            let default_allocator = unsafe {
                (*mutator)
                    .allocator_impl_mut::<mmtk::util::alloc::BumpAllocator<Art>>(selector)
            };
            // Copy bump pointer values to the allocator in the mutator
            default_allocator.bump_pointer = bump_pointer;
        },
        AllocatorSelector::Immix(0) => {
            // SAFETY: Assumes mutator is valid
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
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    let selector = mmtk::memory_manager::get_allocator_mapping(
        &SINGLETON,
        AllocationSemantics::Default,
    );
    // We only match the default allocator, so the index is always 0
    match selector {
        AllocatorSelector::BumpPointer(0) => {
            // SAFETY: Assumes mutator is valid
            let default_allocator = unsafe {
                (*mutator)
                    .allocator_impl_mut::<mmtk::util::alloc::BumpAllocator<Art>>(selector)
            };
            // Return the current bump pointer values
            default_allocator.bump_pointer
        },
        AllocatorSelector::Immix(0) => {
            // SAFETY: Assumes mutator is valid
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

/// Pin the object so it does not move during GCs
#[no_mangle]
pub extern "C" fn mmtk_pin_object(object: ObjectReference) -> bool {
    mmtk::memory_manager::pin_object::<Art>(object)
}

/// Check if an object has been pinned or not
#[no_mangle]
pub extern "C" fn mmtk_is_object_pinned(object: ObjectReference) -> bool {
    mmtk::memory_manager::is_pinned::<Art>(object)
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

/// Get number of GC worker threads
#[no_mangle]
pub extern "C" fn mmtk_get_number_of_workers() -> u32 {
    mmtk::memory_manager::num_of_workers(&SINGLETON) as u32
}

/// Set the image space address and size to make MMTk aware of the boot image
#[no_mangle]
#[allow(mutable_transmutes)]
pub extern "C" fn mmtk_set_image_space(boot_image_start_address: Address, boot_image_size: usize) {
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    let mmtk: &mmtk::MMTK<Art> = &SINGLETON;
    // SAFETY: Assumes mmtk is valid
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
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    mmtk::memory_manager::handle_user_collection_request(
        &SINGLETON,
        tls,
        force,
        exhaustive
    )
}

/// Request full-heap GC to occur before forking the Zygote for the first time.
/// This GC should try to compact the Zygote space as much as possible.
#[no_mangle]
pub extern "C" fn mmtk_handle_pre_first_zygote_fork_collection_request(tls: VMMutatorThread) {
    debug_assert!(IS_MMTK_INITIALIZED.load(Ordering::SeqCst));
    mmtk::memory_manager::handle_pre_first_zygote_fork_collection_request(
        &SINGLETON,
        tls,
    );

    // SAFETY: Assumes upcalls is valid
    unsafe {
        ((*UPCALLS).set_has_zygote_space_in_art)(
            SINGLETON.has_zygote_space(),
        )
    }
}

/// Return if current collection is an emergency collection
#[no_mangle]
pub extern "C" fn mmtk_is_emergency_collection() -> bool {
    SINGLETON.is_emergency_collection()
}

/// Return if current collection is a nursery collection
#[no_mangle]
pub extern "C" fn mmtk_is_nursery_collection() -> bool {
    SINGLETON.is_nursery_collection()
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
    // SAFETY: Assumes mutator is valid
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
    // SAFETY: Assumes mutator is valid
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
    // SAFETY: Assumes mutator is valid
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
    // SAFETY: Assumes mutator is valid
    unsafe {
        (*mutator)
            .barrier()
            .memory_region_copy_post((src..src + bytes).into(), (dst..dst + bytes).into());
    }
}

/// Inform MMTk if the current runtime is the Zygote process or not
#[no_mangle]
pub extern "C" fn mmtk_set_is_zygote_process(is_zygote_process: bool) {
    SINGLETON.set_is_zygote_process(is_zygote_process);
}

/// Hook called before the Zygote is forked. We stop GC worker threads and save
/// their context here
#[no_mangle]
pub extern "C" fn mmtk_pre_zygote_fork() {
    SINGLETON.prepare_to_fork();
}

/// Hook called after the Zygote has been forked. We respawn GC worker threads
/// here
#[no_mangle]
pub extern "C" fn mmtk_post_zygote_fork(tls: VMThread) {
    SINGLETON.after_fork(tls);
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
