# Backs out install-windows.ps1: removes the installed .exe and its PATH entry.
#
#   powershell -ExecutionPolicy Bypass -File uninstall-windows.ps1

param(
    [string]$Name = "gemdigger"
)

$ErrorActionPreference = "Stop"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\$Name"

if (Test-Path $installDir) {
    Remove-Item -Recurse -Force $installDir
    Write-Host "==> Removed $installDir"
}

# NOTE: raw registry access for the same reason as install-windows.ps1: keep %VAR% entries intact.
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Environment", $true)
try {
    $raw = $key.GetValue("Path", "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $entries = @($raw -split ";" | Where-Object { $_ })
    if ($entries -contains $installDir) {
        $key.SetValue("Path", (($entries | Where-Object { $_ -ne $installDir }) -join ";"), [Microsoft.Win32.RegistryValueKind]::ExpandString)
        [Environment]::SetEnvironmentVariable("WINDOWS_EXE_TEMPLATE_REFRESH", $null, "User")
        Write-Host "==> Removed $installDir from your PATH"
    }
} finally {
    $key.Close()
}
