import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (file: string) => readFileSync(`src-tauri/src/${file}`, "utf8");
const section = (source: string, start: string, end: string) =>
  source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start)));

describe("recording system ownership contracts", () => {
  it("switches microphones through cancellation, not a bare recorder stop", () => {
    const source = section(
      read("commands/settings.rs"),
      "pub async fn set_audio_device",
      "pub async fn get_autostart_status",
    );
    expect(source).toContain("cancel_recording(app.clone()).await?");
    expect(source).not.toContain("recorder.stop_recording()");
    const command = section(
      read("commands/audio.rs"),
      "pub async fn cancel_recording",
      "pub async fn delete_transcription_entry",
    );
    expect(command).toContain("cancel_cleanup::cancel(app).await");
    const cancel = read("commands/audio_cancel.rs");
    expect(cancel).toMatch(/current_recording_path[\s\S]*?\.take\(cancel_generation\)/);
    expect(cancel).toMatch(/capture_files[\s\S]*?\.take\(cancel_generation\)/);
    expect(cancel).toContain("files.writer_stopped(");
    expect(cancel).toContain("cancel_generation(&app, cancel_generation)");
    expect(cancel).not.toContain("discard_generation(&app, cancel_generation)");
    expect(cancel).not.toContain("stop_recording_with_post_roll");
    expect(read("commands/audio.rs")).toContain(
      "StopInFlightGuard::try_acquire(app_state.stop_in_flight.clone())",
    );
  });

  it("invalidates normalized config on command select/deselect/removal and tray source changes", () => {
    const remote = read("commands/remote.rs");
    for (const [start, end] of [
      ["pub async fn remove_remote_server", "fn apply_server_update"],
      ["pub async fn set_active_remote_server", "pub async fn get_active_remote_server"],
    ]) {
      expect(section(remote, start, end)).toContain(
        "invalidate_recording_config_cache(&app).await",
      );
    }
    const tray = section(
      read("commands/settings.rs"),
      "pub async fn set_model_from_tray",
      "pub async fn set_audio_device",
    );
    expect(tray.match(/invalidate_recording_config_cache\(&app\)\.await/g)).toHaveLength(2);
  });
});
