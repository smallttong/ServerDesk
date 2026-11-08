param(
  [string]$Path = "shot.png"
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class DpiAware {
  [DllImport("shcore.dll")] public static extern int SetProcessDpiAwareness(int value);
}
"@
[DpiAware]::SetProcessDpiAwareness(2) | Out-Null

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($bounds.X, $bounds.Y, 0, 0, $bmp.Size)
$bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose()
$bmp.Dispose()
Write-Output "saved $Path ($($bounds.Width)x$($bounds.Height))"
