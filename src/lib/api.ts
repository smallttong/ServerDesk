import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  AuthStatus,
  EnableAuthResult,
  ChannelStatus,
  CredentialInput,
  CredentialView,
  GrantView,
  HealthResult,
  Rect,
  VaultStatus,
} from "./types";

export const api = {
  getConfig: () => invoke<AppConfig>("get_config"),
  saveConfig: (config: AppConfig) => invoke<void>("save_config", { config }),
  configPath: () => invoke<string>("config_path"),

  channelStatus: () => invoke<ChannelStatus>("channel_status"),
  channelStart: () => invoke<ChannelStatus>("channel_start"),
  channelStop: () => invoke<void>("channel_stop"),
  channelLogout: () => invoke<void>("channel_logout"),
  channelOpenLogin: (url: string) =>
    invoke<void>("channel_open_login", { url }),

  authStatus: () => invoke<AuthStatus>("auth_get_status"),
  authInitialize: (username: string, password: string, confirm: string) =>
    invoke<EnableAuthResult>("auth_initialize", { username, password, confirm }),
  authRecoverWithCode: (code: string) =>
    invoke<AuthStatus>("auth_recover_with_code", { code }),
  authResetWithRecovery: (code: string) =>
    invoke<void>("auth_reset_with_recovery", { code }),
  authFactoryReset: () => invoke<void>("auth_factory_reset"),
  authLogin: (username: string, password: string) =>
    invoke<AuthStatus>("auth_login", { username, password }),
  authLogout: () => invoke<void>("auth_logout"),
  authLock: () => invoke<void>("auth_lock"),
  authVerifyPassword: (password: string) =>
    invoke<void>("auth_verify_password", { password }),
  authChangePassword: (current: string, next: string, confirm: string) =>
    invoke<void>("auth_change_password", { current, next, confirm }),
  authDisable: (password: string) =>
    invoke<void>("auth_disable", { password }),
  authUpdateLockSettings: (minutes: number) =>
    invoke<AuthStatus>("auth_update_lock_settings", { minutes }),

  vaultStatus: () => invoke<VaultStatus>("vault_status"),
  vaultInitialize: (password: string) =>
    invoke<void>("vault_initialize", { password }),
  vaultUnlock: (password: string) => invoke<void>("vault_unlock", { password }),
  vaultLock: () => invoke<void>("vault_lock"),
  vaultListCredentials: () =>
    invoke<CredentialView[]>("vault_list_credentials"),
  vaultAddCredential: (input: CredentialInput) =>
    invoke<CredentialView>("vault_add_credential", { input }),
  vaultUpdateCredential: (input: CredentialInput) =>
    invoke<CredentialView>("vault_update_credential", { input }),
  vaultDeleteCredential: (credentialId: string) =>
    invoke<void>("vault_delete_credential", { credentialId }),
  vaultRevealCredential: (credentialId: string) =>
    invoke<string>("vault_reveal_credential", { credentialId }),
  vaultAuthorize: (credentialId: string, days: number) =>
    invoke<GrantView>("vault_authorize_service", { credentialId, days }),
  vaultListAuthorizations: () =>
    invoke<GrantView[]>("vault_list_authorizations"),
  vaultRevokeAuthorization: (grantId: string) =>
    invoke<void>("vault_revoke_authorization", { grantId }),
  vaultReset: () => invoke<void>("vault_reset"),

  openService: (id: string) => invoke<void>("open_service", { id }),
  hideAllServices: () => invoke<void>("hide_all_services"),
  closeService: (id: string) => invoke<void>("close_service", { id }),
  setContentRect: (rect: Rect) => invoke<void>("set_content_rect", { rect }),

  navBack: () => invoke<void>("nav_back"),
  navForward: () => invoke<void>("nav_forward"),
  navReload: () => invoke<void>("nav_reload"),
  navHome: () => invoke<void>("nav_home"),
  activeUrl: () => invoke<string | null>("active_url"),
  openExternal: (url: string) => invoke<void>("open_external", { url }),

  checkHealth: (ids: string[]) =>
    invoke<HealthResult[]>("check_services", { ids }),

  setAutostart: (enabled: boolean) =>
    invoke<void>("set_autostart", { enabled }),
  quit: () => invoke<void>("quit_app"),
};
