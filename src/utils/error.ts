export function getErrorMessage(error: unknown, fallback?: string): string {
  if (typeof error === "string") {
    return error || fallback || "An unexpected error occurred";
  }

  if (error instanceof Error) {
    return error.message || fallback || "An unexpected error occurred";
  }

  try {
    const serialized = JSON.stringify(error);
    if (serialized && serialized !== "{}") {
      return serialized;
    }
  } catch {
    // ignore JSON serialization errors
  }

  return fallback || "An unexpected error occurred";
}
