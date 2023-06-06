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

// Upcalls from MMTk to ART
typedef struct {
  size_t (*size_of) (void *object);
} ArtUpcalls;

/**
 * Initialize MMTk instance
 *
 * @param upcalls the set of ART upcalls used by MMTk
 */
void mmtk_init(ArtUpcalls *upcalls);

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
MmtkMutator mmtk_bind_mutator(void *tls);

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
        ssize_t offset, int allocator);

/**
 * Set relevant object metadata
 *
 * @param mutator the mutator instance that is requesting the allocation
 * @param object the returned address of the allocated object
 * @param size the size of the allocated object
 * @param allocator the allocation sematics to use for the allocation
 */
void mmtk_post_alloc(MmtkMutator mutator, void *object, size_t size, int allocator);

/**
 * Check if an object has been allocated by MMTk
 *
 * @param object address of the object
 * @return if the given object has been allocated by MMTk
 */
bool mmtk_is_object_in_heap_space(const void *object);

// }
} // extern "C"

#endif // MMTK_ART_MMTK_H