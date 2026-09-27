import { ExternalLink, Loader2 } from "lucide-react";
import React, { useState } from "react";
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

interface ApiKeyModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSubmit: (apiKey: string) => void;
  providerName: string;
  isLoading?: boolean;
  title?: string;
  description?: string;
  submitLabel?: string;
  docsUrl?: string;
}

export function ApiKeyModal({
  isOpen,
  onClose,
  onSubmit,
  providerName,
  isLoading = false,
  title,
  description,
  submitLabel = "Save API Key",
  docsUrl,
}: ApiKeyModalProps) {
  const [apiKey, setApiKey] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (apiKey.trim()) {
      onSubmit(apiKey);
    }
  };

  const handleClose = () => {
    setApiKey("");
    onClose();
  };

  // Clear input whenever the modal closes — adjusted during render so Esc and
  // overlay-click closes (which bypass handleClose) reset too.
  const [wasOpen, setWasOpen] = useState(isOpen);
  if (wasOpen !== isOpen) {
    setWasOpen(isOpen);
    if (!isOpen) {
      setApiKey("");
    }
  }

  const getProviderDisplayName = () => {
    switch (providerName.toLowerCase()) {
      case "gemini":
        return "Gemini";
      case "openai":
        return "OpenAI";
      case "anthropic":
        return "Anthropic";
      case "soniox":
        return "Soniox";
      default:
        return providerName;
    }
  };

  const getProviderUrl = () => {
    switch (providerName.toLowerCase()) {
      case "gemini":
        return "https://aistudio.google.com/apikey";
      case "openai":
        return "https://platform.openai.com/api-keys";
      case "anthropic":
        return "https://console.anthropic.com/settings/keys";
      case "soniox":
        return "https://soniox.com/docs/stt/get-started";
      default:
        return "";
    }
  };

  const displayName = getProviderDisplayName();
  const providerUrl = docsUrl ?? getProviderUrl();
  const resolvedTitle = title ?? `Add ${displayName} API Key`;
  const resolvedDescription =
    description ??
    `Enter your API key to enable ${displayName}. Your key is stored securely in the system keychain.`;

  return (
    <Dialog open={isOpen} onOpenChange={handleClose}>
      <DialogContent className="sm:max-w-[425px]">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>{resolvedTitle}</DialogTitle>
            <DialogDescription>{resolvedDescription}</DialogDescription>
          </DialogHeader>

          <div className="grid gap-4 py-4">
            <div className="grid gap-2">
              <Label htmlFor="apiKey">API Key</Label>
              <Input
                id="apiKey"
                type="password"
                placeholder={`Enter your ${displayName} API key`}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                disabled={isLoading}
                autoFocus
              />
            </div>

            {providerUrl && (
              <div className="text-sm text-muted-foreground">
                <a
                  href={providerUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 hover:underline"
                >
                  Get your {displayName} API key
                  <ExternalLink className="h-3 w-3" />
                </a>
              </div>
            )}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={handleClose} disabled={isLoading}>
              Cancel
            </Button>
            <Button type="submit" disabled={!apiKey.trim() || isLoading}>
              {isLoading ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Saving...
                </>
              ) : (
                submitLabel
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
