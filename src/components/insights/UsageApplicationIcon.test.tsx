import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { appColour } from "@/pill/app-icon";
import { UsageApplicationIcon } from "@/components/insights/UsageApplicationIcon";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
it("loads by opaque key and uses the island colour when the image fails", async () => {
  vi.mocked(invoke).mockResolvedValue("data:image/png;base64,aGVsbG8=");
  const {container} = render(<UsageApplicationIcon name="Editor" iconKey="opaque" />);
  expect(screen.getByText("E")).toHaveStyle({ background: appColour("Editor") });
  await waitFor(() => expect(container.querySelector("img")).not.toBeNull());
  expect(invoke).toHaveBeenCalledWith("pill_app_icon", {iconKey:"opaque"});
  fireEvent.error(container.querySelector("img")!);
  expect(screen.getByText("E")).toHaveStyle({ background: appColour("Editor") });
});
