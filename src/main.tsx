import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./globals.css";
import { AppErrorBoundary } from "./components/ErrorBoundary";
import { createLogger } from "@/lib/logger";
import { invoke } from "@tauri-apps/api/core";

const log = createLogger("app");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary
      onError={(error, errorInfo) => {
        log.error("Root error boundary caught:", error, errorInfo);
        // Forward to opt-in diagnostics; the backend scrubs + gates (no-op when off).
        const err = error as Error;
        void invoke("report_frontend_error", {
          name: err?.name,
          message: err?.message ?? String(error),
        }).catch(() => {});
      }}
    >
      <App />
    </AppErrorBoundary>
  </React.StrictMode>,
);
