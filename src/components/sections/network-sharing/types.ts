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
}

export interface FirewallStatus {
  firewall_enabled: boolean;
  app_allowed: boolean;
  may_be_blocked: boolean;
}

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
