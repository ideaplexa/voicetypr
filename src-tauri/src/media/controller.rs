//! Cross-platform media pause controller.
//!
//! Pauses system media when recording starts and resumes when recording stops.
//! Only resumes if WE paused it (not if user manually paused during recording).

use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU32, AtomicU8};

#[cfg(target_os = "windows")]
use parking_lot::Mutex;

#[cfg(target_os = "macos")]
use std::{
    io::Write,
    process::{Command as ProcessCommand, Stdio},
    time::Duration,
};

#[cfg(target_os = "macos")]
const NOW_PLAYING_JXA_SCRIPT: &str = r#"
function run() {
  const MediaRemote = $.NSBundle.bundleWithPath(
    "/System/Library/PrivateFrameworks/MediaRemote.framework/",
  );
  MediaRemote.load;

  const MRNowPlayingRequest = $.NSClassFromString("MRNowPlayingRequest");
  const client = MRNowPlayingRequest.localNowPlayingPlayerPath.client;
  const clientConverted = {
    bundleIdentifier: client.bundleIdentifier.js,
    parentApplicationBundleIdentifier:
      client.parentApplicationBundleIdentifier.js,
  };

  const infoDict = MRNowPlayingRequest.localNowPlayingItem.nowPlayingInfo;
  const infoConverted = {};
  for (const key in infoDict.js) {
    const value = infoDict.valueForKey(key).js;
    if (typeof value !== "object") {
      infoConverted[key] = value;
    } else if (value && typeof value.getTime === "function") {
      try {
        infoConverted[key] = value.getTime();
      } catch (e) {
        infoConverted[key] = value.toString();
      }
    } else {
      infoConverted[key] = value.toString();
    }
  }

  return JSON.stringify({
    isPlaying: MRNowPlayingRequest.localIsPlaying,
    client: clientConverted,
    info: infoConverted,
  });
}
"#;

#[cfg(target_os = "macos")]
#[derive(Debug, Clone)]
struct NowPlayingSnapshot {
    is_playing: Option<bool>,
}

#[cfg(target_os = "macos")]
fn now_playing_snapshot_via_osascript() -> Option<NowPlayingSnapshot> {
    let mut child = ProcessCommand::new("/usr/bin/osascript")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("-l")
        .arg("JavaScript")
        .spawn()
        .ok()?;

    {
        let stdin = child.stdin.as_mut()?;
        stdin.write_all(NOW_PLAYING_JXA_SCRIPT.as_bytes()).ok()?;
    }
    // osascript reads the program until EOF; if the write end stays open in
    // the Child, it never executes and every bounded wait times out.
    drop(child.stdin.take());

    let output = wait_for_output_bounded(child, Duration::from_millis(750)).ok()?;
    if !output.status.success() {
        if log::log_enabled!(log::Level::Debug) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Never log process output verbatim: MediaRemote output can
            // carry the user's now-playing title/artist/album (plan 044
            // review). Lengths + exit status are enough to diagnose.
            log::debug!(
                "osascript now playing query failed | status={:?} stdout_len={} stderr_len={}",
                output.status,
                output.stdout.len(),
                stderr.len()
            );
        }
        return None;
    }

    let raw: serde_json::Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(err) => {
            if log::log_enabled!(log::Level::Debug) {
                let stderr = String::from_utf8_lossy(&output.stderr);
                // Same privacy rule: lengths only, never raw JSON payloads.
                log::debug!(
                    "osascript now playing JSON parse failed | error={:?} stdout_len={} stderr_len={}",
                    err,
                    output.stdout.len(),
                    stderr.len()
                );
            }

            return None;
        }
    };
    let is_playing = raw.get("isPlaying").and_then(|v| v.as_bool());

    Some(NowPlayingSnapshot { is_playing })
}

/// Wait for a child process with a hard deadline, killing it on timeout.
/// The layered pause joins synchronously on the stop path; an unbounded
/// `wait_with_output` on a stuck `osascript` would hang recording stop and
/// every command queued behind the recorder mutex.
#[cfg(target_os = "macos")]
fn wait_for_output_bounded(
    mut child: std::process::Child,
    timeout: Duration,
) -> std::io::Result<std::process::Output> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(_) => return child.wait_with_output(),
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "osascript now-playing query timed out",
                ));
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }
}

/// Controller for pausing/resuming system media during voice recording.
pub struct MediaPauseController {
    /// Tracks if we paused the media (so we know whether to resume)
    was_playing_before_recording: AtomicBool,

    /// How the current pause was achieved on macOS, so resume inverts the
    /// same mechanism (command vs. media key vs. output mute).
    #[cfg(target_os = "macos")]
    pause_mechanism: AtomicU8,

    /// True when the output device was already muted before we muted it
    /// (never unmute a device the user muted themselves).
    #[cfg(target_os = "macos")]
    was_muted_before_recording: AtomicBool,

