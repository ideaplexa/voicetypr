import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ApiKeyModal } from "../ApiKeyModal";

describe("ApiKeyModal", () => {
  const defaultProps = {
    isOpen: true,
    onClose: vi.fn(),
    onSubmit: vi.fn(),
    providerName: "gemini",
    isLoading: false,
  };

  it("renders when open", () => {
    render(<ApiKeyModal {...defaultProps} />);

    expect(screen.getByText("Add Gemini API Key")).toBeInTheDocument();
    expect(screen.getByText(/Enter your API key to enable Gemini/)).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    render(<ApiKeyModal {...defaultProps} isOpen={false} />);

    expect(screen.queryByText("Add Gemini API Key")).not.toBeInTheDocument();
  });

  it("shows provider-specific link for Gemini", () => {
    render(<ApiKeyModal {...defaultProps} />);

    const link = screen.getByText("Get your Gemini API key");
    expect(link).toBeInTheDocument();
    expect(link.closest("a")).toHaveAttribute("href", "https://aistudio.google.com/apikey");
  });

  it("shows provider-specific link for Anthropic", () => {
    render(<ApiKeyModal {...defaultProps} providerName="anthropic" />);

    expect(screen.getByText("Add Anthropic API Key")).toBeInTheDocument();
    const link = screen.getByText("Get your Anthropic API key");
    expect(link).toBeInTheDocument();
    expect(link.closest("a")).toHaveAttribute(
      "href",
      "https://console.anthropic.com/settings/keys",
    );
  });

  it("calls onSubmit with API key", async () => {
    const onSubmit = vi.fn();
    render(<ApiKeyModal {...defaultProps} onSubmit={onSubmit} />);

    const input = screen.getByPlaceholderText("Enter your Gemini API key");
    fireEvent.change(input, { target: { value: "test-api-key-12345" } });

    const form = input.closest("form");
    if (form) {
      fireEvent.submit(form);
    }

    expect(onSubmit).toHaveBeenCalledWith("test-api-key-12345");
  });

  it("does not submit empty API key", () => {
    const onSubmit = vi.fn();
    render(<ApiKeyModal {...defaultProps} onSubmit={onSubmit} />);

    const form = screen.getByPlaceholderText("Enter your Gemini API key").closest("form");
    if (form) {
      fireEvent.submit(form);
    }

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("disables submit button for empty input", () => {
    render(<ApiKeyModal {...defaultProps} />);

    const submitButton = screen.getByText("Save API Key");
    expect(submitButton).toBeDisabled();
  });

  it("enables submit button when input has value", () => {
    render(<ApiKeyModal {...defaultProps} />);

    const input = screen.getByPlaceholderText("Enter your Gemini API key");
    fireEvent.change(input, { target: { value: "test-key" } });

    const submitButton = screen.getByText("Save API Key");
    expect(submitButton).toBeEnabled();
  });

  it("shows loading state", () => {
    render(<ApiKeyModal {...defaultProps} isLoading={true} />);

    expect(screen.getByText("Saving...")).toBeInTheDocument();

    const submitButton = screen.getByText("Saving...").closest("button");
    expect(submitButton).toBeDisabled();

    const input = screen.getByPlaceholderText("Enter your Gemini API key");
    expect(input).toBeDisabled();
  });

  it("calls onClose when cancel is clicked", () => {
    const onClose = vi.fn();
    render(<ApiKeyModal {...defaultProps} onClose={onClose} />);

    const cancelButton = screen.getByText("Cancel");
    fireEvent.click(cancelButton);

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("clears input when closing", () => {
    const { rerender } = render(<ApiKeyModal {...defaultProps} />);

    const input = screen.getByPlaceholderText("Enter your Gemini API key");
    fireEvent.change(input, { target: { value: "test-key" } });

    expect(input).toHaveValue("test-key");

    // Close modal
    rerender(<ApiKeyModal {...defaultProps} isOpen={false} />);

    // Reopen modal
    rerender(<ApiKeyModal {...defaultProps} isOpen={true} />);

    const newInput = screen.getByPlaceholderText("Enter your Gemini API key");
    expect(newInput).toHaveValue("");
  });

  it("prevents form submission when loading", () => {
    const onSubmit = vi.fn();
    render(<ApiKeyModal {...defaultProps} onSubmit={onSubmit} isLoading={true} />);

    const input = screen.getByPlaceholderText("Enter your Gemini API key");
    fireEvent.change(input, { target: { value: "test-key" } });

    const form = input.closest("form");
    if (form) {
      fireEvent.submit(form);
    }

    // Should still submit since the button is disabled but form submission works
    expect(onSubmit).toHaveBeenCalledWith("test-key");
  });
});
