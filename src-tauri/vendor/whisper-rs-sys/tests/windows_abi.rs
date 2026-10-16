#![cfg(all(target_os = "windows", feature = "verify-bindings"))]

use std::mem::{align_of, offset_of, size_of};
use whisper_rs_sys::*;

extern "C" {
    fn whisper_test_layout(structure: u32, field: u32) -> usize;
}

#[test]
fn shipped_bindings_call_the_native_library() {
    // Exercise a real native return value as well as the layout fixture. This
    // needs no downloaded speech model and catches a missing/wrong native link.
    unsafe {
        assert_eq!(whisper_lang_id(c"en".as_ptr()), 0);
        let version = whisper_version();
        assert!(!version.is_null());
        assert!(!std::ffi::CStr::from_ptr(version).to_bytes().is_empty());
    }
}

#[test]
fn shipped_bindings_match_native_headers() {
    // The C half is compiled from the pinned native headers. This catches a
    // stale snapshot even if Rust's internally generated layout tests pass.
    let expected = [
        (
            0,
            vec![
                size_of::<whisper_context_params>(),
                align_of::<whisper_context_params>(),
                offset_of!(whisper_context_params, dtw_mem_size),
            ],
        ),
        (
            1,
            vec![
                size_of::<whisper_full_params>(),
                align_of::<whisper_full_params>(),
                offset_of!(whisper_full_params, language),
                offset_of!(whisper_full_params, abort_callback),
                offset_of!(whisper_full_params, vad_params),
            ],
        ),
        (
            2,
            vec![
                size_of::<whisper_token_data>(),
                align_of::<whisper_token_data>(),
                offset_of!(whisper_token_data, t0),
            ],
        ),
        (
            3,
            vec![
                size_of::<whisper_vad_params>(),
                align_of::<whisper_vad_params>(),
                offset_of!(whisper_vad_params, samples_overlap),
            ],
        ),
    ];
    for (structure, fields) in expected {
        for (field, rust_layout) in fields.into_iter().enumerate() {
            // SAFETY: the test helper takes only integer discriminants and
            // returns compile-time layout constants; it owns no pointers.
            let native_layout = unsafe { whisper_test_layout(structure, field as u32) };
            assert_eq!(
                rust_layout, native_layout,
                "structure {structure}, field {field}"
            );
        }
    }
}
