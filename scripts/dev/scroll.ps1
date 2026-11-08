param([int]$X = 2000, [int]$Y = 900, [int]$Clicks = -3)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Sc {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int v);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, int d, UIntPtr e);
}
"@
[Sc]::SetProcessDpiAwareness(2) | Out-Null
[Sc]::SetCursorPos($X, $Y) | Out-Null
Start-Sleep -Milliseconds 150
[Sc]::mouse_event(0x0800, 0, 0, ($Clicks * 120), [UIntPtr]::Zero)
Start-Sleep -Milliseconds 500
Write-Output "scrolled"
