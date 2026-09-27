import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Check, Eye, EyeOff } from "lucide-react";

interface ConnectionSettingsPanelProps {
  enabled: boolean;
  port: string;
  savedPort: string;
  password: string;
  savedPassword: string;
  showPassword: boolean;
  savingPort: boolean;
  savingPassword: boolean;
  savingModelControl: boolean;
  passwordConfigured: boolean;
  allowModelControl: boolean;
  onPortChange: (value: string) => void;
  onPasswordChange: (value: string) => void;
  onTogglePasswordVisibility: () => void;
  onSavePort: () => void;
  onSavePassword: () => void;
  onToggleModelControl: (checked: boolean) => void;
}

export function ConnectionSettingsPanel({
  enabled,
  port,
  savedPort,
  password,
  savedPassword,
  showPassword,
  savingPort,
  savingPassword,
  savingModelControl,
  passwordConfigured,
  allowModelControl,
  onPortChange,
  onPasswordChange,
  onTogglePasswordVisibility,
  onSavePort,
  onSavePassword,
  onToggleModelControl,
}: ConnectionSettingsPanelProps) {
  return (
    <div className="rounded-lg border border-border/50 bg-background/50 p-3 space-y-3">
      <p className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
        Connection Settings
      </p>

      <div className="space-y-1.5">
        <Label htmlFor="sharing-port" className="text-sm">
          Port
        </Label>
        <div className="flex items-center gap-2">
          <Input
            id="sharing-port"
            type="number"
            value={port}
            onChange={(e) => onPortChange(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && port !== savedPort) {
                onSavePort();
              }
            }}
            placeholder="47842"
            className="font-mono h-9 flex-1"
          />
          {enabled && port !== savedPort && (
            <button
              onClick={onSavePort}
              disabled={savingPort}
              className="p-2 rounded-md bg-green-500/10 text-green-600 hover:bg-green-500/20 disabled:opacity-50 transition-colors"
              title="Save and restart server"
            >
              <Check className="h-4 w-4" />
            </button>
          )}
        </div>
        <p className="text-xs text-muted-foreground">
          Default: 47842. {enabled && port !== savedPort ? "Click checkmark to apply." : ""}
        </p>
      </div>

      <div className="space-y-1.5">
        <Label htmlFor="sharing-password" className="text-sm">
          Password (Optional)
        </Label>
        <p className="text-xs text-muted-foreground">
          Other devices need this password to connect to your shared transcription.
        </p>
        <div className="flex items-center gap-2">
          <div className="relative flex-1">
            <Input
              id="sharing-password"
              type={showPassword ? "text" : "password"}
              value={password}
              onChange={(e) => onPasswordChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && password !== savedPassword) {
                  onSavePassword();
                }
              }}
              placeholder={passwordConfigured ? "Password saved" : "No password"}
              className="h-9 pr-10 [&::-ms-reveal]:hidden [&::-webkit-credentials-auto-fill-button]:hidden"
            />
            <button
              type="button"
              onClick={onTogglePasswordVisibility}
              className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-muted-foreground hover:text-foreground transition-colors"
              tabIndex={-1}
              aria-label={showPassword ? "Hide password" : "Show password"}
            >
              {showPassword ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
            </button>
          </div>
          {enabled && password !== savedPassword && (
            <button
              onClick={onSavePassword}
              disabled={savingPassword}
              className="p-2 rounded-md bg-green-500/10 text-green-600 hover:bg-green-500/20 disabled:opacity-50 transition-colors"
              title="Save password"
            >
              <Check className="h-4 w-4" />
            </button>
          )}
          {enabled && passwordConfigured && !password && (
            <button
              onClick={onSavePassword}
              disabled={savingPassword}
              className="px-3 py-2 rounded-md border border-destructive/30 text-xs font-medium text-destructive hover:bg-destructive/10 disabled:opacity-50 transition-colors"
              title="Remove saved password"
            >
              Remove
            </button>
          )}
        </div>
      </div>

      <div className="space-y-1.5">
        <div className="flex items-start justify-between gap-3">
          <div className="space-y-1">
            <Label htmlFor="allow-model-control" className="text-sm">
              Allow trusted devices to change shared model
            </Label>
            <p className="text-xs text-muted-foreground">
              Requires a sharing password so only trusted devices can change the model this device
              shares.
            </p>
          </div>
          <Switch
            id="allow-model-control"
            checked={allowModelControl}
            onCheckedChange={(checked) => {
              void onToggleModelControl(checked);
            }}
            disabled={savingModelControl || !passwordConfigured}
          />
        </div>
        {!passwordConfigured && (
          <p className="text-xs text-muted-foreground">
            Add a sharing password first so only trusted devices can change the shared model.
          </p>
        )}
      </div>
    </div>
  );
}
