#ifndef MMTK_ART_MMTK_H
#define MMTK_ART_MMTK_H

#include <stddef.h>
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
enum GcThreadKind { MmtkGcController, MmtkGcWorker };

// A representation of a Rust buffer
typedef struct {
  Address* buf;
  size_t capacity;
} RustBuffer;

// A closure that operates on MmtkMutators
struct MutatorClosure {
  void (*func)(MmtkMutator mutator, void* data);
  void* data;

  void invoke(MmtkMutator mutator) {
    func(mutator, data);
  }
};

// A closure that operates on Edges. Used for reporting Edges back to MMTk
struct EdgesClosure {
  RustBuffer (*func)(Address* buf, size_t size, size_t capacity, void* data);
  void* data;

  RustBuffer invoke(Address* buf, size_t size, size_t capacity) {
    return func(buf, size, capacity, data);
  }
};

// Upcalls from MMTk to ART
typedef struct {
  size_t (*size_of) (void* object);
  void (*block_for_gc) (void* tls);
  void (*spawn_gc_thread) (void* tls, GcThreadKind kind, void* ctx);
  void (*stop_all_mutators) ();
  void (*resume_mutators) ();
  size_t (*number_of_mutators) ();
  bool (*is_mutator) (void* tls);
  MmtkMutator (*get_mmtk_mutator) (void* tls);
  void (*for_all_mutators) (MutatorClosure closure);
  void (*scan_all_roots) (EdgesClosure closure);
} ArtUpcalls;

/**
 * Initialize MMTk instance
 *
 * @param upcalls the set of ART upcalls used by MMTk
 */
void mmtk_init(ArtUpcalls* upcalls);

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
 * Return if the valid object bit is set or not
 *
 * @param addr the address to be queried
 * @return if the valid object bit is set
 */
bool mmtk_is_valid_object(Address addr);

/**
 * Start the GC Controller thread
 *
 * @param tls the thread that will be used as the GC Controller
 * @param context the context for the GC Controller
 */
void mmtk_start_gc_controller_thread(void* tls, void* context);

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
 * Allocate an object
 *
 * @param mutator the mutator instance that is requesting the allocation
 * @param size the size of the requested object
 * @param align the alignment requirement for the object
 * @param offset the allocation offset for the object
 * @param allocator the allocation sematics to use for the allocation
 * @return the address of the newly allocated object
 */
void *mmtk_alloc(MmtkMutator mutator, size_t size, size_t align,
        size_t offset, int allocator);

/**
 * Set relevant object metadata
 *
 * @param mutator the mutator instance that is requesting the allocation
 * @param object the returned address of the allocated object
 * @param size the size of the allocated object
 * @param allocator the allocation sematics to use for the allocation
 */
void mmtk_post_alloc(MmtkMutator mutator, void* object, size_t size, int allocator);

/**
 * Check if an object has been allocated by MMTk
 *
 * @param object address of the object
 * @return if the given object has been allocated by MMTk
 */
bool mmtk_is_object_in_heap_space(const void* object);

// }
} // extern "C"

#endif // MMTK_ART_MMTK_H
