//! Structured logging utilities for Voicetypr debugging.
//!
//! This module provides consistent logging patterns across the application
//! to help diagnose production issues, performance bottlenecks, and silent failures.
//!
//! ## Performance Strategy
//!
//! This module uses a **selective performance optimization** approach:
//!
//! ### Production Logging (What Users See)
//! - **Major operations**: ALWAYS logged (recording, transcription, errors)
//! - **Hot paths**: Sampled at 1% or disabled
//! - **Debug details**: Only when explicitly enabled via log level
//!
//! ### Performance Optimizations
//! 1. **Log level checking**: Skip expensive ops if logs filtered out
//! 2. **Sampling**: Hot paths use `log_context_sampled!` (1% sample rate)
//! 3. **Lazy evaluation**: Only format strings if actually logging
//!
//! ### Usage Guidelines
//!
//! For different code paths:
//! - **User operations** (record/transcribe): Use normal logging
//! - **Hot paths** (audio processing): Use `log_simple!` or sampling
//! - **Error paths**: ALWAYS log with full context
//! - **Performance tests**: Use specialized test macros

use std::collections::HashMap;

// NOTE: LogEvent enums and complex structures have been removed
// We now use simple logging functions (log_start, log_complete, log_failed, log_with_context)
// for better maintainability and reduced complexity

// REMOVED: Function wrapper logging - use explicit log_start/log_complete for clarity
// This avoids hidden overhead and makes logging intentions explicit at call sites

// REMOVED: Use log_failed() with log_with_context() for error logging

/// Log performance metrics for operations
pub fn log_performance(operation: &str, duration_ms: u64, metadata: Option<&str>) {
    let metadata_str = metadata.unwrap_or("");
    log::info!(
        "⚡ PERF: {} took {}ms {}",
        operation,
        duration_ms,
        metadata_str
    );
}

// Removed: log_performance_detailed - simplified to basic log_performance

// REMOVED: HashMap-based wrappers - use log_start() with log_with_context() directly

/// Log model operations with metadata
pub fn log_model_operation(
    operation: &str,
    model_name: &str,
    status: &str,
    metadata: Option<&HashMap<String, String>>,
) {
    log::info!("🤖 MODEL {} - {}: {}", operation, model_name, status);
    if let Some(meta) = metadata {
        if !meta.is_empty() {
            log::info!("   📋 Model Info: {:?}", meta);
        }
    }
}

// Removed: log_system_resources - use diagnostics::system module instead

/// Log state transitions with validation
pub fn log_state_transition(
    component: &str,
    from_state: &str,
    to_state: &str,
    valid: bool,
    context: Option<&HashMap<String, String>>,
) {
    let status = if valid {
        "✅ VALID"
    } else {
        "⚠️  INVALID"
    };
    log::info!(
        "🔄 STATE [{}]: {} → {} ({})",
        component,
        from_state,
        to_state,
        status
    );

    if let Some(ctx) = context {
        if !ctx.is_empty() {
            log::info!("   📋 State Context: {:?}", ctx);
        }
    }
}

/// Log GPU/Hardware information
#[allow(dead_code)] // Available for hardware diagnostics when needed
pub fn log_hardware_info(component: &str, info: &HashMap<String, String>) {
    log::info!("🎮 HARDWARE [{}]", component);
    if !info.is_empty() {
        log::info!("   📊 Hardware Info: {:?}", info);
    }
}

/// Log file operations with details
pub fn log_file_operation(
    operation: &str,
    path: &str,
    success: bool,
    size_bytes: Option<u64>,
    error: Option<&str>,
) {
    let status = if success { "✅" } else { "❌" };
    let mut log_msg = format!("{} FILE {} - {}", status, operation.to_uppercase(), path);

    if let Some(size) = size_bytes {
        log_msg.push_str(&format!(" ({}KB)", size / 1024));
    }

    log::info!("{}", log_msg);

    if let Some(err) = error {
        log::error!("   ❌ Error: {}", err);
    }
}

// Removed: log_network_operation - use diagnostics::network module instead

// Removed: create_context - use diagnostics::create_context instead

/// Lightweight logging macro for hot paths (no HashMap allocation)
/// Use this instead of log_context! in performance-critical code
#[macro_export]
macro_rules! log_simple {
    ($operation:expr) => {
        #[cfg(debug_assertions)]
        {
            if log::log_enabled!(log::Level::Debug) {
                log::debug!("{}", $operation);
            }
        }
    };
    ($operation:expr, $($arg:expr),*) => {
        #[cfg(debug_assertions)]
        {
            if log::log_enabled!(log::Level::Debug) {
                log::debug!("{}: {}", $operation, format!($($arg),*));
            }
        }
    };
}

