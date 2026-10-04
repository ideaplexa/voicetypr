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

it.each([null, new Error('unavailable')])('retries a failed lookup only on refresh after backoff (%s)', async failure => {
  vi.mocked(invoke).mockReset();
  if (failure instanceof Error) vi.mocked(invoke).mockRejectedValueOnce(failure);
  else vi.mocked(invoke).mockResolvedValueOnce(failure);
  vi.mocked(invoke).mockResolvedValue('data:image/png;base64,aGVsbG8=');
  const now = vi.spyOn(Date, 'now').mockReturnValue(0);
  const {container, rerender} = render(<UsageApplicationIcon name="Editor" iconKey="opaque" refreshToken={0} />);
  await waitFor(() => expect(invoke).toHaveBeenCalledTimes(1));
  rerender(<UsageApplicationIcon name="Editor renamed" iconKey="opaque" refreshToken={0} />);
  rerender(<UsageApplicationIcon name="Editor renamed" iconKey="opaque" refreshToken={1} />);
  expect(invoke).toHaveBeenCalledTimes(1);
  now.mockReturnValue(1000);
  rerender(<UsageApplicationIcon name="Editor renamed" iconKey="opaque" refreshToken={1} />);
  expect(invoke).toHaveBeenCalledTimes(1);
  rerender(<UsageApplicationIcon name="Editor renamed" iconKey="opaque" refreshToken={2} />);
  await waitFor(() => expect(container.querySelector('img')).not.toBeNull());
  expect(invoke).toHaveBeenCalledTimes(2);
  rerender(<UsageApplicationIcon name="Editor renamed" iconKey="opaque" refreshToken={3} />);
  expect(invoke).toHaveBeenCalledTimes(2);
  now.mockRestore();
});
