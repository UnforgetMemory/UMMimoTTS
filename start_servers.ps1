$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = "C:\aapp\development\source_code\UMMimoTTS\backend\target\release\um-mimo-tts-server.exe"
$psi.Arguments = "--port 30231"
$psi.WorkingDirectory = "C:\aapp\development\source_code\UMMimoTTS"
$psi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$psi.CreateNoWindow = $true
$psi.UseShellExecute = $false
[System.Diagnostics.Process]::Start($psi) | Out-Null
Write-Host "Backend started, waiting 8s..."
Start-Sleep -Seconds 8

$vitePsi = New-Object System.Diagnostics.ProcessStartInfo
$vitePsi.FileName = "cmd.exe"
$vitePsi.Arguments = "/c cd /d C:\aapp\development\source_code\UMMimoTTS\frontend && npm run dev"
$vitePsi.WindowStyle = [System.Diagnostics.ProcessWindowStyle]::Hidden
$vitePsi.CreateNoWindow = $true
$vitePsi.UseShellExecute = $false
[System.Diagnostics.Process]::Start($vitePsi) | Out-Null
Write-Host "Vite started"