    /// In-flight layered pause started at recording begin (macOS only, so
    /// the ~300ms layered logic stays off the `Starting` critical path).
    #[cfg(target_os = "macos")]
    pending_pause: parking_lot::Mutex<Option<PendingPause>>,

    /// The output device we muted (macOS mute layer), so resume unmutes the
    /// same device even if the system default changed mid-recording.
    #[cfg(target_os = "macos")]
    muted_output_device: AtomicU32,

    /// On Windows, track which media session we paused so we only resume the same session.
    #[cfg(target_os = "windows")]
    paused_session_source_app_user_model_id: Mutex<Option<String>>,
}

impl Default for MediaPauseController {
    fn default() -> Self {
        Self::new()
    }
}
impl MediaPauseController {
    pub fn new() -> Self {
        Self {
            was_playing_before_recording: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            pause_mechanism: AtomicU8::new(PAUSE_MECHANISM_NONE),
            #[cfg(target_os = "macos")]
            was_muted_before_recording: AtomicBool::new(false),
            #[cfg(target_os = "macos")]
            pending_pause: parking_lot::Mutex::new(None),
            #[cfg(target_os = "macos")]
            muted_output_device: AtomicU32::new(0),
            #[cfg(target_os = "windows")]
            paused_session_source_app_user_model_id: Mutex::new(None),
        }
    }

    /// Pause media if currently playing. Call when recording starts.
    /// Returns true if media was paused.
    pub fn pause_if_playing(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.pause_if_playing_macos()
        }

        #[cfg(target_os = "windows")]
        {
            self.pause_if_playing_windows()
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            log::debug!("Media pause not supported on this platform");
            false
        }
    }

    /// Resume media if we paused it. Call when recording stops.
    /// Returns true if media was resumed.
    #[allow(clippy::needless_return)]
    pub fn resume_if_we_paused(&self) -> bool {
        // Resolve an in-flight pause first (under the lifecycle lock): a
        // very short recording can stop before the pause worker finished,
        // and its outcome must be applied before the decision below. This
        // must happen BEFORE the was_playing swap.
        #[cfg(target_os = "macos")]
        {
            let mut guard = self.pending_pause.lock();
            self.join_pending_pause_locked(&mut guard);
        }

        if self
            .was_playing_before_recording
            .swap(false, Ordering::SeqCst)
        {
            #[cfg(target_os = "macos")]
            {
                self.resume_macos()
            }

            #[cfg(target_os = "windows")]
            {
                return self.resume_windows();
            }

            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                false
            }
        } else {
            false
        }
    }

    /// Reset state without resuming (e.g., if app is closing)
    #[allow(dead_code)]
    pub fn reset(&self) {
        #[cfg(target_os = "macos")]
        {
            let mut guard = self.pending_pause.lock();
            self.join_pending_pause_locked(&mut guard);
        }

        self.was_playing_before_recording
            .store(false, Ordering::SeqCst);

        #[cfg(target_os = "macos")]
        {
            self.pause_mechanism
                .store(PAUSE_MECHANISM_NONE, Ordering::SeqCst);
            self.was_muted_before_recording
                .store(false, Ordering::SeqCst);
        }

        #[cfg(target_os = "windows")]
        {
            *self.paused_session_source_app_user_model_id.lock() = None;
        }
    }
}
// ============================================
// macOS implementation: layered pause with verification.
//
// Layer 1 — MediaRemote `pause` command via the `media-remote` crate.
//   Works for native players that accept remote commands (Music, Spotify…).
// Layer 2 — NX_KEYTYPE_PLAY system-defined HID event (what the F8 media key
//   produces). Reaches players that ignore MediaRemote but obey the key
//   (e.g. Plexamp).
// Layer 3 — Mute the default output device via CoreAudio. The only layer a
//   browser-based player cannot ignore; this is the documented fallback for
//   browsers (verified live: a Chromium-embedded player accepted layers 1–2
//   with "success" while still playing).
// Every layer is verified through now-playing state before being trusted,
// and resume inverts exactly the mechanism that worked.
#[cfg(target_os = "macos")]
const PAUSE_MECHANISM_NONE: u8 = 0;
#[cfg(target_os = "macos")]
const PAUSE_MECHANISM_COMMAND: u8 = 1;
#[cfg(target_os = "macos")]
const PAUSE_MECHANISM_KEY: u8 = 2;
#[cfg(target_os = "macos")]
const PAUSE_MECHANISM_MUTE: u8 = 3;

#[cfg(target_os = "macos")]
enum PauseOutcome {
    NotPlaying,
    Command,
    Key,
    Mute {
        was_muted_before: bool,
        device_id: u32,
    },
    Failed,
}

#[cfg(target_os = "macos")]
struct PendingPause {
    handle: std::thread::JoinHandle<PauseOutcome>,
}

