import { expect, it } from "vitest";
import { releasePageUrl } from "@/lib/releaseNotes";

it.each(["2.1.0-beta.6", "v2.1.0-beta.6"])("normalizes the release tag for %s", (version) => {
  expect(releasePageUrl(version)).toBe("https://github.com/ideaplexa/voicetypr/releases/tag/v2.1.0-beta.6");
});

it("keeps a malformed version inside the release tag path", () => {
  expect(releasePageUrl("v2.1/../../issues?test#x")).toBe(
    "https://github.com/ideaplexa/voicetypr/releases/tag/v2.1%2F..%2F..%2Fissues%3Ftest%23x",
  );
});
