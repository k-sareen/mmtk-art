#ifndef MMTK_ART_MMTK_H
#define MMTK_ART_MMTK_H

#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

extern "C" {
// namespace mmtk {

// An arbitrary address
typedef void* Address;
// MmtkMutator should be an opaque pointer for the VM
typedef void* MmtkMutator;
// An opaque pointer to a VMThread
typedef void* VMThread;
// Type of GC worker
enum GcThreadKind { MmtkGcWorker };
// Allocation semantics
enum AllocationSemantics {
  AllocatorDefault   = 0,
  AllocatorImmortal  = 1,
  AllocatorLos       = 2,
  AllocatorCode      = 3,
  AllocatorReadOnly  = 4,
  AllocatorLargeCode = 5,
  AllocatorNonMoving = 6,
};
// Reference processing phase
enum RefProcessingPhase {
  Phase1 = 0,
  Phase2 = 1,
  Phase3 = 2,
};
// Plan selector
enum MmtkPlanSelector {
  NoGC        = 0,
  SemiSpace   = 1,
  GenCopy     = 2,
  GenImmix    = 3,
  MarkSweep   = 4,
  PageProtect = 5,
  Immix       = 6,
  MarkCompact = 7,
  StickyImmix = 8,
};
// Allocation error
enum MmtkAllocationError {
  HeapOOM = 0,
  MmapOOM = 1,
};

// A representation of an MMTk bump pointer for embedding in the mutator's TLS
typedef struct {
  Address cursor;
  Address limit;
} MmtkBumpPointer;

// A representation of a Rust buffer
typedef struct {
  Address* buf;
  size_t len;
  size_t capacity;
} RustBuffer;

// A representation of a Rust buffer of ObjectReference
typedef struct {
  void** buf;
  size_t len;
  size_t capacity;
} RustObjectReferenceBuffer;

// A representation of an allocated region in MMTk
typedef struct {
  Address start;
  size_t size;
} AllocatedRegion;

// A representation of a Rust buffer of (Address, size)
typedef struct {
  AllocatedRegion* buf;
  size_t len;
  size_t capacity;
} RustAllocatedRegionBuffer;

// A closure that operates on MmtkMutators
struct MutatorClosure {
  void (*func)(MmtkMutator mutator, void* data);
  void* data;

  void invoke(MmtkMutator mutator) {
    func(mutator, data);
  }
};

// A closure used for reporting root nodes back to MMTk
struct NodesClosure {
  RustBuffer (*func)(Address* buf, size_t size, size_t capacity, void* data);
  void* data;

  RustBuffer invoke(Address* buf, size_t size, size_t capacity) {
    return func(buf, size, capacity, data);
  }
};

// A closure used for reporting root slots back to MMTk
struct SlotsClosure {
  RustBuffer (*func)(Address* buf, size_t size, size_t capacity, void* data);
  void* data;

  RustBuffer invoke(Address* buf, size_t size, size_t capacity) {
    return func(buf, size, capacity, data);
  }
};

// A closure used to scan an object for references
struct ScanObjectClosure {
  void (*func)(void* edge, void* data);
  void* data;

  void invoke(void* edge) const {
    func(edge, data);
  }
};

// A closure used to trace weak references during reference processing
struct TraceObjectClosure {
  void* (*func)(void* object, void* data);
  void* data;

  void* invoke(void* object) {
    return func(object, data);
  }
};

// Upcalls from MMTk to ART
typedef struct {
  size_t (*size_of) (void* object);
  void (*scan_object) (void* object, ScanObjectClosure closure);
  bool (*is_valid_object) (void* object);
  void (*block_for_gc) (void* tls);
  void (*spawn_gc_thread) (void* tls, GcThreadKind kind, void* ctx);
  void (*suspend_mutators) (void* tls);
  void (*resume_mutators) (void* tls);
  size_t (*number_of_mutators) ();
  bool (*is_mutator) (void* tls);
  MmtkMutator (*get_mmtk_mutator) (void* tls);
  void (*for_all_mutators) (MutatorClosure closure);
  void (*scan_all_roots) (SlotsClosure closure);
  void (*process_references) (
    void* tls,
    TraceObjectClosure closure,
    RefProcessingPhase phase,
    bool clear_soft_references
  );
  void (*sweep_system_weaks) ();
  void (*set_has_zygote_space_in_art) (bool has_zygote_space);
  void (*throw_out_of_memory) (void* tls, MmtkAllocationError err_kind);
} ArtUpcalls;

/**
 * Initialize MMTk instance
 *
 * @param upcalls the set of ART upcalls used by MMTk
 * @param plan what MMTk plan to use
 * @param is_zygote_process is the current runtime the Zygote process?
 */
void mmtk_init(ArtUpcalls* upcalls, MmtkPlanSelector plan, bool is_zygote_process);

/**
 * Initialize collection for MMTk
 *
 * @param tls reference to the calling VMThread
 */
void mmtk_initialize_collection(VMThread tls);

/**
 * Set the heap size
 *
 * @param min minimum heap size
 * @param max maximum heap size
 */
void mmtk_set_heap_size(size_t min, size_t max);

/**
 * Clamp the max heap size for target application
 *
 * @param max maximum heap size (in bytes)
 * @return if the max heap size was clamped
 */
bool mmtk_clamp_max_heap_size(size_t max);

/**
 * Get the heap start
 *
 * @return the starting heap address
 */
Address mmtk_get_heap_start();

/**
 * Get the heap end
 *
 * @return the ending heap address
 */
Address mmtk_get_heap_end();

/**
 * Get total bytes available to the runtime
 *
 * @return the total bytes available
 */
size_t mmtk_get_total_bytes();

/**
 * Get free bytes
 *
 * @return the free bytes
 */
size_t mmtk_get_free_bytes();

/**
 * Get total bytes allocated
 *
 * @return the total bytes allocated
 */
size_t mmtk_get_used_bytes();

/**
 * Get number of GC worker threads
 *
 * @return the number of GC worker threads
 */
uint32_t mmtk_get_number_of_workers();

/**
 * Iterate through all the allocated regions in MMTk
 *
 * @return a Rust `Vec` with allocated regions returned as (start_address, size)
 */
RustAllocatedRegionBuffer mmtk_iterate_allocated_regions();

/**
 * Enumerate all the large objects allocated in MMTk
 *
 * @return a Rust `Vec` with large objects allocated in MMTk
 */
RustObjectReferenceBuffer mmtk_enumerate_large_objects();

/**
 * Set the image space address and size to make MMTk aware of the boot image
 *
 * @param boot_image_start_address the starting address of the boot image
 * @param boot_image_size the size of the boot image space
 */
void mmtk_set_image_space(uint32_t boot_image_start_address, uint32_t boot_image_size);

/**
 * Return if the object has been marked by a GC or not (i.e. return if the
 * object is live)
 *
 * @param object the object to be queried
 * @return if the object is marked
 */
bool mmtk_is_object_marked(void* object);

/**
 * Check if an object has been allocated by MMTk
 *
 * @param object address of the object
 * @return if the given object has been allocated by MMTk
 */
bool mmtk_is_object_in_heap_space(const void* object);

/**
 * Check if an object is movable
 *
 * @param object address of the object
 * @return if the given object can be moved by MMTk
 */
bool mmtk_is_object_movable(void* object);

/**
 * Check if an object has been forwarded
 *
 * @param object address of the object
 * @return if the given object has been forwarded
 */
bool mmtk_is_object_forwarded(void* object);

/**
 * Get the forwarding address of the object
 *
 * @param object address of the object
 * @return forwarding address of the object
 */
void* mmtk_get_forwarded_object(void* object);

/**
 * Pin the object so it does not move during GCs
 *
 * @param object address of the object
 * @return if the object was pinned or not
 */
bool mmtk_pin_object(void* object);

/**
 * Check if an object has been pinned or not
 *
 * @param object address of the object
 * @return if the object is pinned or not
 */
bool mmtk_is_object_pinned(void* object);

/**
 * Start a GC Worker thread
 *
 * @param tls the thread that will be used as the GC Worker
 * @param context the context for the GC Worker
 */
void mmtk_start_gc_worker_thread(void* tls, void* context);

/**
 * Release a RustBuffer by dropping it. It is the caller's responsibility to
 * ensure that @param buffer points to a valid RustBuffer.
 *
 * @param buffer the address of the buffer
 * @param length the number of items in the buffer
 * @param capacity the maximum capacity of the buffer
 */
void mmtk_release_rust_buffer(void** buffer, size_t length, size_t capacity);

/**
 * Release a RustAllocatedRegionBuffer by dropping it. It is the caller's
 * responsibility to ensure that @param buffer points to a valid
 * RustAllocatedRegionBuffer.
 *
 * @param buffer the address of the buffer
 * @param length the number of items in the buffer
 * @param capacity the maximum capacity of the buffer
 */
void mmtk_release_rust_allocated_region_buffer(AllocatedRegion* buffer, size_t length, size_t capacity);

/**
 * Release a RustObjectReferenceBuffer by dropping it. It is the caller's
 * responsibility to ensure that @param buffer points to a valid
 * RustObjectReferenceBuffer.
 *
 * @param buffer the address of the buffer
 * @param length the number of items in the buffer
 * @param capacity the maximum capacity of the buffer
 */
void mmtk_release_rust_object_reference_buffer(void** buffer, size_t length, size_t capacity);

/**
 * Allocation
 *
 * Functions that interact with the mutator and are responsible for allocation
 */

/**
 * Bind a mutator thread in MMTk
 *
 * @param tls pointer to mutator thread
 * @return an instance of an MMTk mutator
 */
MmtkMutator mmtk_bind_mutator(void* tls);

/**
 * Flush a mutator instance in MMTk.
 *
 * @param mutator pointer to MMTk mutator instance
 */
void mmtk_flush_mutator(MmtkMutator mutator);

/**
 * Destroy a mutator instance in MMTk.
 *
 * @param mutator pointer to MMTk mutator instance
 */
void mmtk_destroy_mutator(MmtkMutator mutator);

/**
 * Allocate an object
 *
 * @param mutator the mutator instance that is requesting the allocation
 * @param size the size of the requested object
 * @param align the alignment requirement for the object
 * @param offset the allocation offset for the object
 * @param allocator the allocation sematics to use for the allocation
 * @return the address of the newly allocated object
 */
void* mmtk_alloc(MmtkMutator mutator, size_t size, size_t align,
        size_t offset, AllocationSemantics semantics);

/**
 * Set relevant object metadata
 *
 * @param mutator the mutator instance that is requesting the allocation
 * @param object the returned address of the allocated object
 * @param size the size of the allocated object
 * @param allocator the allocation sematics to use for the allocation
 */
void mmtk_post_alloc(MmtkMutator mutator, void* object,
        size_t size, AllocationSemantics semantics);

/**
 * Set the thread-local cursor limit for the default allocator for the given
 * mutator thread `mutator` to the specified values in `bump_pointer`
 *
 * @param mutator the mutator instance
 * @param bump_pointer the bump pointer values to set for the mutator
 */
void mmtk_set_default_thread_local_cursor_limit(MmtkMutator mutator, MmtkBumpPointer bump_pointer);

/**
 * Get the thread-local cursor limit for the default allocator for the given
 * mutator thread `mutator`
 *
 * @param mutator the mutator instance
 * @return the bump pointer values for the default allocator for the mutator
 */
MmtkBumpPointer mmtk_get_default_thread_local_cursor_limit(MmtkMutator mutator);

/**
 * Request a GC to occur. A GC may not actually occur, however, if `force` is
 * set to `true`, then a GC is guaranteed to have been requested.
 *
 * @param tls pointer to the mutator thread requesting GC
 * @param force force a GC to occur
 * @param exhaustive force a full heap GC to occur
 */
bool mmtk_handle_user_collection_request(void* tls, bool force, bool exhaustive);

/**
 * Request full-heap GC to occur before forking the Zygote for the first time.
 * This GC will try to compact the Zygote space as much as possible.
 *
 * @param tls pointer to the mutator thread requesting GC
 */
void mmtk_handle_pre_first_zygote_fork_collection_request(void* tls);

/**
 * Return whether the current collection is an emergency collection. This is
 * used for clearing soft references, we only need to clean soft references if
 * we are under heap stress.
 *
 * @return is current collection an emergency collection or not
 */
bool mmtk_is_emergency_collection();

/**
 * Return whether the current collection is a nursery collection. This is used
 * to treat java.lang.ref.Reference objects as strong roots during nursery GCs.
 *
 * @return is current collection a nursery collection or not
 */
bool mmtk_is_nursery_collection();

/**
 * Perform a pre-write barrier for a given source, slot, and target. Only call
 * this before the object has been modified
 *
 * @param mutator mutator executing the barrier code
 * @param src source object being modified
 * @param slot address of the field being modified
 * @param target target object to be written to the field
 */
void mmtk_object_reference_write_pre(MmtkMutator mutator, void* src,
        void* slot, void* target);

/**
 * Perform a post-write barrier for a given source, slot, and target. Only call
 * this after the object has been modified
 *
 * @param mutator mutator executing the barrier code
 * @param src source object being modified
 * @param slot address of the field being modified
 * @param target target object to be written to the field
 */
void mmtk_object_reference_write_post(MmtkMutator mutator, void* src,
        void* slot, void* target);

/**
 * Perform a pre-array copy barrier for given source, target, and count. Only
 * call this before the array has been copied
 *
 * @param mutator mutator executing the barrier code
 * @param src source array slice
 * @param dst destination array slice
 * @param count total count of elements to be written
 */
void mmtk_array_copy_pre(MmtkMutator mutator, void* src,
        void* dst, size_t count);

/**
 * Perform a post-array copy barrier for given source, target, and count. Only
 * call this after the array has been copied
 *
 * @param mutator mutator executing the barrier code
 * @param src source array slice
 * @param dst destination array slice
 * @param count total count of elements to be written
 */
void mmtk_array_copy_post(MmtkMutator mutator, void* src,
        void* dst, size_t count);

/**
 * Inform MMTk if the current runtime is the Zygote process or not
 *
 * @param is_zygote_process is the current runtime the Zygote process?
 */
void mmtk_set_is_zygote_process(bool is_zygote_process);

/**
 * Hook called before the Zygote is forked. We stop GC worker threads and save
 * their context here
 */
void mmtk_pre_zygote_fork();

/**
 * Hook called after the Zygote has been forked. We respawn GC worker threads
 * here
 *
 * @param tls reference to the calling VMThread
 */
void mmtk_post_zygote_fork(VMThread tls);

/**
 * Tell MMTk to create perf counters for this process
 */
void mmtk_create_perf_counters();

/**
 * Generic hook to allow benchmarks to be harnessed. We perform a full-heap GC
 * and then enable collecting statistics inside MMTk.
 *
 * @param tls pointer to the mutator thread calling the function
 */
void mmtk_harness_begin(void* tls);

/**
 * Generic hook to allow benchmarks to be harnessed. We stop collecting
 * statistics and print stats values.
 */
void mmtk_harness_end();

// }
} // extern "C"

#endif // MMTK_ART_MMTK_H
