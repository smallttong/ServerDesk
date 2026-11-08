Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class MClick {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int v);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
}
"@
[MClick]::SetProcessDpiAwareness(2) | Out-Null

function Click-At([int]$x, [int]$y) {
  [MClick]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 120
  [MClick]::mouse_event(0x002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  [MClick]::mouse_event(0x004, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 250
}

# Name field
Click-At 1417 798
[System.Windows.Forms.SendKeys]::SendWait("Jellyfin")

# URL field
Click-At 1604 913
[System.Windows.Forms.SendKeys]::SendWait("http://127.0.0.1:6185")

# Save button
Click-At 1921 1373
Start-Sleep -Milliseconds 600
Write-Output "form submitted"
