import { describe, expect, it } from "vitest";
import { homeNeeds, homeTips } from "./home-needs";

const ready = { label: "Ready", ready: true };
const none = { kept: [], updateVersion: null, status: ready, accessibilityMissing: false };

describe("homeNeeds", () => {
  it("is empty when nothing needs the user", () => {
    expect(homeNeeds(none)).toEqual([]);
  });
  it("does not treat a model download as a problem", () => {
    expect(homeNeeds({ ...none, status: { label: "Downloading", ready: false, pending: true, screen: "transcription" } })).toEqual([]);
  });
  it("orders kept recordings, update, then setup problems", () => {
    const rows = homeNeeds({
      kept: [{ id: "a", engine_short: "Soniox", alt_engine_short: null, alt_engine_id: null, remaining_ms: 61_000, busy: false }],
      updateVersion: "2.1.0",
      status: { label: "No model yet", ready: false, screen: "transcription" },
      accessibilityMissing: true,
    });
    expect(rows.map((row) => row.id)).toEqual(["kept:a", "update", "source", "accessibility"]);
    expect(rows[0].description).toContain("2 more minutes");
    expect(rows[0].action).toEqual({ kind: "retry", id: "a", engine: null });
  });
  it("disables the update row while installing and offers Restart once installed", () => {
    const [installing] = homeNeeds({ ...none, updateVersion: "2.1.0", updateInstall: "installing" });
    expect(installing).toMatchObject({ button: "Installing…", disabled: true, action: { kind: "install" } });
    const [prompting] = homeNeeds({ ...none, updateVersion: "2.1.0", updateInstall: "prompting" });
    expect(prompting.disabled).toBe(true);
    const [pending] = homeNeeds({ ...none, updateVersion: "2.1.0", updateInstall: "restart_pending" });
    expect(pending).toMatchObject({ button: "Restart", action: { kind: "restart" } });
    expect(pending.disabled).toBeUndefined();
  });
  it("disables Retry while a retry is running", () => {
    const [row] = homeNeeds({ ...none, kept: [{ id: "a", engine_short: "X", alt_engine_short: null, alt_engine_id: null, remaining_ms: 1000, busy: true }] });
    expect(row.disabled).toBe(true);
    expect(row.discard).toEqual({ kind: "discard", id: "a" });
  });
  it("keeps Discard available while audio is finalizing, but not Retry", () => {
    const [row] = homeNeeds({ ...none, kept: [{ id: "a", engine_short: "Parakeet", alt_engine_short: "Parakeet", alt_engine_id: "v3", remaining_ms: 1000, busy: false, retry_ready: false }] });
    expect(row).toMatchObject({ button: "Finishing…", disabled: true, discard: { kind: "discard", id: "a" } });
  });
});

describe("homeTips", () => {
  it("shows each tip only when it applies", () => {
    expect(homeTips({ polishOn: false, dictionaryEmpty: true }).map((tip) => tip.id)).toEqual(["polish", "dictionary"]);
    expect(homeTips({ polishOn: true, dictionaryEmpty: true }).map((tip) => tip.id)).toEqual(["dictionary"]);
    expect(homeTips({ polishOn: false, dictionaryEmpty: false }).map((tip) => tip.id)).toEqual(["polish"]);
    expect(homeTips({ polishOn: true, dictionaryEmpty: false })).toEqual([]);
  });
  it("waits for unknown state instead of guessing", () => {
    expect(homeTips({ polishOn: null, dictionaryEmpty: null })).toEqual([]);
  });
});
