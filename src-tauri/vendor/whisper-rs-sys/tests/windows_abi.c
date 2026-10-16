#include <stddef.h>
#include <stdint.h>
#include "whisper.h"

// Independent of bindgen: these constants come from the actual C compiler
// and headers used to build the linked native library.
size_t whisper_test_layout(uint32_t structure, uint32_t field) {
    switch (structure) {
    case 0: {
        const size_t values[] = {sizeof(struct whisper_context_params),
            _Alignof(struct whisper_context_params), offsetof(struct whisper_context_params, dtw_mem_size)};
        return field < 3 ? values[field] : SIZE_MAX;
    }
    case 1: {
        const size_t values[] = {sizeof(struct whisper_full_params),
            _Alignof(struct whisper_full_params), offsetof(struct whisper_full_params, language),
            offsetof(struct whisper_full_params, abort_callback), offsetof(struct whisper_full_params, vad_params)};
        return field < 5 ? values[field] : SIZE_MAX;
    }
    case 2: {
        const size_t values[] = {sizeof(struct whisper_token_data),
            _Alignof(struct whisper_token_data), offsetof(struct whisper_token_data, t0)};
        return field < 3 ? values[field] : SIZE_MAX;
    }
    case 3: {
        const size_t values[] = {sizeof(struct whisper_vad_params),
            _Alignof(struct whisper_vad_params), offsetof(struct whisper_vad_params, samples_overlap)};
        return field < 3 ? values[field] : SIZE_MAX;
    }
    default: return SIZE_MAX;
    }
}
