Add-Type @"
using System; using System.Runtime.InteropServices;
public class MW {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int v);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
"@
[MW]::SetProcessDpiAwareness(2) | Out-Null
$p = Get-Process ServerDesk
[MW]::ShowWindow($p.MainWindowHandle, 6) | Out-Null   # SW_MINIMIZE
Start-Sleep -Milliseconds 1500
Write-Output "minimized"
Start-Sleep -Milliseconds 2500
[MW]::ShowWindow($p.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
[MW]::SetForegroundWindow($p.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 1500
Write-Output "restored"