#[cfg(target_os = "macos")]
impl MediaPauseController {
    /// Application-exit cleanup: resolve any in-flight pause and restore a
    /// device we muted. Player pause state is left alone (the player owns
    /// it); the system output mute is ours to restore.
    pub fn cleanup_on_exit(&self) {
        let mut guard = self.pending_pause.lock();
        self.join_pending_pause_locked(&mut guard);
        let mechanism = self
            .pause_mechanism
            .swap(PAUSE_MECHANISM_NONE, Ordering::SeqCst);
        let device = self.muted_output_device.swap(0, Ordering::SeqCst);
        let was_user_muted = self
            .was_muted_before_recording
            .swap(false, Ordering::SeqCst);
        self.was_playing_before_recording
            .store(false, Ordering::SeqCst);
        drop(guard);
        if mechanism == PAUSE_MECHANISM_MUTE && !was_user_muted && device != 0 {
            if let Err(err) = set_output_device_muted(device, false) {
                log::warn!("⚠️ Exit unmute failed for device {}: {}", device, err);
            }
        }
    }
}

#[cfg(target_os = "macos")]
impl MediaPauseController {
    /// Kick off the layered pause off the recording-start critical path.
    /// The layered logic can cost ~300ms (state check + verification polls);
    /// running it on a worker keeps `Starting` latency untouched. The stop
    /// and cancel paths join the pending pause before resuming.
    ///
    /// All lifecycle mutations (spawn publication, join, resume, exit
    /// cleanup) hold `pending_pause` for their whole critical section so
    /// they are mutually exclusive — an exit can never race a stop-path
    /// join into dropping a completed mute.
    fn pause_if_playing_macos(&self) -> bool {
        let mut guard = self.pending_pause.lock();
        self.join_pending_pause_locked(&mut guard);
        // Still holding a pause from the previous recording (user re-recorded
        // before resume): keep it — re-running the layers would either pause
        // a second time or flip the was-muted flag on our own mute.
        if self.pause_mechanism.load(Ordering::SeqCst) != PAUSE_MECHANISM_NONE {
            log::debug!("Media already paused by previous recording; keeping it paused");
            return true;
        }
        let handle = std::thread::spawn(perform_layered_pause);
        *guard = Some(PendingPause { handle });
        true
    }

    fn resume_macos(&self) -> bool {
        let mut guard = self.pending_pause.lock();
        self.join_pending_pause_locked(&mut guard);
        let mechanism = self
            .pause_mechanism
            .swap(PAUSE_MECHANISM_NONE, Ordering::SeqCst);
        match mechanism {
            PAUSE_MECHANISM_COMMAND => {
                if now_playing_snapshot_via_osascript()
                    .and_then(|s| s.is_playing)
                    .unwrap_or(false)
                {
                    log::debug!("Media already playing, skipping resume");
                    return false;
                }
                log::info!("🎵 Resuming media via MediaRemote play...");
                media_remote::send_command(media_remote::Command::Play)
            }
            PAUSE_MECHANISM_KEY => {
                // The media key is a TOGGLE: only post it when now-playing
                // is specifically Some(false). An unknown state must skip —
                // toggling already-playing media would pause it.
                match now_playing_snapshot_via_osascript().and_then(|s| s.is_playing) {
                    Some(false) => {
                        log::info!("🎵 Resuming media via media key event...");
                        post_media_play_pause_key();
                        true
                    }
                    Some(true) => {
                        log::debug!("Media already playing, skipping key resume");
                        false
                    }
                    None => {
                        log::debug!("Now-playing state unknown; skipping key resume");
                        false
                    }
                }
            }
            PAUSE_MECHANISM_MUTE => {
                if self.was_muted_before_recording.load(Ordering::SeqCst) {
                    log::debug!("Output was muted before recording; leaving muted");
                    self.was_muted_before_recording
                        .store(false, Ordering::SeqCst);
                    return true;
                }
                let device = self.muted_output_device.swap(0, Ordering::SeqCst);
                log::info!("🎵 Unmuting output device {}...", device);
                match set_output_device_muted(device, false) {
                    Ok(()) => {
                        self.was_muted_before_recording
                            .store(false, Ordering::SeqCst);
                        true
                    }
                    Err(err) => {
                        // Keep the mute state published so a later stop or
                        // cancel path retries the unmute instead of
                        // stranding a muted output device.
                        log::warn!("⚠️ Failed to unmute output (will retry): {}", err);
                        self.set_pause_state(PAUSE_MECHANISM_MUTE, false, device);
                        false
                    }
                }
            }
            _ => false,
        }
    }

    /// Join and apply a pending pause while the lifecycle lock is held.
    fn join_pending_pause_locked(
        &self,
        guard: &mut parking_lot::MutexGuard<'_, Option<PendingPause>>,
    ) {
        if let Some(pending) = guard.take() {
            match pending.handle.join() {
                Ok(outcome) => self.apply_outcome(outcome),
                Err(_) => log::warn!("⚠️ Media pause worker panicked"),
            }
        }
    }

