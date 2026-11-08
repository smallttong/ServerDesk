param(
  [string]$WindowTitle = "服务器控制中心",
  [string]$Values = "",
  [string]$ButtonText = ""
)
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes

$root = [System.Windows.Automation.AutomationElement]::RootElement
$cond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::NameProperty, $WindowTitle)
$win = $root.FindFirst([System.Windows.Automation.TreeScope]::Children, $cond)
if (-not $win) { Write-Output "window not found"; exit 2 }

$editCond = New-Object System.Windows.Automation.PropertyCondition(
  [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
  [System.Windows.Automation.ControlType]::Edit)
$edits = $win.FindAll([System.Windows.Automation.TreeScope]::Descendants, $editCond)
Write-Output ("edits found: " + $edits.Count)

$items = @()
if ($Values -ne "") { $items = $Values.Split("|") }
$i = 0
foreach ($value in $items) {
  if ($i -ge $edits.Count) { break }
  try {
    $pattern = $edits[$i].GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
    $pattern.SetValue($value)
    Write-Output ("set edit[" + $i + "]")
  } catch {
    Write-Output ("edit[" + $i + "] failed: " + $_.Exception.Message)
  }
  $i++
}

if ($ButtonText -ne "") {
  $btnCond = New-Object System.Windows.Automation.PropertyCondition(
    [System.Windows.Automation.AutomationElement]::NameProperty, $ButtonText)
  $btn = $win.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $btnCond)
  if ($btn) {
    $invoke = $btn.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $invoke.Invoke()
    Write-Output "button invoked"
  } else {
    Write-Output "button not found"
  }
}
