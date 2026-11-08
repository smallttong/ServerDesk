export type ConnectionMode = "tailnet" | "public" | "failover";
export type EndpointKind = "tailnet" | "public";

export interface Endpoint {
  endpointId: string;
  kind: EndpointKind;
  url: string;
  expectedOrigin: string;
  enabled: boolean;
  httpAckAt?: number | null;
  httpAckOrigin?: string | null;
  httpAckUrl?: string | null;
}

export interface ServiceConfig {
  id: string;
  name: string;
  category: string;
  icon: string;
  healthCheck: boolean;
  sortOrder: number;
  enabled: boolean;
  connectionMode: ConnectionMode;
  endpoints: Endpoint[];
}

export interface AppSettings {
  autostart: boolean;
  closeToTray: boolean;
  startPage: "home" | "last";
  channelEnabled: boolean;
  channelHostname: string;
  channelTags: string;
}

export interface ChannelStatus {
  /** stopped | starting | login | up | error */
  phase: "stopped" | "starting" | "login" | "up" | "error";
  socksPort: number | null;
  loginUrl: string | null;
  ip: string | null;
  message: string | null;
  hostname: string | null;
}

export interface AppConfig {
  version: number;
  services: ServiceConfig[];
  settings: AppSettings;
}

export interface HealthResult {
  id: string;
  online: boolean;
  latencyMs: number | null;
  error: string | null;
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface VaultStatus {
  initialized: boolean;
  unlocked: boolean;
  pendingReset: boolean;
  lockAfterSecs: number;
}

export interface CredentialView {
  credentialId: string;
  serviceId: string;
  endpointId: string;
  origin: string;
  username: string;
  credentialType: string;
  createdAt: number;
  updatedAt: number;
  autoLoginEnabled: boolean;
}

export interface CredentialInput {
  credentialId?: string | null;
  serviceId: string;
  endpointId: string;
  origin: string;
  username: string;
  secret?: string | null;
  credentialType?: string;
  autoLoginEnabled?: boolean;
}

export interface GrantView {
  grantId: string;
  credentialId: string;
  serviceId: string;
  endpointId: string;
  origin: string;
  createdAt: number;
  expiresAt: number;
  expired: boolean;
  storage: "session" | "persisted";
}

export interface AuthStatus {
  enabled: boolean;
  registered: boolean;
  authenticated: boolean;
  locked: boolean;
  username: string | null;
  userId: string | null;
  autoLockMinutes: number;
  error: string | null;
}

export interface EnableAuthResult {
  status: AuthStatus;
  recoveryCode: string | null;
}
