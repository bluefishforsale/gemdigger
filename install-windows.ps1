# Windows entry point. Downloads a release .exe, checks it against the
# release's SHA256SUMS, and puts it on the user PATH. Safe to rerun; it
# upgrades in place.
#
#   powershell -ExecutionPolicy Bypass -File install-windows.ps1 [-Version v0.1.0]

param(
    [string]$Version = "",
    [string]$Repo = "bluefishforsale/gemdigger",
    [string]$Name = "gemdigger"
)

$ErrorActionPreference = "Stop"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\$Name"
$exe = "$Name.exe"

$base = if ($Version) { "https://github.com/$Repo/releases/download/$Version" } else { "https://github.com/$Repo/releases/latest/download" }

$tmp = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
New-Item -ItemType Directory $tmp | Out-Null
try {
    foreach ($file in @($exe, "SHA256SUMS")) {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/$file" -OutFile (Join-Path $tmp $file)
    }

    $line = Get-Content (Join-Path $tmp "SHA256SUMS") | Where-Object { $_ -match "^[0-9a-f]{64}  $([regex]::Escape($exe))$" }
    if (-not $line) { throw "SHA256SUMS has no entry for $exe" }
    $expected = $line.Substring(0, 64)
    $actual = (Get-FileHash (Join-Path $tmp $exe) -Algorithm SHA256).Hash
    if ($actual -ne $expected) { throw "checksum mismatch for ${exe}: expected $expected, got $actual" }

    New-Item -ItemType Directory -Force $installDir | Out-Null
    Copy-Item -Force (Join-Path $tmp $exe) $installDir
} finally {
    Remove-Item -Recurse -Force $tmp
}

# NOTE: read and write the raw registry value. [Environment]::GetEnvironmentVariable
# expands it and SetEnvironmentVariable writes REG_SZ, which would freeze every
# %VAR% entry already on the user's PATH.
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Environment", $true)
try {
    $raw = $key.GetValue("Path", "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $entries = @($raw -split ";" | Where-Object { $_ })
    if ($entries -notcontains $installDir) {
        $key.SetValue("Path", (($entries + $installDir) -join ";"), [Microsoft.Win32.RegistryValueKind]::ExpandString)
        # Clearing an unset variable makes .NET broadcast WM_SETTINGCHANGE, so new terminals see the PATH.
        [Environment]::SetEnvironmentVariable("WINDOWS_EXE_TEMPLATE_REFRESH", $null, "User")
        Write-Host "==> Added $installDir to your PATH. Open a new terminal to use it."
    }
} finally {
    $key.Close()
}

# The game is a GUI exe, so `& exe` wouldn't wait or show its output.
$out = New-TemporaryFile
Start-Process (Join-Path $installDir $exe) -ArgumentList "--version" -Wait -NoNewWindow -RedirectStandardOutput $out
Write-Host "==> Installed $((Get-Content $out -Raw).Trim())"
Remove-Item $out