    fn apply_outcome(&self, outcome: PauseOutcome) {
        match outcome {
            PauseOutcome::NotPlaying => {
                log::debug!("No media playing, nothing to pause");
                self.clear_pause_state();
            }
            PauseOutcome::Command => {
                log::info!("✅ Media paused via MediaRemote command");
                self.set_pause_state(PAUSE_MECHANISM_COMMAND, false, 0);
            }
            PauseOutcome::Key => {
                log::info!("✅ Media paused via media key event");
                self.set_pause_state(PAUSE_MECHANISM_KEY, false, 0);
            }
            PauseOutcome::Mute {
                was_muted_before,
                device_id,
            } => {
                log::info!("✅ Media muted via CoreAudio (player ignored pause commands)");
                self.set_pause_state(PAUSE_MECHANISM_MUTE, was_muted_before, device_id);
            }
            PauseOutcome::Failed => {
                log::warn!("⚠️ All media pause layers failed");
                self.clear_pause_state();
            }
        }
    }

    /// Publish pause state. Ordering matters: the mechanism and mute
    /// provenance become visible BEFORE the `was_playing` ready flag, so a
    /// racing resume that observes the flag also observes a complete state.
    fn set_pause_state(&self, mechanism: u8, was_muted_before: bool, device_id: u32) {
        self.pause_mechanism.store(mechanism, Ordering::SeqCst);
        self.was_muted_before_recording
            .store(was_muted_before, Ordering::SeqCst);
        self.muted_output_device.store(device_id, Ordering::SeqCst);
        self.was_playing_before_recording
            .store(true, Ordering::SeqCst);
    }

    fn clear_pause_state(&self) {
        self.was_playing_before_recording
            .store(false, Ordering::SeqCst);
        self.pause_mechanism
            .store(PAUSE_MECHANISM_NONE, Ordering::SeqCst);
        self.was_muted_before_recording
            .store(false, Ordering::SeqCst);
        self.muted_output_device.store(0, Ordering::SeqCst);
    }
}

/// The layered pause, as a free function so it can run on a worker thread
/// without borrowing the controller.
#[cfg(target_os = "macos")]
fn perform_layered_pause() -> PauseOutcome {
    let is_playing = now_playing_snapshot_via_osascript()
        .and_then(|s| s.is_playing)
        .unwrap_or(false);
    if !is_playing {
        return PauseOutcome::NotPlaying;
    }

    log::info!("🎵 Media is playing, pausing for recording...");

    // Layer 1: MediaRemote pause command.
    if media_remote::send_command(media_remote::Command::Pause) && wait_until_paused(250) {
        return PauseOutcome::Command;
    }

    // Layer 2: hardware media-key event.
    post_media_play_pause_key();
    if wait_until_paused(250) {
        return PauseOutcome::Key;
    }
    // Layer 3: mute the default output device.
    match mute_default_output() {
        Ok((was_muted_before, device_id)) => PauseOutcome::Mute {
            was_muted_before,
            device_id,
        },
        Err(err) => {
            log::warn!("⚠️ Mute fallback failed: {}", err);
            PauseOutcome::Failed
        }
    }
}

