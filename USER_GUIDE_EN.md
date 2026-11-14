# ServerDesk v0.1.1 User Guide

ServerDesk is a Windows desktop app that opens multiple web administration panels in one window, such as server dashboards, monitoring tools, and bot control panels. Each service still manages its own features and login credentials.

The app's interface currently uses Chinese labels. This guide shows those labels alongside English descriptions.

## 1. Download and launch

Requirements: Windows 10 or 11 and the WebView2 runtime.

### Installer

Download `ServerDesk_0.1.1_x64-setup.exe`, run it, and follow the installer. It includes the network channel program and is the recommended choice for regular use.

### Portable package

Download `ServerDesk_0.1.1_x64-portable.zip` and extract the entire archive before running `ServerDesk.exe`. Keep `serverdesk-channel.exe` in the same folder as `ServerDesk.exe`. Do not run a single file directly from inside the ZIP archive.

Both packages have the same features. If you are unsure which to choose, use the installer.

## 2. Add your first service

1. Open ServerDesk and go to **Settings (设置)**.
2. Under **Service List (服务列表)**, select **Add Service (添加服务)**.
3. Enter a name, category, connection mode, and service URL, then save.
4. Select the service in the sidebar to open its web page. If the page asks you to sign in, use that service's own account.

You may see example services on first launch. Replace their placeholder URLs with your own working addresses.

### Choose a connection mode

| Mode in the app | Use it when | Required address |
| --- | --- | --- |
| **Public (公网)** | You can already open the service directly in a browser | Public URL |
| **Private (私网)** | The service is reachable only through your Tailscale network | Private URL and network channel authorization |
| **Automatic failover (自动切换)** | The same service has both private and public endpoints | Both URLs; ServerDesk tries the private endpoint first |

Enter a full URL, including `http://` or `https://` and the correct port. ServerDesk does not deploy the web services for you.

## 3. Connect to a private service

This section applies to **Private** and **Automatic failover** modes.

1. Make sure the target server has joined the Tailscale network you intend to use and the service is reachable from that network.
2. In ServerDesk, enable the built-in channel under **Settings → Network Channel (设置 → 网络通道)**.
3. If authorization is requested, select **Authorize in Browser (在浏览器中授权)** and sign in with an account that has access to that Tailscale network.
4. Return to ServerDesk, wait for the channel to connect, then open the private service.

ServerDesk's built-in channel appears as a separate device and needs its own authorization. Public mode does not require this channel.

## 4. Local login protection and credential vault (optional)

- Under **Settings → Security (设置 → 安全)**, you can enable a password for opening ServerDesk. This protects the local app; it does not replace the login for each web service.
- When enabling this protection, save the recovery code shown once in a safe place. You may need it if you forget the password.
- The **Credential Vault (凭据保险箱)** can store service usernames and passwords encrypted on this computer. It has a separate secondary password. If you forget that password, you must reset the vault, which deletes its saved credentials.
- Automatic login works with some simple login forms. Pages with a CAPTCHA, two-factor authentication, or complex forms may still require manual sign-in.

## 5. Where your data is stored

The service list and app settings are stored under `%APPDATA%\com.serverdesk.app`. Web page sessions are stored under `%LOCALAPPDATA%\com.serverdesk.app`.

“Portable” means the app does not need an installer. It does **not** mean all user data stays in the extracted folder. Moving that folder alone will not move your settings or sessions. Do not upload your local data, service URLs, or credential files to a public repository.

## 6. Troubleshooting

**The portable app cannot find the channel program:** Extract the entire ZIP and keep `ServerDesk.exe` and `serverdesk-channel.exe` together in one folder.

**A private service does not open:** Check authorization and connection status under **Settings → Network Channel (设置 → 网络通道)**. Confirm that the target server is on the same Tailscale network and that the URL and port are correct.

**A public service does not open:** Try the same URL in a regular browser first, then check the address entered in ServerDesk.

**The app keeps running after you close its window:** If “close to system tray” is enabled, this is expected. Select **Exit (退出)** from the system tray to stop the app.

**A web page asks you to sign in again:** Use that service's own credentials. Private and public endpoints may keep separate web login sessions.