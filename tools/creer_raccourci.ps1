# Crée le raccourci « Live Régie » (avec le logo) dans le dossier du projet,
# et sur le Bureau si on passe -Bureau.
# Usage : powershell -ExecutionPolicy Bypass -File tools\creer_raccourci.ps1 [-Bureau]
param([switch]$Bureau)

$root = Split-Path -Parent $PSScriptRoot
$dossiers = @($root)
if ($Bureau) { $dossiers += [Environment]::GetFolderPath("Desktop") }

$ws = New-Object -ComObject WScript.Shell
foreach ($d in $dossiers) {
    $lnk = $ws.CreateShortcut((Join-Path $d "Live Régie.lnk"))
    $lnk.TargetPath = Join-Path $root "Lancer_Studio_TikTok.bat"
    $lnk.WorkingDirectory = $root
    $lnk.IconLocation = (Join-Path $root "assets\logo\live_regie.ico") + ",0"
    $lnk.WindowStyle = 7  # fenêtre de commande réduite : seule la régie apparaît
    $lnk.Description = "Live Régie — régie de live pour TikTok LIVE Studio"
    $lnk.Save()
    Write-Output "Raccourci créé : $(Join-Path $d 'Live Régie.lnk')"
}