/// Poll now-playing state until it reports paused, or `timeout_ms` elapses.
#[cfg(target_os = "macos")]
fn wait_until_paused(timeout_ms: u64) -> bool {
    let mut waited = 0u64;
    while waited <= timeout_ms {
        match now_playing_snapshot_via_osascript() {
            Some(snapshot) => match snapshot.is_playing {
                Some(false) => return true,
                Some(true) => {}
                None => return false,
            },
            None => return false,
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        waited += 50;
    }
    false
}

/// Post the hardware play/pause media-key event (NX_KEYTYPE_PLAY = 16) as a
/// system-defined event — the same HID event the physical F8 key produces,
/// and the same technique VoiceInk and Hammerspoon use.
#[cfg(target_os = "macos")]
fn post_media_play_pause_key() {
    const NSEVENT_TYPE_SYSTEM_DEFINED: usize = 14;
    const NX_SUBTYPE_AUX_CONTROL_BUTTONS: i16 = 8;
    const NX_KEYTYPE_PLAY: isize = 16;
    const K_CGHID_EVENT_TAP: u32 = 0;

    extern "C" {
        fn CGEventPost(tap: u32, event: *const std::ffi::c_void);
    }

    unsafe {
        let class = objc2::runtime::AnyClass::get(c"NSEvent").expect("NSEvent class");
        let origin = objc2_foundation::NSPoint::new(0.0, 0.0);
        // The worker thread has no run loop; without a pool the autoreleased
        // NSEvents from otherEventWithType: would leak for the process
        // lifetime on every layer-2 attempt.
        objc2::rc::autoreleasepool(|_| {
            for down in [true, false] {
                let state: isize = if down { 0xa } else { 0xb };
                let flags: usize = if down { 0xa00 } else { 0xb00 };
                let event: *mut objc2::runtime::AnyObject = objc2::msg_send![
                    class,
                    otherEventWithType: NSEVENT_TYPE_SYSTEM_DEFINED,
                    location: origin,
                    modifierFlags: flags,
                    timestamp: 0f64,
                    windowNumber: 0isize,
                    context: std::ptr::null_mut::<objc2::runtime::AnyObject>(),
                    subtype: NX_SUBTYPE_AUX_CONTROL_BUTTONS,
                    data1: (NX_KEYTYPE_PLAY << 16) | (state << 8),
                    data2: -1isize,
                ];
                if event.is_null() {
                    log::warn!("Failed to create media key NSEvent");
                    return;
                }
                let cg_event: *const std::ffi::c_void = objc2::msg_send![event, CGEvent];
                if cg_event.is_null() {
                    log::warn!("Media key NSEvent had no CGEvent");
                    return;
                }
                CGEventPost(K_CGHID_EVENT_TAP, cg_event);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        });
    }
}

/// Mute the default output device. Returns `(was_muted_before, device_id)`
/// so resume can restore exactly this device's previous state even if the
/// system default changes mid-recording.
#[cfg(target_os = "macos")]
fn mute_default_output() -> Result<(bool, u32), String> {
    use coreaudio_sys::{
        kAudioDevicePropertyMute, kAudioDevicePropertyScopeOutput,
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectGetPropertyData,
        AudioObjectHasProperty, AudioObjectPropertyAddress, AudioObjectSetPropertyData,
    };

    unsafe {
        let mut device: u32 = 0;
        let mut size: u32 = std::mem::size_of::<u32>() as u32;
        let default_addr = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let status = AudioObjectGetPropertyData(
            kAudioObjectSystemObject,
            &default_addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut device as *mut u32 as *mut std::ffi::c_void,
        );
        if status != 0 {
            return Err(format!("default output device lookup failed: {}", status));
        }

        let mute_addr = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyMute,
            mScope: kAudioDevicePropertyScopeOutput,
            mElement: kAudioObjectPropertyElementMain,
        };
        if AudioObjectHasProperty(device, &mute_addr) == 0 {
            return Err("output device has no mute property".to_string());
        }

        let mut previous: u32 = 0;
        let status = AudioObjectGetPropertyData(
            device,
            &mute_addr,
            0,
            std::ptr::null(),
            &mut size,
            &mut previous as *mut u32 as *mut std::ffi::c_void,
        );
        if status != 0 {
            return Err(format!("mute state read failed: {}", status));
        }

        let value: u32 = 1;
        let status = AudioObjectSetPropertyData(
            device,
            &mute_addr,
            0,
            std::ptr::null(),
            std::mem::size_of::<u32>() as u32,
            &value as *const u32 as *const std::ffi::c_void,
        );
        if status != 0 {
            return Err(format!("mute set failed: {}", status));
        }

        Ok((previous != 0, device))
    }
}

/// Set the mute property of a specific output device. A `device_id` of 0
/// falls back to the current default (defensive only; the layered pause
/// always records the real id).
#[cfg(target_os = "macos")]
fn set_output_device_muted(device_id: u32, mute: bool) -> Result<(), String> {
    use coreaudio_sys::{
        kAudioDevicePropertyMute, kAudioDevicePropertyScopeOutput,
        kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectGetPropertyData,
        AudioObjectHasProperty, AudioObjectPropertyAddress, AudioObjectSetPropertyData,
    };

    unsafe {
        let device = if device_id != 0 {
            device_id
        } else {
            let mut device: u32 = 0;
            let mut size: u32 = std::mem::size_of::<u32>() as u32;
            let default_addr = AudioObjectPropertyAddress {
                mSelector: kAudioHardwarePropertyDefaultOutputDevice,
                mScope: kAudioObjectPropertyScopeGlobal,
                mElement: kAudioObjectPropertyElementMain,
            };
            let status = AudioObjectGetPropertyData(
                kAudioObjectSystemObject,
                &default_addr,
                0,
                std::ptr::null(),
                &mut size,
                &mut device as *mut u32 as *mut std::ffi::c_void,
            );
            if status != 0 {
                return Err(format!("default output device lookup failed: {}", status));
            }
            device
        };

        let mute_addr = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyMute,
            mScope: kAudioDevicePropertyScopeOutput,
            mElement: kAudioObjectPropertyElementMain,
        };
        if AudioObjectHasProperty(device, &mute_addr) == 0 {
            return Err("output device has no mute property".to_string());
        }

        let value: u32 = if mute { 1 } else { 0 };
        let status = AudioObjectSetPropertyData(
            device,
            &mute_addr,
            0,
            std::ptr::null(),
            std::mem::size_of::<u32>() as u32,
            &value as *const u32 as *const std::ffi::c_void,
        );
        if status != 0 {
            return Err(format!("mute set failed: {}", status));
        }

        Ok(())
    }
}

