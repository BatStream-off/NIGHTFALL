# Measure total RAM and CPU of a browser's processes on Windows.
# Usage: .\measure_windows.ps1 -Name nightfall -Seconds 300
param(
  [Parameter(Mandatory = $true)][string]$Name,
  [int]$Seconds = 300,
  [int]$Interval = 5
)
$out = "ram_cpu_$Name_$(Get-Date -Format yyyyMMdd_HHmmss).csv"
"time,processes,private_mb,cpu_percent" | Out-File $out
$cores = (Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors
$prev = @{}
for ($t = 0; $t -le $Seconds; $t += $Interval) {
  $procs = Get-Process | Where-Object { $_.ProcessName -like "*$Name*" }
  $mem = ($procs | Measure-Object -Property PrivateMemorySize64 -Sum).Sum / 1MB
  $cpuTotal = ($procs | Measure-Object -Property TotalProcessorTime -Sum).Sum
  $cpu = 0
  if ($prev.ContainsKey('cpu')) {
    $cpu = (($cpuTotal - $prev['cpu']).TotalSeconds / $Interval / $cores) * 100
  }
  $prev['cpu'] = $cpuTotal
  "$t,$($procs.Count),$([math]::Round($mem,1)),$([math]::Round($cpu,2))" | Out-File $out -Append
  Start-Sleep -Seconds $Interval
}
Write-Output "Résultats : $out"
