param([string]$Title = "服务器控制中心", [int]$Tries = 6)
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class FA {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int v);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
}
"@
[FA]::SetProcessDpiAwareness(2) | Out-Null
$proc = Get-Process ServerDesk -ErrorAction SilentlyContinue
if (-not $proc) { Write-Output "no-process"; exit 2 }
$h = $proc.MainWindowHandle
if ([FA]::IsIconic($h)) { [FA]::ShowWindow($h, 9) | Out-Null }
[FA]::ShowWindow($h, 5) | Out-Null

$sb = New-Object System.Text.StringBuilder 256
for ($i = 0; $i -lt $Tries; $i++) {
  [FA]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
  [FA]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
  [FA]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 350
  $fg = [FA]::GetForegroundWindow()
  $sb.Clear() | Out-Null
  [FA]::GetWindowText($fg, $sb, 256) | Out-Null
  if ($sb.ToString() -eq $Title) { Write-Output "focused"; exit 0 }
}
Write-Output ("focus-failed: " + $sb.ToString())
exit 1