// ============================================
// Windows Implementation (GSMTC - Global System Media Transport Controls)
// ============================================
// Uses Windows.Media.Control APIs to properly detect playback state
// and use explicit pause/play (not toggle). Requires Windows 10 1809+.
#[cfg(target_os = "windows")]
impl MediaPauseController {
    fn try_pause_session(
        &self,
        session: &windows::Media::Control::GlobalSystemMediaTransportControlsSession,
    ) -> bool {
        // We can only resume a session we can re-identify later by its app id. If the id is
        // unreadable, pausing it would risk stranding it (or resuming the wrong player) at stop,
        // so skip it rather than break the resume-only-what-we-paused contract.
        let source_app_id = match session.SourceAppUserModelId().ok().map(|id| id.to_string()) {
            Some(id) if !id.is_empty() => id,
            _ => {
                log::debug!("Skipping media pause for a session with no readable app id");
                return false;
            }
        };
        match session.TryPauseAsync() {
            Ok(op) => match op.join() {
                Ok(true) => {
                    log::info!("Media paused successfully via GSMTC");
                    self.was_playing_before_recording
                        .store(true, Ordering::SeqCst);
                    *self.paused_session_source_app_user_model_id.lock() = Some(source_app_id);
                    true
                }
                Ok(false) => {
                    log::info!("GSMTC TryPauseAsync returned false; trying next candidate");
                    false
                }
                Err(e) => {
                    log::warn!("Failed to pause media: {:?}; trying next candidate", e);
                    false
                }
            },
            Err(e) => {
                log::warn!("Failed to request pause: {:?}; trying next candidate", e);
                false
            }
        }
    }

    fn pause_if_playing_windows(&self) -> bool {
        use std::{thread, time::Duration};
        use windows::Media::Control::{
            GlobalSystemMediaTransportControlsSession,
            GlobalSystemMediaTransportControlsSessionManager,
            GlobalSystemMediaTransportControlsSessionPlaybackStatus,
        };

        // Get the session manager (blocking wait with .join())
        let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            Ok(op) => match op.join() {
                Ok(mgr) => mgr,
                Err(e) => {
                    log::warn!("Failed to get GSMTC session manager: {:?}", e);
                    return false;
                }
            },
            Err(e) => {
                log::warn!("Failed to request GSMTC session manager: {:?}", e);
                return false;
            }
        };

        fn is_playing(session: &GlobalSystemMediaTransportControlsSession) -> bool {
            let playback_info = match session.GetPlaybackInfo() {
                Ok(info) => info,
                Err(_) => return false,
            };

            let status = match playback_info.PlaybackStatus() {
                Ok(status) => status,
                Err(_) => return false,
            };

            status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing
        }

        fn timeline_position_ticks(
            session: &GlobalSystemMediaTransportControlsSession,
        ) -> Option<i64> {
            let timeline = session.GetTimelineProperties().ok()?;
            Some(timeline.Position().ok()?.Duration)
        }

        let mut current_session = manager.GetCurrentSession().ok();
        let current_id = current_session
            .as_ref()
            .and_then(|session| session.SourceAppUserModelId().ok().map(|id| id.to_string()));

        let mut all: Vec<(String, GlobalSystemMediaTransportControlsSession)> = Vec::new();
        if let Ok(sessions) = manager.GetSessions() {
            if let Ok(size) = sessions.Size() {
                for i in 0..size {
                    let session = match sessions.GetAt(i) {
                        Ok(session) => session,
                        Err(_) => continue,
                    };

                    // Keep the session even if its app id is unreadable (id is only used for
                    // dedup/ordering; resume bookkeeping is re-read on pause), so a playing but
                    // id-less session is still attempted.
                    let id = session
                        .SourceAppUserModelId()
                        .ok()
                        .map(|id| id.to_string())
                        .unwrap_or_default();

                    all.push((id, session));
                }
            }
        }

        if let Some(cid) = current_id.clone() {
            if !all.iter().any(|(id, _)| id == &cid) {
                if let Some(session) = current_session.take() {
                    all.push((cid, session));
                }
            }
        }

        let mut attempted: Vec<usize> = Vec::new();
        let mut had_reported_candidates = false;

        let mut phase1_order: Vec<usize> = Vec::new();
        if let Some(ref cid) = current_id {
            if let Some(idx) = all.iter().position(|(id, _)| id == cid) {
                phase1_order.push(idx);
            }
        }
        for (idx, (id, _)) in all.iter().enumerate() {
            if current_id.as_ref() != Some(id) {
                phase1_order.push(idx);
            }
        }

        for &idx in &phase1_order {
            let session = &all[idx].1;
            if !is_playing(session) {
                continue;
            }
            if !had_reported_candidates {
                had_reported_candidates = true;
                log::info!("Media is playing, pausing for recording...");
            }
            attempted.push(idx);
            if self.try_pause_session(session) {
                return true;
            }
        }

