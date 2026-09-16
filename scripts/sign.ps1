# Authenticode signing used by `tauri build` (bundle.windows.signCommand).
# Signs our own binaries with the project certificate and leaves the WinDivert
# files untouched: the kernel driver carries Microsoft's attestation signature
# and re-signing it would stop it from loading.
param([Parameter(Mandatory = $true)][string]$File)

$ErrorActionPreference = "Stop"
$thumbprint = "7E92BB10BEE8A1C0FA951039EAB035B475FD00BE"

if ((Split-Path $File -Leaf) -like "WinDivert*") {
    Write-Host "sign.ps1: skipping third-party file $File"
    exit 0
}

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
    Sort-Object FullName | Select-Object -Last 1 -ExpandProperty FullName
if (-not $signtool) { $signtool = (Get-Command signtool.exe -ErrorAction Stop).Source }

& $signtool sign /sha1 $thumbprint /fd sha256 /tr http://timestamp.digicert.com /td sha256 /d "Bandwidth Limiter" $File
if ($LASTEXITCODE -ne 0) {
    # Timestamp servers are flaky; retry once without a timestamp rather than fail the build.
    Write-Warning "sign.ps1: signing with timestamp failed, retrying without timestamp"
    & $signtool sign /sha1 $thumbprint /fd sha256 /d "Bandwidth Limiter" $File
}
exit $LASTEXITCODE
