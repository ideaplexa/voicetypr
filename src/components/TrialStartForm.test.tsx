import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { TrialStartForm } from "@/components/TrialStartForm";
import type { TrialStartError } from "@/types";

function setup(onStart = vi.fn(async (): Promise<TrialStartError | null> => null)) {
  render(<TrialStartForm onStart={onStart} />);
  return {
    onStart,
    user: userEvent.setup(),
    name: screen.getByLabelText("Name"),
    email: screen.getByLabelText("Email"),
    start: screen.getByRole("button", { name: "Start 7-day trial" }),
  };
}

describe("TrialStartForm", () => {
  it("asks for both fields and starts nothing while they are empty", async () => {
    const { user, start, onStart, name } = setup();

    await user.click(start);

    expect(screen.getByText("Enter your name")).toBeInTheDocument();
    expect(screen.getByText("Enter your email")).toBeInTheDocument();
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(name).toHaveFocus();
    expect(onStart).not.toHaveBeenCalled();
  });

  it("refuses an email that is not an address", async () => {
    const { user, start, onStart, name, email } = setup();

    await user.type(name, "Priya Raman");
    await user.type(email, "priya@example");
    await user.click(start);

    expect(screen.getByText("That email doesn’t look right")).toBeInTheDocument();
    expect(email).toHaveFocus();
    expect(onStart).not.toHaveBeenCalled();
  });

  it("starts the trial once with the trimmed name and email, also from Enter", async () => {
    const { user, onStart, name, email } = setup();

    await user.type(name, "  Priya Raman ");
    await user.type(email, " priya@example.com {Enter}");

    expect(onStart).toHaveBeenCalledTimes(1);
    expect(onStart).toHaveBeenCalledWith("Priya Raman", "priya@example.com");
  });

  it("does not start twice when the button is pressed again while starting", async () => {
    let finish: (value: TrialStartError | null) => void = () => {};
    const onStart = vi.fn(() => new Promise<TrialStartError | null>((resolve) => { finish = resolve; }));
    const { user, start, name, email } = setup(onStart);
    await user.type(name, "Priya Raman");
    await user.type(email, "priya@example.com");

    await user.click(start);
    await user.click(start);
    await user.type(email, "{Enter}");
    finish(null);

    expect(onStart).toHaveBeenCalledTimes(1);
  });

  it("says the server could not be reached, never that the trial ended", async () => {
    const { user, start, name, email } = setup(vi.fn(async () => "trial_unreachable" as const));
    await user.type(name, "Priya Raman");
    await user.type(email, "priya@example.com");

    await user.click(start);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Couldn’t reach Voicetypr, so your trial hasn’t started. Check your connection and try again.");
    expect(document.body).not.toHaveTextContent(/expired|ended/i);
    expect(start).toBeEnabled();
    expect(name).toHaveValue("Priya Raman");
  });

  it("shows the server's rejection of the email on the email field", async () => {
    const { user, start, name, email } = setup(vi.fn(async () => "trial_invalid_email" as const));
    await user.type(name, "Priya Raman");
    await user.type(email, "priya@example.com");

    await user.click(start);

    expect(await screen.findByText("That email doesn’t look right")).toBeInTheDocument();
    expect(email).toHaveAttribute("aria-invalid", "true");
  });

  it("tells the user to wait when the server is rate limiting", async () => {
    const { user, start, name, email } = setup(vi.fn(async () => "trial_rate_limited" as const));
    await user.type(name, "Priya Raman");
    await user.type(email, "priya@example.com");

    await user.click(start);

    expect(await screen.findByRole("alert")).toHaveTextContent("Wait a few minutes, then try again.");
  });
});