        // Fallback: some sessions occasionally report non-Playing states even while audio is
        // progressing. If we can observe timeline position advancing over a short interval,
        // treat it as playing and pause it.
        let mut timeline_probe: Vec<(usize, i64)> = Vec::new();
        for (idx, (_, session)) in all.iter().enumerate() {
            if attempted.contains(&idx) {
                continue;
            }
            let pos = timeline_position_ticks(session).unwrap_or(0);
            timeline_probe.push((idx, pos));
        }

        let mut timeline_candidates: Vec<usize> = Vec::new();
        let had_timeline_probe_entries = !timeline_probe.is_empty();

        if had_timeline_probe_entries {
            if let Some(ref current_session_id) = current_id {
                if let Some(pos) = timeline_probe
                    .iter()
                    .position(|(idx, _)| all[*idx].0 == *current_session_id)
                {
                    let current = timeline_probe.remove(pos);
                    timeline_probe.insert(0, current);
                }
            }

            thread::sleep(Duration::from_millis(120));

            // 1 tick = 100ns, so 50ms = 500_000 ticks.
            const DELTA_THRESHOLD_TICKS: i64 = 50 * 10_000;

            for (idx, before) in timeline_probe {
                let (id, session) = &all[idx];
                let after = timeline_position_ticks(session).unwrap_or(before);
                let delta = after.saturating_sub(before);

                if delta > DELTA_THRESHOLD_TICKS {
                    log::info!(
                        "Inferred playing session via timeline movement | source_app_id={} delta_ms={}",
                        id,
                        delta / 10_000
                    );
                    timeline_candidates.push(idx);
                }
            }
        }
        let had_timeline_candidates = !timeline_candidates.is_empty();
        for idx in timeline_candidates {
            let session = &all[idx].1;
            if self.try_pause_session(session) {
                return true;
            }
        }

        self.was_playing_before_recording
            .store(false, Ordering::SeqCst);
        *self.paused_session_source_app_user_model_id.lock() = None;

