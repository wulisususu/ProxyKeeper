# 构建并注册开机自启（无需管理员权限）
$ErrorActionPreference = "Stop"

cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Host "编译失败"; exit 1 }

$exe = (Resolve-Path "target\release\proxy-keeper.exe").Path
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v ProxyKeeper /t REG_SZ /d "$exe" /f | Out-Null

# 已在运行则先退出
taskkill /IM proxy-keeper.exe /F 2>$null | Out-Null
Start-Sleep -Milliseconds 500
Start-Process -FilePath $exe

Write-Host ""
Write-Host "ProxyKeeper 安装完成："
Write-Host "  exe 路径   : $exe"
Write-Host "  开机自启   : 已注册 (HKCU\...\Run)"
Write-Host "  托盘图标   : 绿色对勾，在任务栏右下角（可能在 ^ 隐藏区）"
Write-Host "  卸载命令   : taskkill /IM proxy-keeper.exe /F; reg delete \"HKCU\Software\Microsoft\Windows\CurrentVersion\Run\" /v ProxyKeeper /f"
