param([string]$Button = "left", [int]$MinTop = 1800)

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Mouse3 {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int v);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
}
"@
[Mouse3]::SetProcessDpiAwareness(2) | Out-Null

$procId = (Get-Process ServerDesk).Id
$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ProcessIdProperty, $procId)
$all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $cond)

$pick = $null
foreach ($e in $all) {
  $r = $e.Current.BoundingRectangle
  if ($r.Width -le 0 -or $r.Height -le 0) { continue }
  if ($r.Top -lt $MinTop) { continue }
  Write-Output ("candidate name=" + $e.Current.Name + " class=" + $e.Current.ClassName + " rect=" + $r.Left + "," + $r.Top + "," + $r.Width + "," + $r.Height)
  if (-not $pick) { $pick = $e }
}

if (-not $pick) { Write-Output "tray-icon-not-found"; exit 2 }

$r = $pick.Current.BoundingRectangle
$x = [int]($r.Left + $r.Width / 2)
$y = [int]($r.Top + $r.Height / 2)
[Mouse3]::SetCursorPos($x, $y) | Out-Null
Start-Sleep -Milliseconds 200

if ($Button -eq "right") { $down = 0x008; $up = 0x010 } else { $down = 0x002; $up = 0x004 }
[Mouse3]::mouse_event($down, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 100
[Mouse3]::mouse_event($up, 0, 0, 0, [UIntPtr]::Zero)
Write-Output ("clicked " + $Button + " at " + $x + "," + $y)
