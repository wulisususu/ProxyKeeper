# ProxyKeeper

轻量级 Windows 系统代理守护工具。Rust 编写，单文件 exe（≈180KB），常驻内存 ≈2.5MB，CPU 占用 ≈0%。

## 解决什么问题

使用 Clash / V2Ray 等代理客户端时，系统代理的"绕过列表"（不走代理的地址）会被客户端反复覆盖，手动在 Windows 设置里加的条目重启就丢。ProxyKeeper：

- **守护**：注册表被代理客户端覆盖后 ~300ms 内自动恢复
- **托管**：`bypass.txt` 是唯一配置源，改文件即生效，不用再进 Windows 设置

## 功能

- 托盘常驻（绿底白勾图标），**左键点击**：自动读剪贴板，从 URL 提取主机名加入直连列表，气泡通知结果
  - `https://115.190.225.138:38443/-/user/ssh_keys/4` → 添加 `115.190.225.138`（自动去端口/路径，端口不需要，绕过匹配只看主机）
  - 自动去重、拒绝非 ASCII/含空格的无效条目
- **右键菜单**：打开 bypass.txt / 退出
- **bypass.txt 热更新**：一行一个地址，保存后 ~2 秒自动写入注册表并即时生效
- **开机自启**（HKCU Run，无需管理员）+ 单实例保护
- 内网段 / `localhost` 默认已内置

## 快速安装

前提：[Rust 工具链](https://rustup.rs)（MSVC 目标）

```powershell
git clone https://github.com/wulisususu/ProxyKeeper.git
cd ProxyKeeper
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

`install.ps1` 一条龙：编译 release → 注册开机自启 → 启动。完成后托盘右下角出现绿色对勾图标（可能在 `^` 隐藏区，可拖出常驻）。

手动安装（等价）：

```powershell
git clone https://github.com/wulisususu/ProxyKeeper.git
cd ProxyKeeper
cargo build --release
reg add "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v ProxyKeeper /t REG_SZ /d "%cd%\target\release\proxy-keeper.exe" /f
start target\release\proxy-keeper.exe
```

## 配置：bypass.txt

与 exe 同目录（`target\release\bypass.txt`），首次启动自动生成默认列表：

```
localhost
127.*
192.168.*
10.*
172.16.*
...(172.17 ~ 172.31)
www.openkylin.top
115.190.225.138
```

- **加直连地址**：加一行（或直接点托盘图标粘贴 URL）
- **去掉直连地址**：删掉那一行
- `#` 开头为注释；保存后 ~2 秒生效
- 改坏了（删空）会自动回退内置默认列表
- 唯一配置源：在 Windows 设置里手动改会被守护线程改回去，属于预期行为

## 命令行模式（可选）

```powershell
proxy-keeper.exe add https://example.com/some/path   # 提取 example.com 并加入列表
proxy-keeper.exe add                                  # 无参数时读剪贴板
```

可在脚本/快捷方式里复用，与托盘点击走同一逻辑。

## 卸载

```powershell
taskkill /IM proxy-keeper.exe /F
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v ProxyKeeper /f
# 然后删除项目目录
```

## 实现说明

- 注册表：`HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings`（`ProxyEnable` / `ProxyServer` / `ProxyOverride`），仅 HKCU，不需要管理员
- 监听：`RegNotifyChangeKeyValue` 阻塞式监听 + `bypass.txt` mtime 轮询（2s）
- 生效：写入后调用 `InternetSetOption(SETTINGS_CHANGED | REFRESH)` 即时通知系统
- 日志：exe 同目录 `proxy-keeper.log`（仅记录异常/恢复动作）

仅支持 Windows 10/11。
