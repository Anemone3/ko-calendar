param([switch]$Update)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$taskManifestPath = Join-Path (Split-Path -Parent $PSScriptRoot) 'bucket/ko-calendar.json'
$taskManifest = Get-Content -LiteralPath $taskManifestPath -Raw | ConvertFrom-Json
$taskRepository = 'Anemone3/ko-calendar'
$taskHomepage = "https://github.com/$taskRepository"

if ($Update) {
    $taskHeaders = @{ Accept = 'application/vnd.github+json' }
    if ($env:GH_TOKEN) { $taskHeaders.Authorization = "Bearer $env:GH_TOKEN" }
    $taskRelease = Invoke-RestMethod "https://api.github.com/repos/$taskRepository/releases/latest" -Headers $taskHeaders
    $taskManifest.version = $taskRelease.tag_name -replace '^v', ''
}
if ($taskManifest.version -notmatch '^\d+\.\d+\.\d+$') {
    throw 'La versión de Scoop debe ser una versión estable.'
}
$taskBaseUrl = "$taskHomepage/releases/download/v$($taskManifest.version)"
$taskArchiveUrl = "$taskBaseUrl/ko-calendar-windows-x64.zip"
$taskChecksums = (Invoke-WebRequest "$taskBaseUrl/SHA256SUMS").Content
if ($taskChecksums -is [byte[]]) { $taskChecksums = [Text.Encoding]::UTF8.GetString($taskChecksums) }
$taskMatch = [regex]::Match($taskChecksums, '(?m)^([a-f0-9]{64})\s+\*?ko-calendar-windows-x64\.zip\r?$')
if (-not $taskMatch.Success) { throw 'No se encontró el hash oficial del ZIP de Windows.' }
$taskHash = $taskMatch.Groups[1].Value

if ($Update) {
    $taskManifest.architecture.'64bit'.url = $taskArchiveUrl
    $taskManifest.architecture.'64bit'.hash = $taskHash
}
if ($taskManifest.homepage -ne $taskHomepage -or $taskManifest.bin -ne 'ko-calendar.exe' -or
    $taskManifest.architecture.'64bit'.url -ne $taskArchiveUrl -or
    $taskManifest.architecture.'64bit'.hash -ne $taskHash) {
    throw 'El manifiesto no coincide con la release oficial de ko-calendar.'
}
$taskJson = $taskManifest | ConvertTo-Json -Depth 10
$taskSchema = (Invoke-WebRequest 'https://raw.githubusercontent.com/ScoopInstaller/Scoop/master/schema.json').Content
if (-not (Test-Json -Json $taskJson -Schema $taskSchema)) { throw 'Manifiesto de Scoop inválido.' }

$taskArchivePath = [IO.Path]::GetTempFileName()
try {
    Invoke-WebRequest $taskArchiveUrl -OutFile $taskArchivePath
    if ((Get-FileHash -LiteralPath $taskArchivePath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $taskHash) {
        throw 'El ZIP descargado no coincide con SHA256SUMS.'
    }
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $taskArchive = [IO.Compression.ZipFile]::OpenRead($taskArchivePath)
    try {
        if (@($taskArchive.Entries.FullName) -notcontains 'ko-calendar.exe') {
            throw 'El ZIP no contiene ko-calendar.exe.'
        }
    }
    finally { $taskArchive.Dispose() }
}
finally { Remove-Item -LiteralPath $taskArchivePath -Force }

if ($Update) {
    $taskJson = [regex]::Replace($taskJson, '(?m)^ +', { param($match) $match.Value + $match.Value })
    [IO.File]::WriteAllText($taskManifestPath, "$taskJson`n", [Text.UTF8Encoding]::new($false))
}
Write-Output "Scoop $($taskManifest.version): manifiesto y descarga verificados."
