import { useMemo, useState } from "react";
import { Button } from "./ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "./ui/dialog";
import { Input } from "./ui/input";
import { Label } from "./ui/label";
import { Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";

interface OpenAICompatConfigModalProps {
  isOpen: boolean;
  defaultBaseUrl?: string;
  defaultModel?: string;
  onClose: () => void;
  onSubmit: (args: { baseUrl: string; model: string; apiKey?: string }) => void;
}

export function OpenAICompatConfigModal({
  isOpen,
  defaultBaseUrl = "https://api.openai.com/v1",
  defaultModel = "",
  onClose,
  onSubmit,
}: OpenAICompatConfigModalProps) {
  const [baseUrl, setBaseUrl] = useState(defaultBaseUrl);
  const [model, setModel] = useState(defaultModel);
  const [apiKey, setApiKey] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<null | { ok: boolean; message: string }>(null);
  const [testedPayload, setTestedPayload] = useState<null | {
    baseUrl: string;
    model: string;
    apiKey: string;
  }>(null);
  const testOk = testResult?.ok === true;
  const inputsMatchTest = useMemo(() => {
    if (!testedPayload) return false;
    return (
      testedPayload.baseUrl === baseUrl.trim() &&
      testedPayload.model === model.trim() &&
      testedPayload.apiKey === apiKey.trim()
    );
  }, [testedPayload, baseUrl, model, apiKey]);

  // Sync form state when the dialog (re)opens or defaults change while open —
  // adjusted during render so no post-paint flash; Base UI keeps its close
  // animation because the component stays mounted.
  const [appliedOpenState, setAppliedOpenState] = useState<string | null>(null);
  const openStateKey = isOpen ? `open\u0000${defaultBaseUrl}\u0000${defaultModel}` : "closed";
  if (appliedOpenState !== openStateKey) {
    setAppliedOpenState(openStateKey);
    if (isOpen) {
      setBaseUrl(defaultBaseUrl);
      setModel(defaultModel);
      setTestedPayload(null);
      setTestResult(null);
    } else {
      setApiKey("");
      setSubmitting(false);
      setTesting(false);
      setTestResult(null);
      setTestedPayload(null);
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!baseUrl.trim() || !model.trim()) return;
    try {
      setSubmitting(true);
      onSubmit({
        baseUrl: baseUrl.trim(),
        model: model.trim(),
        apiKey: apiKey.trim() || undefined,
      });
    } finally {
      // Keep spinner controlled by parent isLoading if needed; here we reset
      setSubmitting(false);
    }
  };

  const handleTest = async () => {
    setTestResult(null);
    setTesting(true);
    const trimmedBase = baseUrl.trim();
    const trimmedModel = model.trim();
    const trimmedKey = apiKey.trim();
    try {
      const noAuth = !trimmedKey;
      await invoke("test_openai_endpoint", {
        baseUrl: trimmedBase,
        model: trimmedModel,
        apiKey: trimmedKey || undefined,
        noAuth,
      });
      setTestResult({ ok: true, message: "Connection successful" });
      setTestedPayload({ baseUrl: trimmedBase, model: trimmedModel, apiKey: trimmedKey });
    } catch (e: unknown) {
      setTestResult({ ok: false, message: String(e) });
      setTestedPayload({ baseUrl: trimmedBase, model: trimmedModel, apiKey: trimmedKey });
    } finally {
      setTesting(false);
    }
  };

  // Test-result freshness is derived (inputsMatchTest) — no reset effect.

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="sm:max-w-[520px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Configure OpenAI-Compatible Provider</DialogTitle>
            <DialogDescription>
              Set the API base URL, model ID, and optional API key for any OpenAI-compatible
              endpoint.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="baseUrl">API Base URL</Label>
              <Input
                id="baseUrl"
                placeholder="https://api.openai.com/v1"
                value={baseUrl}
                onChange={(e) => setBaseUrl(e.target.value)}
              />
              <p className="text-xs text-muted-foreground">
                Examples: https://api.openai.com/v1, http://localhost:11434/v1
              </p>
            </div>

            <div className="grid gap-2">
              <Label htmlFor="model">Model ID</Label>
              <Input
                id="model"
                placeholder="e.g. gpt-5-nano, gpt-5-mini"
                value={model}
                onChange={(e) => setModel(e.target.value)}
              />
            </div>

            <div className="grid gap-2">
              <Label htmlFor="apiKey">API Key</Label>
              <Input
                id="apiKey"
                type="password"
                placeholder="Leave empty for no authentication"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
              />
            </div>
          </div>

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
              disabled={submitting || testing}
            >
              Cancel
            </Button>
            <Button
              type="button"
              variant="outline"
              onClick={handleTest}
              disabled={!baseUrl.trim() || !model.trim() || submitting || testing}
            >
              {testing ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Testing...
                </>
              ) : (
                "Test"
              )}
            </Button>
            <Button
              type="submit"
              disabled={
                !baseUrl.trim() || !model.trim() || submitting || !testOk || !inputsMatchTest
              }
              title={!testOk || !inputsMatchTest ? "Run Test and pass before saving" : undefined}
            >
              {submitting ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Saving...
                </>
              ) : (
                "Save"
              )}
            </Button>
          </DialogFooter>
          {testResult && inputsMatchTest && (
            <div className={`mt-2 text-sm ${testResult.ok ? "text-green-600" : "text-red-600"}`}>
              {testResult.message}
            </div>
          )}
        </form>
      </DialogContent>
    </Dialog>
  );
}
