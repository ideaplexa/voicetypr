import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { Spinner } from "@/components/ui/spinner";
import { CheckCircle2, Eye, EyeOff, XCircle } from "lucide-react";
import { getModelDisplayName } from "@/lib/model-display";

export type TestStatus = "idle" | "testing" | "success" | "error";

export interface StatusResponse {
  status: string;
  version: string;
  model: string;
  name: string;
  machine_id: string;
}

interface ConnectionFieldsProps {
  host: string;
  port: string;
  saving: boolean;
  onHostChange: (value: string) => void;
  onPortChange: (value: string) => void;
}

export function AddServerConnectionFields({
  host,
  port,
  saving,
  onHostChange,
  onPortChange,
}: ConnectionFieldsProps) {
  return (
    <>
      <Field>
        <FieldLabel htmlFor="server-host">Host Address</FieldLabel>
        <Input
          id="server-host"
          placeholder="192.168.1.100 or hostname"
          value={host}
          onChange={(e) => onHostChange(e.target.value)}
          disabled={saving}
        />
        <FieldDescription>
          Use the host shown on the sharing Mac, or a stable LAN hostname.
        </FieldDescription>
      </Field>

      <Field>
        <FieldLabel htmlFor="server-port">Port</FieldLabel>
        <Input
          id="server-port"
          type="number"
          placeholder="47842"
          value={port}
          onChange={(e) => onPortChange(e.target.value)}
          disabled={saving}
          className="font-mono"
        />
      </Field>
    </>
  );
}

interface AuthFieldProps {
  password: string;
  saving: boolean;
  showPassword: boolean;
  initialServerRequiresPassword: boolean;
  isEditMode: boolean;
  hasSavedPassword: boolean;
  clearSavedPassword: boolean;
  onPasswordChange: (value: string) => void;
  onToggleShowPassword: () => void;
  onToggleClearSavedPassword: () => void;
}

export function AddServerAuthField({
  password,
  saving,
  showPassword,
  initialServerRequiresPassword,
  isEditMode,
  hasSavedPassword,
  clearSavedPassword,
  onPasswordChange,
  onToggleShowPassword,
  onToggleClearSavedPassword,
}: AuthFieldProps) {
  return (
    <Field>
      <FieldLabel htmlFor="server-password">
        Password {initialServerRequiresPassword ? "(required)" : "(if required)"}
      </FieldLabel>
      <InputGroup>
        <InputGroupInput
          id="server-password"
          type={showPassword ? "text" : "password"}
          placeholder={
            initialServerRequiresPassword
              ? "Enter the password from the sharing device"
              : isEditMode && hasSavedPassword
                ? "Enter the password from the sharing device"
                : "Enter the password from the sharing device (if required)"
          }
          value={password}
          onChange={(e) => onPasswordChange(e.target.value)}
          disabled={saving}
          className="[&::-ms-reveal]:hidden [&::-webkit-credentials-auto-fill-button]:hidden"
        />
        <InputGroupAddon align="inline-end">
          <InputGroupButton
            type="button"
            size="icon-xs"
            onClick={onToggleShowPassword}
            tabIndex={-1}
            aria-label={showPassword ? "Hide password" : "Show password"}
          >
            {showPassword ? <EyeOff className="size-4" /> : <Eye className="size-4" />}
          </InputGroupButton>
        </InputGroupAddon>
      </InputGroup>
      <FieldDescription>
        Enter the password shown on the device that is sharing transcription.
      </FieldDescription>
      {isEditMode && hasSavedPassword && !password && (
        <div>
          <Button
            type="button"
            variant={clearSavedPassword ? "destructive" : "outline"}
            size="sm"
            onClick={onToggleClearSavedPassword}
            disabled={saving}
          >
            {clearSavedPassword ? "Password will be removed" : "Keep saved password"}
          </Button>
        </div>
      )}
    </Field>
  );
}

interface NameFieldProps {
  name: string;
  saving: boolean;
  onNameChange: (value: string) => void;
}

export function AddServerNameField({ name, saving, onNameChange }: NameFieldProps) {
  return (
    <Field>
      <FieldLabel htmlFor="server-name">Display Name (optional)</FieldLabel>
      <Input
        id="server-name"
        placeholder="e.g., Office Desktop"
        value={name}
        onChange={(e) => onNameChange(e.target.value)}
        disabled={saving}
      />
    </Field>
  );
}

interface TestPanelProps {
  host: string;
  saving: boolean;
  testStatus: TestStatus;
  testResult: StatusResponse | null;
  testError: string | null;
  isSelfConnection: boolean;
  testRequiresReplacementPassword: boolean;
  onTestConnection: () => void;
}

export function AddServerTestPanel({
  host,
  saving,
  testStatus,
  testResult,
  testError,
  isSelfConnection,
  testRequiresReplacementPassword,
  onTestConnection,
}: TestPanelProps) {
  return (
    <>
      <Button
        variant="outline"
        className="w-full"
        onClick={onTestConnection}
        disabled={
          !host.trim() || testStatus === "testing" || saving || testRequiresReplacementPassword
        }
      >
        {testStatus === "testing" ? (
          <>
            <Spinner className="size-4" />
            Testing...
          </>
        ) : (
          "Test Connection"
        )}
      </Button>
      {testRequiresReplacementPassword && (
        <p className="text-xs text-muted-foreground">
          Enter a replacement password to test this server. Saving with this field empty keeps the
          saved password.
        </p>
      )}

      {testStatus === "success" && testResult && (
        <div className="rounded-lg border border-emerald-500/30 bg-emerald-500/10 px-3 py-2">
          <div className="flex items-center justify-between gap-2">
            <div className="flex items-center gap-1.5 text-emerald-700 dark:text-emerald-400">
              <CheckCircle2 className="size-3.5" />
              <span className="text-xs font-medium">Connected</span>
            </div>
            <span className="text-xs text-muted-foreground">
              {testResult.name} • {getModelDisplayName(testResult.model)}
            </span>
          </div>
        </div>
      )}

      {testStatus === "error" && testError && (
        <div
          className={`rounded-lg border px-3 py-2 ${
            isSelfConnection
              ? "border-amber-500/30 bg-amber-500/10"
              : "border-destructive/30 bg-destructive/10"
          }`}
        >
          <div
            className={`flex items-center gap-1.5 ${
              isSelfConnection ? "text-amber-700 dark:text-amber-400" : "text-destructive"
            }`}
          >
            <XCircle className="size-3.5" />
            <span className="text-xs font-medium">
              {isSelfConnection ? "Self-connection detected" : "Connection failed"}
            </span>
            <span className="text-xs text-muted-foreground">– {testError}</span>
          </div>
        </div>
      )}
    </>
  );
}
