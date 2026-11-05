# CPU-only VAD must not enumerate ggml's global registry: registering Metal
# creates its device and compiles its library even when use_gpu=false.
# Only patch whisper-rs-sys's disposable OUT_DIR copy, never Cargo registry files.
if(NOT APPLE OR NOT PROJECT_NAME STREQUAL "whisper.cpp")
    return()
endif()
get_filename_component(out "$ENV{OUT_DIR}" REALPATH)
set(expected "${out}/whisper.cpp")
get_filename_component(actual "${CMAKE_CURRENT_SOURCE_DIR}" REALPATH)
if(NOT DEFINED ENV{OUT_DIR} OR NOT actual STREQUAL expected)
    message(FATAL_ERROR "CPU VAD patch requires the whisper-rs-sys OUT_DIR copy")
endif()
execute_process(COMMAND "${CMAKE_COMMAND}" -E echo
    "cargo:rerun-if-changed=${CMAKE_CURRENT_LIST_FILE}")
execute_process(COMMAND "${CMAKE_COMMAND}" -E echo
    "cargo:rerun-if-env-changed=CMAKE_PROJECT_INCLUDE")
set(source "${actual}/src/whisper.cpp")
get_filename_component(real_source "${source}" REALPATH)
if(NOT real_source STREQUAL source)
    message(FATAL_ERROR "CPU VAD patch rejects source symlink escapes")
endif()
file(SHA256 "${source}" digest)
set(before_sha "d1e041704c12e20b8eb9596632db3b3b31046348dbb0e493f8a5ab8edd5f4376")
set(after_sha "df8c01bd598eed9d14e35def1696ee7e396caa70a5fe0e6322092252fd1bcf38")
if(digest STREQUAL after_sha)
    return()
endif()
if(NOT digest STREQUAL before_sha)
    message(FATAL_ERROR "Unsupported whisper.cpp source for CPU VAD patch")
endif()
file(READ "${source}" text)
set(old_1 [=[static buft_list_t make_buft_list(whisper_context_params & params) {]=])
set(new_1 [=[static buft_list_t make_buft_list(whisper_context_params & params, bool cpu_only = false) {]=])
set(old_2 [=[    auto * cpu_dev = ggml_backend_dev_by_type(GGML_BACKEND_DEVICE_TYPE_CPU);]=])
set(new_2 [=[    auto * cpu_dev = cpu_only
        ? ggml_backend_reg_dev_get(ggml_backend_cpu_reg(), 0)
        : ggml_backend_dev_by_type(GGML_BACKEND_DEVICE_TYPE_CPU);]=])
set(old_3 [=[    whisper_context_params wparams = whisper_context_default_params();
    wparams.use_gpu = params.use_gpu;
    wparams.gpu_device = params.gpu_device;
    buft_list_t buft_list = make_buft_list(wparams);
]=])
set(new_3 [=[    whisper_context_params wparams = whisper_context_default_params();
    wparams.use_gpu = false;
    wparams.gpu_device = params.gpu_device;
    buft_list_t buft_list = make_buft_list(wparams, true);
]=])
set(old_4 [=[    auto whisper_context_params = whisper_context_default_params();
    // TODO: GPU VAD is forced disabled until the performance is improved
    //whisper_context_params.use_gpu    = vctx->params.use_gpu;
    whisper_context_params.use_gpu    = false;
    whisper_context_params.gpu_device = vctx->params.gpu_device;

    vctx->backends = whisper_backend_init(whisper_context_params);
    if (vctx->backends.empty()) {
        WHISPER_LOG_ERROR("%s: whisper_backend_init() failed\n", __func__);
        return false;
    }
]=])
set(new_4 [=[    auto * cpu = ggml_backend_cpu_init();
    if (!cpu) {
        WHISPER_LOG_ERROR("%s: CPU backend initialization failed\n", __func__);
        return false;
    }
    vctx->backends = { cpu };
]=])
foreach(i RANGE 1 4)
    string(FIND "${text}" "${old_${i}}" found)
    if(found LESS 0)
        message(FATAL_ERROR "CPU VAD patch preimage mismatch")
    endif()
    string(REPLACE "${old_${i}}" "${new_${i}}" text "${text}")
endforeach()
# Link-time enforcement: an old cached native archive must not silently ship.
string(APPEND text "\nextern \"C\" int voicetypr_vad_cpu_only_v1(void) { return 1; }\n")
string(SHA256 patched_digest "${text}")
if(NOT patched_digest STREQUAL after_sha)
    message(FATAL_ERROR "CPU VAD patch postimage mismatch")
endif()
file(WRITE "${source}" "${text}")
