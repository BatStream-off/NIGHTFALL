# Integrated title bar (prototype M-0005): drives the real NIGHTFALL window
# with the mouse on Windows (lock screen) and checks that the frameless
# window moves, maximises by double-click, resizes by its edge and that its
# own buttons work. Prints "NFTITLEBAR ..." lines; exit code 1 on failure.
param([int]$ProcessId = 0, [string]$ShotDir = $env:RUNNER_TEMP)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class NfWin {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out int v, int s);
  public static void Down() { mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero); }
  public static void Up() { mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero); }
  public static int Hit(IntPtr h, int x, int y) { return (int)SendMessage(h, 0x0084, IntPtr.Zero, (IntPtr)((y << 16) | (x & 0xFFFF))); }
}
"@
$fail = 0
function Say($s) { Write-Output "NFTITLEBAR $s" }
function Check($name, $ok, $detail) { if ($ok) { Say "OK   $name $detail" } else { Say "ECHEC $name $detail"; $script:fail = 1 } }
function Rect($h) { $r = New-Object NfWin+RECT; [void][NfWin]::GetWindowRect($h, [ref]$r); return $r }
function Fmt($r) { "($($r.L),$($r.T))-($($r.R),$($r.B)) $($r.R-$r.L)x$($r.B-$r.T)" }
function Drag($x1, $y1, $x2, $y2) {
  [void][NfWin]::SetCursorPos($x1, $y1); Start-Sleep -Milliseconds 150
  [NfWin]::Down(); Start-Sleep -Milliseconds 150
  for ($i = 1; $i -le 12; $i++) { [void][NfWin]::SetCursorPos([int]($x1 + ($x2 - $x1) * $i / 12), [int]($y1 + ($y2 - $y1) * $i / 12)); Start-Sleep -Milliseconds 40 }
  Start-Sleep -Milliseconds 150; [NfWin]::Up(); Start-Sleep -Milliseconds 700
}
function Click($x, $y, [int]$n = 1) {
  [void][NfWin]::SetCursorPos($x, $y); Start-Sleep -Milliseconds 150
  for ($i = 0; $i -lt $n; $i++) { [NfWin]::Down(); Start-Sleep -Milliseconds 40; [NfWin]::Up(); Start-Sleep -Milliseconds 60 }
  Start-Sleep -Milliseconds 900
}
function Shot($name) {
  $sb = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $sb.Width, $sb.Height
  [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($sb.Location, [System.Drawing.Point]::Empty, $sb.Size)
  $small = New-Object System.Drawing.Bitmap $bmp, 960, ([int](960 * $sb.Height / $sb.Width))
  $f = Join-Path $ShotDir "$name.png"; $small.Save($f, [System.Drawing.Imaging.ImageFormat]::Png)
  Write-Output "NFSHOT-$name-BEGIN"; [Convert]::ToBase64String([IO.File]::ReadAllBytes($f)); Write-Output "NFSHOT-$name-END"
}

# The top-level NIGHTFALL window.
$h = [IntPtr]::Zero
foreach ($p in Get-Process | Where-Object { $_.ProcessName -like 'nightfall*' -and $_.MainWindowHandle -ne 0 }) { $h = $p.MainWindowHandle; break }
if ($h -eq [IntPtr]::Zero) { Say "ECHEC fenêtre introuvable"; exit 1 }
$cls = New-Object System.Text.StringBuilder 256; [void][NfWin]::GetClassName($h, $cls, 256)
$style = [NfWin]::GetWindowLong($h, -16)
Say ("fenêtre 0x{0:X} classe={1} style=0x{2:X8} (WS_CAPTION={3})" -f [int64]$h, $cls, $style, (($style -band 0x00C00000) -eq 0x00C00000))
$corner = 0; $dwm = [NfWin]::DwmGetWindowAttribute($h, 33, [ref]$corner, 4)
Say "DWM coins arrondis (attribut 33) : hr=$dwm valeur=$corner (0 = défaut système)"
[void][NfWin]::SetForegroundWindow($h); Start-Sleep -Milliseconds 500
$r0 = Rect $h; Say "position initiale $(Fmt $r0)"
$w = $r0.R - $r0.L; $cx = $r0.L + [int]($w / 2); $ty = $r0.T + 18

# 1. Hit test answered by the real window procedure (lock screen band).
$ht = [NfWin]::Hit($h, $cx, $ty); Check "bande de titre = HTCAPTION(2)" ($ht -eq 2) "réponse $ht"
$ht = [NfWin]::Hit($h, $r0.R - 23, $ty); Check "bouton Fermer = HTCLIENT(1)" ($ht -eq 1) "réponse $ht"
$ht = [NfWin]::Hit($h, $r0.L + 1, $r0.T + [int](($r0.B - $r0.T) / 2)); Check "bord gauche = HTLEFT(10)" ($ht -eq 10) "réponse $ht"
$ht = [NfWin]::Hit($h, $cx, $r0.T + 300); Check "contenu = HTCLIENT(1)" ($ht -eq 1) "réponse $ht"

# 2. Move by dragging the band.
Drag $cx $ty ($cx - 140) ($ty + 60)
$r1 = Rect $h; Check "déplacement par glisser" (($r1.L - $r0.L) -le -100 -and ($r1.T - $r0.T) -ge 40) "$(Fmt $r1)"

# 3. Resize by the right edge.
$r = Rect $h; $my = $r.T + [int](($r.B - $r.T) / 2)
Drag ($r.R - 2) $my ($r.R - 120) $my
$r2 = Rect $h; Check "redimensionnement par le bord droit" (($r2.R - $r2.L) -le ($r.R - $r.L) - 80) "$(Fmt $r2)"

# 4. Double-click on the band maximises, again restores.
$r = Rect $h; Click ($r.L + [int](($r.R - $r.L) / 2)) ($r.T + 18) 2
$z = [NfWin]::IsZoomed($h); $rm = Rect $h; Check "double-clic : agrandie" $z "$(Fmt $rm)"
$sw = [NfWin]::GetSystemMetrics(0); $sh = [NfWin]::GetSystemMetrics(1)
Say "écran ${sw}x${sh} ; fenêtre agrandie $(Fmt $rm) (débordement gauche/haut : $($rm.L),$($rm.T))"
Shot "max"
Click ($rm.L + [int](($rm.R - $rm.L) / 2)) ($rm.T + 18) 2
Check "double-clic : restaurée" (-not [NfWin]::IsZoomed($h)) "$(Fmt (Rect $h))"

# 5. The interface's own buttons (Windows): maximise, restore, minimise.
$r = Rect $h; Click ($r.R - 69) ($r.T + 19)
Check "bouton Agrandir" ([NfWin]::IsZoomed($h)) "$(Fmt (Rect $h))"
$r = Rect $h; Click ($r.R - 69) ($r.T + 19)
Check "bouton Restaurer" (-not [NfWin]::IsZoomed($h)) "$(Fmt (Rect $h))"
$r = Rect $h; Click ($r.R - 115) ($r.T + 19)
Check "bouton Réduire" ([NfWin]::IsIconic($h)) ""
[void][NfWin]::ShowWindow($h, 9); Start-Sleep -Milliseconds 800; [void][NfWin]::SetForegroundWindow($h)
Check "restauration après réduction" (-not [NfWin]::IsIconic($h)) "$(Fmt (Rect $h))"
Shot "after"
exit $fail