/// Macro for quick context creation (optimized for performance)
/// For hot paths (called 1000+ times/sec), use log_context_sampled! instead
#[macro_export]
macro_rules! log_context {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            // Only create HashMap if logging is actually enabled
            // This prevents allocation when logs are filtered out
            if log::log_enabled!(log::Level::Debug) {
                let mut context = std::collections::HashMap::new();
                $(
                    context.insert($key.to_string(), $value.to_string());
                )*
                context
            } else {
                std::collections::HashMap::new()
            }
        }
    };
}

/// Sampled context creation for hot paths (only creates context 1% of the time)
#[macro_export]
macro_rules! log_context_sampled {
    ($($key:expr => $value:expr),* $(,)?) => {
        {
            // Sample at 1% rate for hot paths (thread-safe)
            use once_cell::sync::Lazy;
            use std::sync::atomic::{AtomicUsize, Ordering};

            static SAMPLE_COUNTER: Lazy<AtomicUsize> = Lazy::new(|| AtomicUsize::new(0));
            let count = SAMPLE_COUNTER.fetch_add(1, Ordering::Relaxed);

            if count % 100 == 0 && log::log_enabled!(log::Level::Debug) {
                let mut context = std::collections::HashMap::new();
                $(
                    context.insert($key.to_string(), $value.to_string());
                )*
                context
            } else {
                std::collections::HashMap::new()
            }
        }
    };
}

/// Log application lifecycle events
pub fn log_lifecycle_event(
    event: &str,
    version: Option<&str>,
    context: Option<&HashMap<String, String>>,
) {
    let version_str = version.unwrap_or("unknown");
    log::info!("🚀 LIFECYCLE {} - Version: {}", event, version_str);

    if let Some(ctx) = context {
        if !ctx.is_empty() {
            log::info!("   📋 Context: {:?}", ctx);
        }
    }
}

// ============================================================================
// SIMPLE LOGGING HELPERS - Phase 1 of simplification
// ============================================================================
// These are new, simple helpers that will gradually replace the complex logging
// infrastructure above. They use standard log macros directly for simplicity.

/// Simple log helper for operation start
pub fn log_start(operation: &str) {
    log::info!("🚀 {} STARTING", operation);
}

/// Simple log helper for successful completion
pub fn log_complete(operation: &str, duration_ms: u64) {
    log::info!("✅ {} COMPLETE in {}ms", operation, duration_ms);
}

/// Simple log helper for failures
pub fn log_failed(operation: &str, error: &str) {
    log::error!("❌ {} FAILED: {}", operation, error);
}

/// Simple contextual logging without HashMap overhead
pub fn log_with_context(level: log::Level, operation: &str, context: &[(&str, &str)]) {
    if !log::log_enabled!(level) {
        return;
    }

    let ctx_str: Vec<String> = context
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    let ctx = if ctx_str.is_empty() {
        String::new()
    } else {
        format!(" | {}", ctx_str.join(", "))
    };

    match level {
        log::Level::Info => log::info!("{}{}", operation, ctx),
        log::Level::Debug => log::debug!("{}{}", operation, ctx),
        log::Level::Warn => log::warn!("{}{}", operation, ctx),
        log::Level::Error => log::error!("{}{}", operation, ctx),
        _ => log::info!("{}{}", operation, ctx),
    }
}

/// Log critical path operations (recording, transcription, etc)
#[allow(dead_code)] // Useful for marking critical operations in future
pub fn log_critical_operation(operation: &str, status: &str, details: Option<&str>) {
    if let Some(d) = details {
        log::info!("⭐ {} - {}: {}", operation, status, d);
    } else {
        log::info!("⭐ {} - {}", operation, status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_context() {
        // Local helper for test
        fn create_context(pairs: &[(&str, &str)]) -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        }

        let context = create_context(&[
            ("model", "base.en"),
            ("language", "auto"),
            ("duration", "3.5s"),
        ]);

        assert_eq!(context.len(), 3);
        assert_eq!(context.get("model"), Some(&"base.en".to_string()));
        assert_eq!(context.get("language"), Some(&"auto".to_string()));
        assert_eq!(context.get("duration"), Some(&"3.5s".to_string()));
    }

    #[test]
    fn test_log_context_macro() {
        // Context is created only if logging is enabled
        // This prevents unnecessary allocations when logs are filtered
        let context = log_context! {
            "operation" => "transcription",
            "model" => "whisper-base",
            "duration" => "2.3s"
        };

        // The context creation depends on log level
        if log::log_enabled!(log::Level::Debug) {
            assert_eq!(context.len(), 3);
            assert_eq!(context.get("operation"), Some(&"transcription".to_string()));
        } else {
            // Context is empty when logging is disabled (performance optimization)
            assert_eq!(context.len(), 0);
        }
    }
}
