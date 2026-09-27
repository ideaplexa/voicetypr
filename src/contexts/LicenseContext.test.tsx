import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LicenseStatus } from "@/types";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
}));

import { invoke } from "@tauri-apps/api/core";
import { LicenseProvider, useLicense } from "./LicenseContext";
import { AccountSection } from "@/components/sections/AccountSection";

const SECRET_KEY = "SUPER-SECRET-LICENSE-KEY-XYZ";

const licensedStatus: LicenseStatus = {
  status: "licensed",
  trial_days_left: undefined,
  license_type: "lifetime",
  license_key: SECRET_KEY,
  expires_at: "2027-01-01T00:00:00Z",
};

function createDeferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function LoadingProbe() {
  const { isLoading, revalidateLicense, deactivateLicense } = useLicense();
  return (
    <>
      <span data-testid="license-loading">{String(isLoading)}</span>
      <button type="button" onClick={deactivateLicense}>
        Deactivate
      </button>
      <button type="button" onClick={revalidateLicense}>
        Revalidate
      </button>
    </>
  );
}

function Probe() {
  useLicense();
  return null;
}

function RevalidationProbe() {
  const { status, revalidateLicense } = useLicense();
  return (
    <>
      <span>{status?.verification_state ?? "none"}</span>
      <button type="button" onClick={revalidateLicense}>
        Revalidate
      </button>
    </>
  );
}

describe("LicenseContext", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("allows re-entering an existing key after an unreadable saved license without a reset", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "check_license_status") {
        throw new Error(
          "Your saved license could not be read. The saved entry has not been deleted.",
        );
      }
      if (command === "activate_license") return licensedStatus;
      throw new Error(`Unexpected command: ${command}`);
    });

    render(
      <LicenseProvider>
        <AccountSection />
      </LicenseProvider>,
    );

    expect(await screen.findByText("Couldn’t load license status")).toBeInTheDocument();
    expect(screen.queryByText("Trial Expired")).not.toBeInTheDocument();
    expect(screen.queryByText("Loading...")).not.toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("Enter license key"), {
      target: { value: SECRET_KEY },
    });
    fireEvent.click(screen.getByRole("button", { name: "Activate" }));

    expect(await screen.findByText("Pro Licensed")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("activate_license", { licenseKey: SECRET_KEY });
    expect(screen.queryByText("Couldn’t load license status")).not.toBeInTheDocument();
    expect(screen.queryByPlaceholderText("Enter license key")).not.toBeInTheDocument();
    expect(
      vi
        .mocked(invoke)
        .mock.calls.every(
          ([command]) => command === "check_license_status" || command === "activate_license",
        ),
    ).toBe(true);
  });

  it("never logs the license key when checking status", async () => {
    vi.mocked(invoke).mockResolvedValue(licensedStatus);

    const debugSpy = vi.spyOn(console, "debug").mockImplementation(() => {});

    render(
      <LicenseProvider>
        <Probe />
      </LicenseProvider>,
    );

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("check_license_status"));

    // Give the awaited resolution + the post-invoke debug log a tick to flush.
    await waitFor(() =>
      expect(
        debugSpy.mock.calls.some((c) => JSON.stringify(c).includes("License status received")),
      ).toBe(true),
    );

    for (const call of debugSpy.mock.calls) {
      expect(JSON.stringify(call)).not.toContain(SECRET_KEY);
    }

    // Sanity: the non-secret status fields ARE still logged (proves the check
    // ran and that we redacted rather than silencing the log entirely).
    const receivedLog = debugSpy.mock.calls.find((c) =>
      JSON.stringify(c).includes("License status received"),
    );
    expect(receivedLog).toBeDefined();
    const serialized = JSON.stringify(receivedLog);
    expect(serialized).toContain('"status":"licensed"');
    expect(serialized).toContain('"license_type":"lifetime"');
  });

  it("revalidates explicitly and replaces offline-grace metadata", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "revalidate_license") {
        return { ...licensedStatus, verification_state: "verified" };
      }
      return { ...licensedStatus, verification_state: "offline_grace" };
    });

    render(
      <LicenseProvider>
        <RevalidationProbe />
      </LicenseProvider>,
    );

    expect(await screen.findByText("offline_grace")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Revalidate" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("revalidate_license"));
    expect(await screen.findByText("verified")).toBeInTheDocument();
  });

  it("keeps loading while stale revalidation completes after deactivation", async () => {
    const initialStatus: LicenseStatus = { ...licensedStatus, verification_state: "offline_grace" };
    const deactivateRequest = createDeferred<undefined>();
    const staleRevalidationRequest = createDeferred<LicenseStatus>();
    const latestCheckRequest = createDeferred<LicenseStatus>();
    let statusCheckCount = 0;

    vi.mocked(invoke).mockImplementation((command) => {
      if (command === "check_license_status") {
        statusCheckCount += 1;
        return statusCheckCount === 1 ? Promise.resolve(initialStatus) : latestCheckRequest.promise;
      }
      if (command === "deactivate_license") return deactivateRequest.promise;
      if (command === "revalidate_license") return staleRevalidationRequest.promise;
      throw new Error(`Unexpected command: ${command}`);
    });

    render(
      <LicenseProvider>
        <LoadingProbe />
      </LicenseProvider>,
    );

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("check_license_status"));
    await waitFor(() => expect(screen.getByTestId("license-loading").textContent).toBe("false"));

    fireEvent.click(screen.getByRole("button", { name: "Deactivate" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("deactivate_license"));

    fireEvent.click(screen.getByRole("button", { name: "Revalidate" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("revalidate_license"));
    await waitFor(() => expect(screen.getByTestId("license-loading").textContent).toBe("true"));

    deactivateRequest.resolve(undefined);
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(([command]) => command === "check_license_status"),
      ).toHaveLength(2),
    );

    staleRevalidationRequest.resolve({ ...licensedStatus, verification_state: "verified" });
    await waitFor(() => expect(screen.getByTestId("license-loading").textContent).toBe("true"));

    latestCheckRequest.resolve({ ...licensedStatus, verification_state: "verified" });
    await waitFor(() => expect(screen.getByTestId("license-loading").textContent).toBe("false"));
  });
});
