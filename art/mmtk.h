#ifndef MMTK_ART_MMTK_H
#define MMTK_ART_MMTK_H

#include <stddef.h>

#ifdef __cplusplus
extern "C"
{
#endif
// namespace mmtk {

/**
 * Allocation
 *
 * Functions that interact with the mutator and are responsible for allocation
 */
void *mmtk_alloc(size_t size);

// }
#ifdef __cplusplus
} // extern "C"
#endif

#endif // MMTK_ART_MMTK_H
