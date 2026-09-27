import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  AppErrorBoundary,
  RecordingErrorBoundary,
  SettingsErrorBoundary,
  ModelManagementErrorBoundary,
} from "./ErrorBoundary";

// Component that throws an error
const ThrowError = ({ shouldThrow }: { shouldThrow: boolean }) => {
  if (shouldThrow) {
    throw new Error("Test error message");
  }
  return <div>No error</div>;
};

describe("ErrorBoundary Components", () => {
  const consoleErrorSpy = vi.spyOn(console, "error");

  beforeEach(() => {
    vi.clearAllMocks();
    // Suppress console.error for error boundary tests
    consoleErrorSpy.mockImplementation(() => {});
  });

  afterEach(() => {
    consoleErrorSpy.mockRestore();
  });

  describe("AppErrorBoundary", () => {
    it("should render children when no error occurs", () => {
      render(
        <AppErrorBoundary>
          <div>Test content</div>
        </AppErrorBoundary>,
      );

      expect(screen.getByText("Test content")).toBeInTheDocument();
    });

    it("should show default error fallback when error occurs", () => {
      render(
        <AppErrorBoundary>
          <ThrowError shouldThrow={true} />
        </AppErrorBoundary>,
      );

      expect(screen.getByText("Something went wrong")).toBeInTheDocument();
      expect(screen.getByText("Test error message")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /try again/i })).toBeInTheDocument();
    });

    it("should use custom fallback component when provided", () => {
      const CustomFallback = ({ error, resetErrorBoundary }: any) => (
        <div>
          <h1>Custom Error</h1>
          <p>{error.message}</p>
          <button onClick={resetErrorBoundary}>Custom Reset</button>
        </div>
      );

      render(
        <AppErrorBoundary fallback={CustomFallback}>
          <ThrowError shouldThrow={true} />
        </AppErrorBoundary>,
      );

      expect(screen.getByText("Custom Error")).toBeInTheDocument();
      expect(screen.getByText("Test error message")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Custom Reset" })).toBeInTheDocument();
    });

    it("should call onError callback when error occurs", () => {
      const onError = vi.fn();

      render(
        <AppErrorBoundary onError={onError}>
          <ThrowError shouldThrow={true} />
        </AppErrorBoundary>,
      );

      expect(onError).toHaveBeenCalledWith(
        expect.objectContaining({ message: "Test error message" }),
        expect.any(Object),
      );
    });

    it("should reset error boundary when try again is clicked", async () => {
      const user = userEvent.setup();
      let throwError = true;

      const TestComponent = () => {
        return <ThrowError shouldThrow={throwError} />;
      };

      const { rerender } = render(
        <AppErrorBoundary>
          <TestComponent />
        </AppErrorBoundary>,
      );

      // Should show error
      expect(screen.getByText("Something went wrong")).toBeInTheDocument();

      // Fix the error condition
      throwError = false;

      // Click try again
      const tryAgainButton = screen.getByRole("button", { name: /try again/i });
      await user.click(tryAgainButton);

      // Rerender to show the fixed component
      rerender(
        <AppErrorBoundary>
          <TestComponent />
        </AppErrorBoundary>,
      );

      // Should show normal content
      expect(screen.getByText("No error")).toBeInTheDocument();
    });

    it("should call onReset callback when provided", async () => {
      const user = userEvent.setup();
      const onReset = vi.fn();

      render(
        <AppErrorBoundary onReset={onReset}>
          <ThrowError shouldThrow={true} />
        </AppErrorBoundary>,
      );

      const tryAgainButton = screen.getByRole("button", { name: /try again/i });
      await user.click(tryAgainButton);

      expect(onReset).toHaveBeenCalled();
    });

    it("should handle missing error message gracefully", () => {
      const ThrowErrorWithoutMessage = () => {
        throw new Error("");
      };

      render(
        <AppErrorBoundary>
          <ThrowErrorWithoutMessage />
        </AppErrorBoundary>,
      );

      expect(screen.getByText("Something went wrong")).toBeInTheDocument();
      expect(screen.getByText("An unexpected error occurred")).toBeInTheDocument();
    });

    it("should log errors to console when no onError provided", () => {
      // Create a fresh spy for this specific test
      const localConsoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      render(
        <AppErrorBoundary>
          <ThrowError shouldThrow={true} />
        </AppErrorBoundary>,
      );

      expect(localConsoleErrorSpy).toHaveBeenCalledWith(
        "Error caught by boundary:",
        expect.objectContaining({ message: "Test error message" }),
        expect.any(Object),
      );

      localConsoleErrorSpy.mockRestore();
    });
  });

  describe("RecordingErrorBoundary", () => {
    it("should render children normally", () => {
      render(
        <RecordingErrorBoundary>
          <div>Recording UI</div>
        </RecordingErrorBoundary>,
      );

      expect(screen.getByText("Recording UI")).toBeInTheDocument();
    });

    it("should show error fallback and log recording errors", () => {
      const localConsoleErrorSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      render(
        <RecordingErrorBoundary>
          <ThrowError shouldThrow={true} />
        </RecordingErrorBoundary>,
      );

      expect(screen.getByText("Something went wrong")).toBeInTheDocument();
      expect(localConsoleErrorSpy).toHaveBeenCalledWith(
        "Recording error:",
        expect.objectContaining({ message: "Test error message" }),
      );

      localConsoleErrorSpy.mockRestore();
    });
  });

  describe("SettingsErrorBoundary", () => {
    it("should render children normally", () => {
      render(
        <SettingsErrorBoundary>
          <div>Settings UI</div>
        </SettingsErrorBoundary>,
      );

      expect(screen.getByText("Settings UI")).toBeInTheDocument();
    });

    it("should show custom settings error fallback", () => {
      render(
        <SettingsErrorBoundary>
          <ThrowError shouldThrow={true} />
        </SettingsErrorBoundary>,
      );

      expect(screen.getByText(/Failed to load settings:/)).toBeInTheDocument();
      // The error message is part of the same text node as "Failed to load settings:"
      expect(screen.getByText(/Failed to load settings: Test error message/)).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    });

    it("should have styled error container", () => {
      render(
        <SettingsErrorBoundary>
          <ThrowError shouldThrow={true} />
        </SettingsErrorBoundary>,
      );

      const errorContainer = screen.getByText(/Failed to load settings:/).parentElement;
      expect(errorContainer).toHaveClass(
        "p-4",
        "border",
        "border-destructive/20",
        "rounded-lg",
        "bg-destructive/5",
      );
    });
  });

  describe("ModelManagementErrorBoundary", () => {
    it("should render children normally", () => {
      render(
        <ModelManagementErrorBoundary>
          <div>Model Management UI</div>
        </ModelManagementErrorBoundary>,
      );

      expect(screen.getByText("Model Management UI")).toBeInTheDocument();
    });

    it("should show custom model error fallback", () => {
      render(
        <ModelManagementErrorBoundary>
          <ThrowError shouldThrow={true} />
        </ModelManagementErrorBoundary>,
      );

      expect(screen.getByText("Error loading models")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Reload Models" })).toBeInTheDocument();
    });

    it("should reset when reload button is clicked", async () => {
      const user = userEvent.setup();
      let throwError = true;

      const TestComponent = () => {
        return <ThrowError shouldThrow={throwError} />;
      };

      const { rerender } = render(
        <ModelManagementErrorBoundary>
          <TestComponent />
        </ModelManagementErrorBoundary>,
      );

      // Should show error
      expect(screen.getByText("Error loading models")).toBeInTheDocument();

      // Fix the error condition
      throwError = false;

      // Click reload
      const reloadButton = screen.getByRole("button", { name: "Reload Models" });
      await user.click(reloadButton);

      // Rerender
      rerender(
        <ModelManagementErrorBoundary>
          <TestComponent />
        </ModelManagementErrorBoundary>,
      );

      // Should show normal content
      expect(screen.getByText("No error")).toBeInTheDocument();
    });
  });

  describe("Error Boundary Integration", () => {
    it("should handle nested error boundaries", () => {
      render(
        <AppErrorBoundary>
          <div>
            <h1>App Level</h1>
            <RecordingErrorBoundary>
              <ThrowError shouldThrow={true} />
            </RecordingErrorBoundary>
            <div>Other app content</div>
          </div>
        </AppErrorBoundary>,
      );

      // Inner boundary should catch the error
      expect(screen.getByText("Something went wrong")).toBeInTheDocument();
      // Outer content should still render
      expect(screen.getByText("App Level")).toBeInTheDocument();
      expect(screen.getByText("Other app content")).toBeInTheDocument();
    });

    it("should isolate errors between boundaries", () => {
      render(
        <div>
          <RecordingErrorBoundary>
            <ThrowError shouldThrow={true} />
          </RecordingErrorBoundary>
          <SettingsErrorBoundary>
            <div>Settings are fine</div>
          </SettingsErrorBoundary>
        </div>,
      );

      // Recording error should be shown
      expect(screen.getByText("Something went wrong")).toBeInTheDocument();
      // Settings should render normally
      expect(screen.getByText("Settings are fine")).toBeInTheDocument();
    });

    it("should handle errors during event handlers", async () => {
      const user = userEvent.setup();

      const ButtonWithError = () => {
        const [clicked, setClicked] = React.useState(false);

        if (clicked) {
          throw new Error("Error in render after click");
        }

        return <button onClick={() => setClicked(true)}>Click me</button>;
      };

      render(
        <AppErrorBoundary>
          <ButtonWithError />
        </AppErrorBoundary>,
      );

      const button = screen.getByRole("button", { name: "Click me" });
      await user.click(button);

      expect(screen.getByText("Something went wrong")).toBeInTheDocument();
      expect(screen.getByText("Error in render after click")).toBeInTheDocument();
    });
  });
});
