#ifndef MMTK_ART_MMTK_H
#define MMTK_ART_MMTK_H

#include <stddef.h>
#include <sys/types.h>

#ifdef __cplusplus
extern "C"
{
#endif
// namespace mmtk {

/**
 * MmtkMutator should be an opaque pointer for the VM
 */
typedef void* MmtkMutator;

/**
 * Initialize MMTk instance
 */
void mmtk_init();

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

// }
#ifdef __cplusplus
} // extern "C"
#endif

#endif // MMTK_ART_MMTK_H
