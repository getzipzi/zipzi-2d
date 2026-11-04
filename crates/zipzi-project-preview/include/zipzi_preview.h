#pragma once
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
#define ZIPZI_PREVIEW_CAPACITY (8u * 1024u * 1024u)
// Return bytes read, or -1; no callback may throw across the C boundary.
typedef intptr_t (*zipzi_preview_read_at)(void *, uint64_t, uint8_t *, size_t);
int64_t zipzi_preview_read(void *, zipzi_preview_read_at, uint64_t, uint8_t *, size_t);
#ifdef __cplusplus
}
#endif
