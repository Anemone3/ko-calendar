param(
    [string]$Toolchain = '',
    [string]$GoogleClientFile = ''
)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path -Parent $PSScriptRoot
$taskOriginalClientId = $env:KOKONUT_GOOGLE_CLIENT_ID
$taskOriginalClientSecret = $env:KOKONUT_GOOGLE_CLIENT_SECRET
try {
    if ($GoogleClientFile) {
        try {
            $taskClient = Get-Content -LiteralPath $GoogleClientFile -Raw | ConvertFrom-Json
        }
        catch { throw 'No se pudo leer el JSON de Google. Usa el archivo descargado para el cliente de escritorio.' }
        if (-not $taskClient.installed -or $taskClient.installed.client_id -isnot [string]) {
            throw 'El JSON debe pertenecer a un cliente OAuth de tipo Aplicación de escritorio (installed). No uses un cliente web.'
        }
        if ($null -ne $taskClient.installed.client_secret -and $taskClient.installed.client_secret -isnot [string]) {
            throw 'El JSON del cliente de escritorio no contiene un formato válido.'
        }
        $env:KOKONUT_GOOGLE_CLIENT_ID = $taskClient.installed.client_id
        $env:KOKONUT_GOOGLE_CLIENT_SECRET = $taskClient.installed.client_secret
    }
    if (-not $env:KOKONUT_GOOGLE_CLIENT_ID -or $env:KOKONUT_GOOGLE_CLIENT_ID -notmatch '^[A-Za-z0-9._-]+\.apps\.googleusercontent\.com$') {
        throw 'Configura una vez el cliente OAuth de escritorio de ko-calendar con -GoogleClientFile o KOKONUT_GOOGLE_CLIENT_ID antes de empaquetar. En GitHub Actions viene del secreto KOKONUT_GOOGLE_CLIENT_ID del entorno release.'
    }
    Push-Location -LiteralPath $taskRoot
    try {
        if (-not $Toolchain -and (Get-Command gcc -ErrorAction SilentlyContinue)) {
            if ((rustup toolchain list) -match 'stable-x86_64-pc-windows-gnu') { $Toolchain = 'stable-x86_64-pc-windows-gnu' }
        }
        $taskArgs = @()
        if ($Toolchain) { $taskArgs += "+$Toolchain" }
        $taskArgs += @('build', '--locked', '--release')
        & cargo @taskArgs
        if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación de ko-calendar.' }
        $taskDist = Join-Path $taskRoot 'dist'
        New-Item -ItemType Directory -Path $taskDist -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $taskRoot 'target/release/ko-calendar.exe') -Destination (Join-Path $taskDist 'ko-calendar.exe') -Force
        Copy-Item -LiteralPath (Join-Path $taskRoot 'LICENSE') -Destination (Join-Path $taskDist 'LICENSE') -Force
        $taskSounds = Join-Path $taskDist 'sounds'
        New-Item -ItemType Directory -Path $taskSounds -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $taskRoot 'assets/sounds/ko-tify.mp3') -Destination (Join-Path $taskSounds 'ko-tify.mp3') -Force
        Copy-Item -LiteralPath (Join-Path $taskRoot 'assets/sounds/alarm.mp3') -Destination (Join-Path $taskSounds 'alarm.mp3') -Force
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $taskArchiveStream = [IO.File]::Create((Join-Path $taskDist 'ko-calendar-windows-x64.zip'))
        $taskArchive = [IO.Compression.ZipArchive]::new($taskArchiveStream, [IO.Compression.ZipArchiveMode]::Create)
        try {
            foreach ($taskEntry in @('ko-calendar.exe', 'LICENSE', 'sounds/ko-tify.mp3', 'sounds/alarm.mp3')) {
                [IO.Compression.ZipFileExtensions]::CreateEntryFromFile($taskArchive, (Join-Path $taskDist $taskEntry), $taskEntry) | Out-Null
            }
        }
        finally {
            $taskArchive.Dispose()
            $taskArchiveStream.Dispose()
        }
        Write-Output (Join-Path $taskDist 'ko-calendar-windows-x64.zip')
    }
    finally { Pop-Location }
}
finally {
    $env:KOKONUT_GOOGLE_CLIENT_ID = $taskOriginalClientId
    $env:KOKONUT_GOOGLE_CLIENT_SECRET = $taskOriginalClientSecret
}
