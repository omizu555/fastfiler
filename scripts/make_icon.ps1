# PNG → マルチサイズ ICO (PNG 埋め込み形式) を生成するスクリプト。
# 使い方: pwsh scripts\make_icon.ps1
#
# 入力:  crates\fastfiler-native\assets\icon.png
# 出力:  crates\fastfiler-native\assets\icon.ico (16/32/48/64/128/256 px)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$src = Join-Path $root 'crates\fastfiler-native\assets\icon.png'
$dst = Join-Path $root 'crates\fastfiler-native\assets\icon.ico'

Add-Type -AssemblyName System.Drawing
$srcImg = [System.Drawing.Image]::FromFile($src)
$sizes = @(16, 32, 48, 64, 128, 256)
$pngs = @()
foreach ($sz in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap $sz, $sz
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
    $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
    $g.DrawImage($srcImg, 0, 0, $sz, $sz)
    $g.Dispose()
    $ms = New-Object System.IO.MemoryStream
    $bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    $pngs += ,[pscustomobject]@{ Size = $sz; Data = $ms.ToArray() }
}
$srcImg.Dispose()

$fs = [System.IO.File]::Create($dst)
$bw = New-Object System.IO.BinaryWriter $fs
try {
    # ICONDIR
    $bw.Write([uint16]0)          # reserved
    $bw.Write([uint16]1)          # type = ICO
    $bw.Write([uint16]$pngs.Count)
    $offset = 6 + 16 * $pngs.Count
    foreach ($p in $pngs) {
        $w = if ($p.Size -ge 256) { [byte]0 } else { [byte]$p.Size }
        $h = $w
        $bw.Write([byte]$w)
        $bw.Write([byte]$h)
        $bw.Write([byte]0)        # palette
        $bw.Write([byte]0)        # reserved
        $bw.Write([uint16]1)      # planes
        $bw.Write([uint16]32)     # bitcount
        $bw.Write([uint32]$p.Data.Length)
        $bw.Write([uint32]$offset)
        $offset += $p.Data.Length
    }
    foreach ($p in $pngs) {
        $bw.Write($p.Data)
    }
} finally {
    $bw.Flush()
    $fs.Close()
}

Write-Host "Generated: $dst ($((Get-Item $dst).Length) bytes, $($pngs.Count) sizes)"
