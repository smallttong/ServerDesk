Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WEnum {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int v);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int idx);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  public struct RECT { public int L, T, R, B; }
}
"@
[WEnum]::SetProcessDpiAwareness(2) | Out-Null

$target = (Get-Process ServerDesk).Id
$rows = New-Object System.Collections.ArrayList

$cb = [WEnum+EnumProc] {
  param($h, $p)
  $wpid = 0
  [WEnum]::GetWindowThreadProcessId($h, [ref]$wpid) | Out-Null
  if ($wpid -eq $target) {
    $sb = New-Object System.Text.StringBuilder 256
    [WEnum]::GetWindowText($h, $sb, 256) | Out-Null
    $cn = New-Object System.Text.StringBuilder 256
    [WEnum]::GetClassName($h, $cn, 256) | Out-Null
    $r = New-Object WEnum+RECT
    [WEnum]::GetWindowRect($h, [ref]$r) | Out-Null
    $owner = [WEnum]::GetWindow($h, 4)
    $ex = [WEnum]::GetWindowLong($h, -20)
    $null = $rows.Add("hwnd=$h vis=$([WEnum]::IsWindowVisible($h)) owner=$owner exStyle=$ex class=$($cn.ToString()) title='$($sb.ToString())' rect=$($r.L),$($r.T)-$($r.R),$($r.B)")
  }
  return $true
}
[WEnum]::EnumWindows($cb, [IntPtr]::Zero) | Out-Null
$rows | ForEach-Object { Write-Output $_ }
