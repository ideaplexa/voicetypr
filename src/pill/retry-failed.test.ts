import { afterEach, expect, it } from "vitest";
import { pillHarness } from "@/pill/test-harness";
import { recoveryCard } from "@/pill/feedback";
import type { DictationRecovery } from "@/types/island-events";

const failed: DictationRecovery = { generation: 2, id: "clip", kind: "cloud_failed", engine_short: "Soniox", alt_engine_short: "Whisper Small", alt_engine_id: "small.en", expires_in_ms: 600000, retry_ready: true, retry_failed: true };
let h: ReturnType<typeof pillHarness>;
afterEach(() => h?.destroy());

it("names the failed model and only offers another model when available", () => {
  const card = recoveryCard(failed, 0);
  expect(card.title).toBe("Soniox couldn't transcribe this");
  expect(card.closeLabel).toBe("Close, recording kept");
  expect(card.actions.map(a => a.label)).toEqual(["Retry again", "Try another model", "Delete audio"]);
  expect(recoveryCard({ ...failed, alt_engine_short: null, alt_engine_id: null }, 0).actions.map(a => a.label)).toEqual(["Retry again", "Delete audio"]);
});

it.each(["retry", "alternative", "delete", "close"])("routes the failed card action without making Close delete: %s", async action => {
  h = pillHarness(); await h.settle(0);
  h.emit("dictation-recovery", failed); await h.settle(1500);
  const selector = action === "close" ? ".card-dismiss" : `.act:nth-child(${action === "retry" ? 1 : action === "alternative" ? 2 : 3})`;
  h.get(`[data-key="recovery:clip"] ${selector}`).click(); await h.settle(500);
  if (action === "retry") expect(h.invoke).toHaveBeenCalledWith("retry_kept_dictation", { id: "clip", engine: null });
  else if (action === "alternative") expect(h.invoke).toHaveBeenCalledWith("retry_kept_dictation", { id: "clip", engine: "small.en" });
  else if (action === "delete") expect(h.invoke).toHaveBeenCalledWith("discard_kept_dictation", { id: "clip" });
  else expect(h.invoke).not.toHaveBeenCalledWith("discard_kept_dictation", { id: "clip" });
});

it.each([true, false])("keeps actionable recovery on top regardless of event versus rejection ordering: event first=%s", async eventFirst => {
  h = pillHarness(); await h.settle(0);
  const base = h.invoke.getMockImplementation()!;
  h.invoke.mockImplementation(async command => {
    if (command === "retry_kept_dictation") {
      if (eventFirst) h.emit("dictation-recovery", failed);
      throw "Retry failed; recording remains available until expiry";
    }
    return base(command);
  });
  h.emit("dictation-recovery", { ...failed, generation: 1, retry_failed: false }); await h.settle(1500);
  h.get('[data-key="recovery:clip"] .act').click(); await h.settle(500);
  if (!eventFirst) h.emit("dictation-recovery", failed);
  await h.settle(1000);
  expect(h.get('[data-key="recovery:clip"]').hidden).toBe(false);
  expect(h.get('[data-key="recovery:clip"]').inert).toBe(false);
  expect(h.root.querySelector('[data-key="retry-error"]:not([hidden])')).toBeNull();
});

it.each([
  ["Dictation is busy", "Finish the current dictation, then retry."],
  ["Requested local engine is not ready", "That engine is no longer ready."],
])("still explains an unaccepted Retry again when the old failed card has not changed: %s", async (error, copy) => {
  h = pillHarness(); await h.settle(0);
  const base = h.invoke.getMockImplementation()!;
  h.invoke.mockImplementation(async command => {
    if (command === "retry_kept_dictation") throw error;
    return base(command);
  });
  h.emit("dictation-recovery", failed); await h.settle(1500);
  h.get('[data-key="recovery:clip"] .act').click(); await h.settle(1000);
  expect(h.get('[data-key="retry-error"]').textContent).toContain(copy);
});

it("withdraws an unavailable alternative without hiding its explanation", async () => {
  h = pillHarness(); await h.settle(0);
  const base = h.invoke.getMockImplementation()!;
  h.invoke.mockImplementation(async command => {
    if (command === "retry_kept_dictation") throw "Requested local engine is not ready";
    return base(command);
  });
  h.emit("dictation-recovery", failed); await h.settle(1500);
  h.get('[data-key="recovery:clip"] .act:nth-child(2)').click(); await h.settle(1000);
  expect([...h.root.querySelectorAll('[data-key="recovery:clip"] .act')].map(button => button.textContent))
    .toEqual(["Retry again", "Delete audio"]);
  expect(h.get('[data-key="retry-error"]').textContent).toContain("That engine is no longer ready.");
});
