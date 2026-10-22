param([int]$ParentProcess, [string]$Installer, [string]$Executable, [string]$InstallDirectory, [string]$Cache)
$ErrorActionPreference = 'Stop'
try {
    Wait-Process -Id $ParentProcess -Timeout 120 -ErrorAction SilentlyContinue
    if (Get-Process -Id $ParentProcess -ErrorAction SilentlyContinue) { throw 'Amigo is still running.' }
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/CLOSEAPPLICATIONS', ('/DIR="' + $InstallDirectory + '"'))
    $process = Start-Process -FilePath $Installer -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw ('Installer exited with code ' + $process.ExitCode) }
    Start-Process -FilePath $Executable
    Remove-Item -LiteralPath $Cache -Recurse -Force
} catch {
    $env:AMIGO_UPDATE_ERROR = 'The update did not complete. The previous installation is available. Retry from Settings > Updates.'
    if (Test-Path -LiteralPath $Executable) { Start-Process -FilePath $Executable }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'Amigo Update') | Out-Null
}
