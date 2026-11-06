# Génère le logo « Live Régie » : console de régie (3 curseurs aux couleurs des branches)
# + point rouge « en direct ». Dessiné en vectoriel pour chaque taille, puis assemblé en .ico.
# Usage : powershell -ExecutionPolicy Bypass -File tools\generer_logo.ps1

Add-Type -AssemblyName System.Drawing
$root = Split-Path -Parent $PSScriptRoot
$outDir = Join-Path $root "assets\logo"
New-Item -ItemType Directory -Force $outDir | Out-Null

function New-RoundedRect([single]$x, [single]$y, [single]$w, [single]$h, [single]$r) {
    $p = New-Object Drawing.Drawing2D.GraphicsPath
    $d = 2 * $r
    $p.AddArc($x, $y, $d, $d, 180, 90)
    $p.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $p.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $p.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $p.CloseFigure()
    return $p
}

function Draw-Logo([int]$size) {
    $bmp = New-Object Drawing.Bitmap $size, $size, ([Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([Drawing.Color]::Transparent)
    $s = $size / 256.0

    # Fond : carré arrondi sombre (béton nuit) avec léger dégradé
    $bg = New-RoundedRect (8 * $s) (8 * $s) (240 * $s) (240 * $s) (52 * $s)
    $grad = New-Object Drawing.Drawing2D.LinearGradientBrush ([Drawing.PointF]::new(0, 0)), ([Drawing.PointF]::new($size, $size)), ([Drawing.Color]::FromArgb(255, 36, 34, 52)), ([Drawing.Color]::FromArgb(255, 14, 14, 22))
    $g.FillPath($grad, $bg)
    if ($size -ge 32) {
        $g.DrawPath((New-Object Drawing.Pen ([Drawing.Color]::FromArgb(90, 255, 255, 255)), ([single][Math]::Max(1, 3 * $s))), $bg)
    }

    # Trois curseurs de console (violet VSS, rouge/orange politique-musique, vert chill)
    $faders = @(
        @{ x = 62;  knob = 150; col = [Drawing.Color]::FromArgb(255, 139, 92, 246) },
        @{ x = 112; knob = 92;  col = [Drawing.Color]::FromArgb(255, 249, 115, 22) },
        @{ x = 162; knob = 128; col = [Drawing.Color]::FromArgb(255, 16, 185, 129) }
    )
    $top = 62; $bottom = 206
    foreach ($f in $faders) {
        $trackW = [Math]::Max(2, 10 * $s)
        $cx = $f.x * $s + (16 * $s - $trackW) / 2
        $track = New-RoundedRect $cx ($top * $s) $trackW (($bottom - $top) * $s) ($trackW / 2)
        $g.FillPath((New-Object Drawing.SolidBrush ([Drawing.Color]::FromArgb(255, 70, 70, 92))), $track)
        $knobW = 36 * $s; $knobH = [Math]::Max(3, 20 * $s)
        $kx = $f.x * $s + 8 * $s - $knobW / 2
        $ky = $f.knob * $s - $knobH / 2
        $knob = New-RoundedRect $kx $ky $knobW $knobH ([Math]::Max(1, 6 * $s))
        $g.FillPath((New-Object Drawing.SolidBrush $f.col), $knob)
        if ($size -ge 48) {
            $g.FillRectangle((New-Object Drawing.SolidBrush ([Drawing.Color]::FromArgb(200, 255, 255, 255))), [single]($kx + 6 * $s), [single]($ky + $knobH / 2 - 1.5 * $s), [single]($knobW - 12 * $s), [single]([Math]::Max(1, 3 * $s)))
        }
    }

    # Point rouge « en direct » en haut à droite, avec halo et anneau blanc
    $dot = 46 * $s; $dx = 186 * $s; $dy = 26 * $s
    if ($size -ge 32) {
        $g.FillEllipse((New-Object Drawing.SolidBrush ([Drawing.Color]::FromArgb(70, 239, 68, 68))), [single]($dx - 8 * $s), [single]($dy - 8 * $s), [single]($dot + 16 * $s), [single]($dot + 16 * $s))
    }
    $g.FillEllipse((New-Object Drawing.SolidBrush ([Drawing.Color]::White)), [single]$dx, [single]$dy, [single]$dot, [single]$dot)
    $inner = $dot * 0.72
    $g.FillEllipse((New-Object Drawing.SolidBrush ([Drawing.Color]::FromArgb(255, 239, 38, 38))), [single]($dx + ($dot - $inner) / 2), [single]($dy + ($dot - $inner) / 2), [single]$inner, [single]$inner)

    $g.Dispose()
    return $bmp
}

# PNG grand format (fenêtre de l'appli, doc) + .ico multi-tailles (exe, raccourci)
$big = Draw-Logo 512
$big.Save((Join-Path $outDir "live_regie_512.png"), [Drawing.Imaging.ImageFormat]::Png)
$png256 = Draw-Logo 256
$png256.Save((Join-Path $outDir "live_regie_256.png"), [Drawing.Imaging.ImageFormat]::Png)

# Entrée .ico au format bitmap (DIB 32 bits + masque) : lue partout, contrairement au PNG
# que certains lecteurs n'acceptent que pour la taille 256
function Get-DibBytes([Drawing.Bitmap]$bmp) {
    $sz = $bmp.Width
    $ms = New-Object IO.MemoryStream
    $bw = New-Object IO.BinaryWriter $ms
    $maskRow = [int]([Math]::Ceiling($sz / 32.0) * 4)
    $bw.Write([UInt32]40); $bw.Write([Int32]$sz); $bw.Write([Int32]($sz * 2))
    $bw.Write([UInt16]1); $bw.Write([UInt16]32); $bw.Write([UInt32]0)
    $bw.Write([UInt32]($sz * $sz * 4 + $maskRow * $sz)); $bw.Write([Int32]0); $bw.Write([Int32]0); $bw.Write([UInt32]0); $bw.Write([UInt32]0)
    for ($y = $sz - 1; $y -ge 0; $y--) {
        for ($x = 0; $x -lt $sz; $x++) {
            $c = $bmp.GetPixel($x, $y)
            $bw.Write([byte]$c.B); $bw.Write([byte]$c.G); $bw.Write([byte]$c.R); $bw.Write([byte]$c.A)
        }
    }
    $bw.Write((New-Object byte[] ($maskRow * $sz)))  # masque AND vide : la transparence vient du canal alpha
    return , $ms.ToArray()  # la virgule empêche PowerShell de dérouler le tableau d'octets
}

$sizes = 16, 24, 32, 48, 64, 128, 256
$blobs = foreach ($sz in $sizes) {
    $bmp = Draw-Logo $sz
    if ($sz -ge 256) {
        $ms = New-Object IO.MemoryStream
        $bmp.Save($ms, [Drawing.Imaging.ImageFormat]::Png)
        , $ms.ToArray()
    } else {
        , (Get-DibBytes $bmp)
    }
}
$ico = New-Object IO.MemoryStream
$w = New-Object IO.BinaryWriter $ico
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
for ($i = 0; $i -lt $sizes.Count; $i++) {
    $sz = $sizes[$i]; $len = $blobs[$i].Length
    $w.Write([byte]($(if ($sz -ge 256) { 0 } else { $sz }))); $w.Write([byte]($(if ($sz -ge 256) { 0 } else { $sz })))
    $w.Write([byte]0); $w.Write([byte]0); $w.Write([UInt16]1); $w.Write([UInt16]32)
    $w.Write([UInt32]$len); $w.Write([UInt32]$offset)
    $offset += $len
}
foreach ($b in $blobs) { $w.Write($b) }
[IO.File]::WriteAllBytes((Join-Path $outDir "live_regie.ico"), $ico.ToArray())
Write-Output "Logo généré dans $outDir"
