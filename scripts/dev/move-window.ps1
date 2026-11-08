param(
  [int]$X = 600,
  [int]$Y = 300,
  [int]$W = 2000,
  [int]$H = 1400
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Mover {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int v);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
}
"@
[Mover]::SetProcessDpiAwareness(2) | Out-Null

$p = Get-Process ServerDesk
[Mover]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, $X, $Y, $W, $H, 0x0004) | Out-Null
Start-Sleep -Milliseconds 1200
Write-Output "moved to $X,$Y size ${W}x${H}"
