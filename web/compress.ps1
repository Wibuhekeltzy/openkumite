# Windows' web/compress.sh (run by web\build.bat): compressed copies for the web server to send: the game (pkg\:
# brotli and gzip) and the assets that compress (models, stages, game data: gzip; brotli too with -Br). Brotli needs
# PowerShell 7 (.NET's BrotliStream); Windows PowerShell 5.1 makes the gzip copies only, which the server sends as well.
# Pictures, music and movies are compressed already; sound effects stay as they are (audio elements ask for ranges).
# Only what changed since its copy is redone.
# Also web\assets, as link_assets.sh: a junction to the converted assets (OPENKUMITE_ASSETS when set, else ..\assets),
# made again on each run; a real folder there is left as it is.
param([switch]$Br)
$ErrorActionPreference = 'Stop'
$web = $PSScriptRoot
Set-Location $web
Add-Type -AssemblyName System.IO.Compression

$link = Join-Path $web 'assets'
$to = if ($env:OPENKUMITE_ASSETS) { $env:OPENKUMITE_ASSETS } else { Join-Path $web '..\assets' }
$item = Get-Item $link -Force -ErrorAction SilentlyContinue
if (-not $item -or $item.LinkType) {
    if (-not (Test-Path $to -PathType Container)) { throw "OPENKUMITE_ASSETS: no folder $to" }
    if ($item) { $item.Delete() }
    New-Item -ItemType Junction -Path $link -Target (Resolve-Path $to).Path | Out-Null
}

$brotli = [type]::GetType('System.IO.Compression.BrotliStream, System.IO.Compression.Brotli', $false)
if (-not $brotli) { $brotli = 'System.IO.Compression.BrotliStream' -as [type] }
function Write-Copy($file, $copy, $make) {
    $src = Get-Item $file
    if ((Test-Path $copy) -and (Get-Item $copy).LastWriteTimeUtc -ge $src.LastWriteTimeUtc) { return }
    $in = [IO.File]::OpenRead($src.FullName)
    $out = [IO.File]::Create($copy)
    try { $z = & $make $out; try { $in.CopyTo($z) } finally { $z.Dispose() } } finally { $out.Dispose(); $in.Dispose() }
}
function Pack($file, $withBr) {   # FILE: FILE.gz (and FILE.br) when missing or older
    Write-Copy $file "$file.gz" { param($o) [IO.Compression.GZipStream]::new($o, [IO.Compression.CompressionLevel]::Optimal) }
    if ($withBr -and $brotli) {
        Write-Copy $file "$file.br" { param($o) $brotli::new($o, [IO.Compression.CompressionLevel]::SmallestSize) }
    }
}
foreach ($f in @(Get-ChildItem pkg -File -Include *.wasm, *.js -Recurse) + @(Get-Item preload.json -ErrorAction SilentlyContinue)) {
    if ($f) { Pack $f.FullName $true }
}
Get-ChildItem assets -Recurse -File -Include *.glb, *.json | ForEach-Object { Pack $_.FullName $Br }
$n = @(Get-ChildItem assets, pkg -Recurse -File -Filter *.gz -ErrorAction SilentlyContinue).Count
"compressed: $n files"
