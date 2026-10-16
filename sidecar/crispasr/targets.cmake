# Injected after CrispASR's project() by the build helper. Upstream uses
# CMAKE_SOURCE_DIR for several private source paths, so keep its project at
# the root rather than pretending it is safe to embed with add_subdirectory.
set(VOICETYPR_SIDECAR_DIR "${CMAKE_CURRENT_LIST_DIR}")

set(CRISPASR_SOURCE_DIR "" CACHE PATH "Pinned CrispASR source checkout")
if(NOT EXISTS "${CRISPASR_SOURCE_DIR}/src/parakeet.cpp")
    message(FATAL_ERROR "Use scripts/build-crispasr-sidecar.mjs to prepare the pinned source")
endif()

set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(CRISPASR_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(CRISPASR_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(CRISPASR_BUILD_SERVER OFF CACHE BOOL "" FORCE)
set(CRISPASR_NO_C2PA_NATIVE ON CACHE BOOL "" FORCE)
set(CRISPASR_OPUS OFF CACHE BOOL "" FORCE)
set(CRISPASR_AMR OFF CACHE BOOL "" FORCE)
set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
add_compile_definitions(GGML_MAX_NAME=128 NOMINMAX _USE_MATH_DEFINES)
if(MSVC)
    add_compile_options(/utf-8)
endif()

# Keep upstream's model/core build settings, but request only these targets.
# Linking crispasr-lib would pull in its unrelated ASR/TTS catalog.
add_executable(crispasr-sidecar
    "${VOICETYPR_SIDECAR_DIR}/src/main.cpp" "${VOICETYPR_SIDECAR_DIR}/src/session.cpp"
    "${CRISPASR_SOURCE_DIR}/examples/cli/crispasr_backend_parakeet.cpp"
    "${VOICETYPR_SIDECAR_DIR}/src/qwen3_adapter.cpp")
target_compile_features(crispasr-sidecar PRIVATE cxx_std_20)
target_include_directories(crispasr-sidecar PRIVATE
    "${CRISPASR_SOURCE_DIR}/examples"
    "${CRISPASR_SOURCE_DIR}/examples/cli"
    "${CRISPASR_SOURCE_DIR}/src"
    "${CRISPASR_SOURCE_DIR}/include")
target_link_libraries(crispasr-sidecar PRIVATE parakeet qwen3_asr)
set_target_properties(crispasr-sidecar PROPERTIES
    RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin"
    RUNTIME_OUTPUT_DIRECTORY_RELEASE "${CMAKE_BINARY_DIR}/bin")

enable_testing()
add_executable(crispasr-audio-test "${VOICETYPR_SIDECAR_DIR}/tests/audio.cpp")
target_compile_features(crispasr-audio-test PRIVATE cxx_std_20)
target_include_directories(crispasr-audio-test PRIVATE "${VOICETYPR_SIDECAR_DIR}/src")
add_test(NAME crispasr-audio-codec COMMAND crispasr-audio-test)

add_executable(crispasr-qwen3-failure-test "${VOICETYPR_SIDECAR_DIR}/tests/qwen3-failure.cpp")
target_compile_features(crispasr-qwen3-failure-test PRIVATE cxx_std_20)
target_include_directories(crispasr-qwen3-failure-test PRIVATE
    "${CRISPASR_SOURCE_DIR}/examples" "${CRISPASR_SOURCE_DIR}/examples/cli"
    "${CRISPASR_SOURCE_DIR}/src" "${CRISPASR_SOURCE_DIR}/include")
target_link_libraries(crispasr-qwen3-failure-test PRIVATE ggml)
add_test(NAME crispasr-qwen3-failure COMMAND crispasr-qwen3-failure-test)
