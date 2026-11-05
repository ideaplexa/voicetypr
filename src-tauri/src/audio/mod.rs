pub mod decode;
pub mod device_watcher;
pub mod level_meter;
pub mod normalizer;
pub mod recorder;
pub mod recorder_watchdog;
pub mod resampler;
pub mod silence_detector;
pub mod silero_vad;
pub mod speech_evidence;
pub mod stream_tap;
pub mod vad_cli;
pub mod vad_shadow;

#[cfg(test)]
mod decode_tests;
#[cfg(test)]
mod normalizer_tests;
