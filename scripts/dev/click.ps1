param(
  [int]$X,
  [int]$Y
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class DpiHelper {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int value);
}
"@
[DpiHelper]::SetProcessDpiAwareness(2) | Out-Null

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MouseHelper {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
}
"@
[MouseHelper]::SetCursorPos($X, $Y) | Out-Null
Start-Sleep -Milliseconds 150
[MouseHelper]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 90
[MouseHelper]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Write-Output "clicked $X,$Y"
