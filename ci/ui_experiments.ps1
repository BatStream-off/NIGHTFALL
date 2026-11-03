# CI diagnosis (Windows): see ui_experiments.sh.
param([string]$Bin)
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
foreach ($exp in @("direct", "direct-shell", "full", "nobg", "chromewin", "chromewin-shell")) {
  $env:NIGHTFALL_PROFILE_DIR = "$env:RUNNER_TEMP\nf-exp-$exp"
  $env:NIGHTFALL_UI_EXPERIMENT = $exp
  $p = Start-Process -FilePath $Bin -PassThru
  Start-Sleep -Seconds 15
  $sb = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $sb.Width, $sb.Height
  [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($sb.Location, [System.Drawing.Point]::Empty, $sb.Size)
  $small = New-Object System.Drawing.Bitmap $bmp, 640, ([int](640 * $sb.Height / $sb.Width))
  $file = "$env:RUNNER_TEMP\exp-$exp.png"
  $small.Save($file, [System.Drawing.Imaging.ImageFormat]::Png)
  python ci/probe_color.py $exp $file
  Write-Output "NFSHOT-BEGIN-$exp"; [Convert]::ToBase64String([IO.File]::ReadAllBytes($file)); Write-Output "NFSHOT-END-$exp"
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Get-Process | Where-Object { $_.Path -like "*selftest-dist*" } | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
}
exit 0