        if !had_reported_candidates && !had_timeline_candidates {
            log::info!("No playing media session found");
        } else {
            log::info!("No media session could be paused");
        }
        false
    }

    fn resume_windows(&self) -> bool {
        use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager;

        log::info!("Resuming media playback via GSMTC...");

        // Get the session manager (blocking wait with .join())
        let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            Ok(op) => match op.join() {
                Ok(mgr) => mgr,
                Err(e) => {
                    log::warn!("Failed to get GSMTC session manager for resume: {:?}", e);
                    return false;
                }
            },
            Err(e) => {
                log::warn!(
                    "Failed to request GSMTC session manager for resume: {:?}",
                    e
                );
                return false;
            }
        };

        let paused_id = self.paused_session_source_app_user_model_id.lock().take();

        let session = if let Some(paused_id) = paused_id {
            let sessions = match manager.GetSessions() {
                Ok(sessions) => sessions,
                Err(e) => {
                    log::warn!("Failed to enumerate GSMTC sessions for resume: {:?}", e);
                    return false;
                }
            };

            let size = match sessions.Size() {
                Ok(size) => size,
                Err(e) => {
                    log::warn!("Failed to read GSMTC sessions size for resume: {:?}", e);
                    return false;
                }
            };

            let mut found = None;
            for i in 0..size {
                let session = match sessions.GetAt(i) {
                    Ok(session) => session,
                    Err(_) => continue,
                };

                let session_id = match session.SourceAppUserModelId() {
                    Ok(id) => id.to_string(),
                    Err(_) => continue,
                };

                if session_id == paused_id {
                    found = Some(session);
                    break;
                }
            }

            match found {
                Some(session) => session,
                None => {
                    log::info!("Paused media session is no longer available; skipping resume");
                    return false;
                }
            }
        } else {
            // We only resume the exact session we paused. With no stored id we must not guess —
            // resuming GetCurrentSession could play a player the user intentionally left paused.
            log::info!("No paused media session id recorded; skipping resume");
            return false;
        };

        // If it is already playing, don't send play.
        if let Ok(playback_info) = session.GetPlaybackInfo() {
            if let Ok(status) = playback_info.PlaybackStatus() {
                use windows::Media::Control::GlobalSystemMediaTransportControlsSessionPlaybackStatus;
                if status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing {
                    log::info!("Media already playing, skipping resume");
                    return false;
                }
            }
        }

        // Use explicit play (not toggle!)
        match session.TryPlayAsync() {
            Ok(op) => match op.join() {
                Ok(success) => {
                    if success {
                        log::info!("Media resumed successfully via GSMTC");
                        true
                    } else {
                        log::warn!("GSMTC TryPlayAsync returned false");
                        false
                    }
                }
                Err(e) => {
                    log::warn!("Failed to resume media: {:?}", e);
                    false
                }
            },
            Err(e) => {
                log::warn!("Failed to request play: {:?}", e);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_controller_creation() {
        let controller = MediaPauseController::new();
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_new_controller_has_no_pause_mechanism() {
        let controller = MediaPauseController::new();
        assert_eq!(
            controller.pause_mechanism.load(Ordering::SeqCst),
            PAUSE_MECHANISM_NONE
        );
        assert!(!controller.was_muted_before_recording.load(Ordering::SeqCst));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_resume_without_mechanism_is_noop() {
        let controller = MediaPauseController::new();
        controller
            .was_playing_before_recording
            .store(true, Ordering::SeqCst);
        // mechanism == NONE: resume must not touch any OS API and report false.
        assert!(!controller.resume_macos());
        assert_eq!(
            controller.pause_mechanism.load(Ordering::SeqCst),
            PAUSE_MECHANISM_NONE
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_resume_mute_preserves_user_muted_device() {
        // User muted the device themselves before recording: resume must keep
        // it muted and report success without touching CoreAudio.
        let controller = MediaPauseController::new();
        controller
            .was_playing_before_recording
            .store(true, Ordering::SeqCst);
        controller
            .pause_mechanism
            .store(PAUSE_MECHANISM_MUTE, Ordering::SeqCst);
        controller
            .was_muted_before_recording
            .store(true, Ordering::SeqCst);
        assert!(controller.resume_macos());
        // mechanism consumed
        assert_eq!(
            controller.pause_mechanism.load(Ordering::SeqCst),
            PAUSE_MECHANISM_NONE
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_reset_clears_mechanism() {
        let controller = MediaPauseController::new();
        controller
            .pause_mechanism
            .store(PAUSE_MECHANISM_COMMAND, Ordering::SeqCst);
        controller
            .was_muted_before_recording
            .store(true, Ordering::SeqCst);
        controller.reset();
        assert_eq!(
            controller.pause_mechanism.load(Ordering::SeqCst),
            PAUSE_MECHANISM_NONE
        );
        assert!(!controller.was_muted_before_recording.load(Ordering::SeqCst));
    }

    #[test]
    fn test_default_impl() {
        let controller = MediaPauseController::default();
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[test]
    fn test_resume_without_pause_does_nothing() {
        let controller = MediaPauseController::new();
        // Should return false since we didn't pause anything
        assert!(!controller.resume_if_we_paused());
    }

    #[test]
    fn test_resume_clears_was_playing_flag() {
        let controller = MediaPauseController::new();
        // Manually set the flag to true
        controller
            .was_playing_before_recording
            .store(true, Ordering::SeqCst);

        // Resume should clear the flag (swap returns old value)
        // Note: actual resume behavior depends on platform APIs
        let _ = controller.resume_if_we_paused();

        // Flag should be cleared after resume attempt
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[test]
    fn test_reset() {
        let controller = MediaPauseController::new();
        controller
            .was_playing_before_recording
            .store(true, Ordering::SeqCst);
        controller.reset();
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[test]
    fn test_multiple_resets_are_safe() {
        let controller = MediaPauseController::new();
        controller.reset();
        controller.reset();
        controller.reset();
        assert!(!controller
            .was_playing_before_recording
            .load(Ordering::SeqCst));
    }

    #[test]
    fn test_was_playing_flag_is_atomic() {
        use std::sync::Arc;
        use std::thread;

        let controller = Arc::new(MediaPauseController::new());
        let mut handles = vec![];

        // Spawn multiple threads toggling the flag
        for i in 0..10 {
            let c = Arc::clone(&controller);
            handles.push(thread::spawn(move || {
                c.was_playing_before_recording
                    .store(i % 2 == 0, Ordering::SeqCst);
                c.was_playing_before_recording.load(Ordering::SeqCst)
            }));
        }

        // All threads should complete without panic
        for handle in handles {
            let _ = handle.join().unwrap();
        }
    }

    #[test]
    fn test_parking_lot_mutex_survives_panic() {
        use parking_lot::Mutex;
        use std::sync::Arc;
        use std::thread;

        let mutex = Arc::new(Mutex::new(Some(String::from("test-value"))));
        let mutex_clone = Arc::clone(&mutex);

        // Spawn a thread that modifies the value, then panics while holding the lock.
        // If the mutex poisoned, the write would be lost and lock() would fail.
        let handle = thread::spawn(move || {
            let mut guard = mutex_clone.lock();
            *guard = Some(String::from("modified-before-panic"));
            panic!("intentional test panic");
        });

        // The thread should have panicked
        let result = handle.join();
        assert!(result.is_err(), "Thread should have panicked");

        // parking_lot::Mutex should NOT be poisoned — we can still lock it,
        // and the write from the panicking thread should be visible.
        let value = mutex.lock().clone();
        assert_eq!(value, Some(String::from("modified-before-panic")));
    }
}
