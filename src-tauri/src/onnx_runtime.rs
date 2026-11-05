//! Conservative beta.8 Parakeet CPU policy, checked before lazy DLL loading.
//! The official CPU DLL is no longer statically linked, so unsupported CPUs
//! can reach this guard. Keep the existing Haswell policy for Parakeet; ggml
//! shadow VAD has its own four-feature gate.
#[cfg_attr(
    not(any(test, all(target_os = "windows", target_arch = "x86_64"))),
    allow(dead_code)
)]
pub const CPU_UNSUPPORTED: &str =
    "This CPU cannot run ONNX models. AVX2, FMA and associated CPU features are required.";

#[cfg_attr(
    not(any(test, all(target_os = "windows", target_arch = "x86_64"))),
    allow(dead_code)
)]
pub fn require_cpu() -> Result<(), &'static str> {
    #[cfg(test)]
    if FORCE_UNSUPPORTED.get() {
        return Err(CPU_UNSUPPORTED);
    }
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    let supported = [
        std::is_x86_feature_detected!("avx"),
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("fma"),
        std::is_x86_feature_detected!("f16c"),
        std::is_x86_feature_detected!("bmi1"),
        std::is_x86_feature_detected!("bmi2"),
        std::is_x86_feature_detected!("lzcnt"),
        std::is_x86_feature_detected!("popcnt"),
        std::is_x86_feature_detected!("sse4.2"),
        std::is_x86_feature_detected!("movbe"),
    ]
    .into_iter()
    .all(|present| present);
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    let supported = true;
    if supported {
        Ok(())
    } else {
        Err(CPU_UNSUPPORTED)
    }
}

#[cfg(test)]
thread_local! {
    static FORCE_UNSUPPORTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub fn without_cpu<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            FORCE_UNSUPPORTED.set(self.0);
        }
    }
    let _reset = Reset(FORCE_UNSUPPORTED.replace(true));
    f()
}

#[cfg(test)]
mod tests {
    #[test]
    fn forced_off_never_enters_the_native_initializer() {
        super::without_cpu(|| {
            let result = super::require_cpu().map(|()| panic!("entered native code"));
            assert_eq!(result, Err(super::CPU_UNSUPPORTED));
        });
    }
}
