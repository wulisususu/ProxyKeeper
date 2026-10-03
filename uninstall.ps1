# 停止 ProxyKeeper、恢复启动前的 ProxyOverride，并移除开机自启
$ErrorActionPreference = "Continue"

$exe = Join-Path $PSScriptRoot "target\release\proxy-keeper.exe"

taskkill /IM proxy-keeper.exe /F 2>$null | Out-Null
Start-Sleep -Milliseconds 300

if (Test-Path $exe) {
    Start-Process -FilePath $exe -ArgumentList "restore" -Wait
}

reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v ProxyKeeper /f 2>$null | Out-Null

Write-Host ""
Write-Host "ProxyKeeper 已停止并移除开机自启。"
Write-Host "如果存在安装前的 ProxyOverride，已尝试恢复。"
Write-Host "如需彻底删除，请再删除项目目录。"
