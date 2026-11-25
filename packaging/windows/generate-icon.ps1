$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$assets = Join-Path $PSScriptRoot '../../crates/windows-app/ui/assets'
$source = [Drawing.Image]::FromFile((Join-Path $assets 'app-icon.png'))
$sizes = @(16, 24, 32, 48, 64, 128, 256)
$stream = [IO.File]::Create((Join-Path $assets 'app-icon.ico'))
$writer = [IO.BinaryWriter]::new($stream)
try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    foreach ($size in $sizes) {
        $maskStride = [int]([Math]::Ceiling($size / 32) * 4)
        $length = 40 + 4 * $size * $size + $maskStride * $size
        $writer.Write([byte]($size % 256))
        $writer.Write([byte]($size % 256))
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]$length)
        $writer.Write([uint32]$offset)
        $offset += $length
    }
    foreach ($size in $sizes) {
        $bitmap = [Drawing.Bitmap]::new($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.DrawImage($source, 0, 0, $size, $size)
            $maskStride = [int]([Math]::Ceiling($size / 32) * 4)
            $writer.Write([uint32]40)
            $writer.Write([int]$size)
            $writer.Write([int]($size * 2))
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([byte[]]::new(24))
            $mask = [byte[]]::new($maskStride * $size)
            for ($row = 0; $row -lt $size; $row++) {
                for ($column = 0; $column -lt $size; $column++) {
                    $pixel = $bitmap.GetPixel($column, $size - 1 - $row)
                    $writer.Write([byte[]]@($pixel.B, $pixel.G, $pixel.R, $pixel.A))
                    if ($pixel.A -eq 0) {
                        $index = $row * $maskStride + ($column -shr 3)
                        $mask[$index] = $mask[$index] -bor (128 -shr ($column % 8))
                    }
                }
            }
            $writer.Write($mask)
        } finally {
            $graphics.Dispose()
            $bitmap.Dispose()
        }
    }
} finally {
    $writer.Dispose()
    $source.Dispose()
}
