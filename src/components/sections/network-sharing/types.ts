export interface BindingResult {
  ip: string;
  success: boolean;
  error: string | null;
  interface_name?: string;
}

export interface SharingStatus {
  enabled: boolean;
  port: number | null;
  model_name: string | null;
  server_name: string | null;
  active_connections: number;
  password_configured: boolean;
  binding_results: BindingResult[];
  allow_model_control: boolean;
  host_license_refusal?: HostLicenseRefusal | null;
}

export type HostLicenseRefusal = "host_license_required" | "host_license_unverified";

export interface FirewallStatus {
  firewall_enabled: boolean;
  app_allowed: boolean;
  may_be_blocked: boolean;
}

export type FirewallCheck =
  | { state: "checking" }
  | { state: "allowed"; status: FirewallStatus }
  | { state: "blocked"; status: FirewallStatus }
  | { state: "unknown" };

export interface SharingModelInfo {
  name: string;
  display_name: string;
  downloaded: boolean;
  engine?: string;
  kind?: string;
}

export interface ModelStatusResponse {
  models: SharingModelInfo[];
}
