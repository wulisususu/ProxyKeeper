# ProxyKeeper

轻量级 Windows 系统代理**绕过列表**守护工具。Rust 编写，单文件 exe，面向固定本地代理 `127.0.0.1:7897` 的使用环境。

## 解决什么问题

使用 Clash / V2Ray 等代理客户端时，Windows 的系统代理绕过列表（`ProxyOverride`）可能被客户端反复覆盖，手动添加的直连地址会丢失。

ProxyKeeper 只负责守护 `ProxyOverride`：

- **不会强制开启系统代理**
- **不会修改 `ProxyServer`**
- 固定预期代理地址仍为 `127.0.0.1:7897`；如果当前环境不是这个地址，只写日志提醒，不抢配置
- `ProxyOverride` 被外部覆盖后，收到注册表通知并等待约 300ms 后自动恢复
- `bypass.txt` 是绕过列表的唯一配置源

这样代理客户端仍然负责“开不开代理、代理监听在哪”，ProxyKeeper 只负责“哪些地址直连”。

## 功能

- 托盘常驻（绿底白勾图标）
- **左键点击**：读取剪贴板，从 URL / 主机地址中提取合法 host，加入直连列表并立即应用
  - `https://115.190.225.138:38443/-/user/ssh_keys/4` → `115.190.225.138`
  - `https://example.com/path` → `example.com`
  - `http://[2001:db8::1]:8080/path` → `[2001:db8::1]`
  - 支持 IPv4 / IPv6 / 域名 / 合法端口
  - 会拒绝普通单词、Windows 路径、非法端口、非法 hostname、非 HTTP(S) URL
- **右键菜单**
  - 打开 `bypass.txt`
  - 退出并恢复 ProxyKeeper 启动前的 `ProxyOverride`
- **bypass.txt 热更新**：保存后约 2 秒重新应用
- **开机自启**（HKCU Run，无需管理员权限）
- GUI 单实例保护
- 内网段 / `localhost` 默认内置
- 首次运行会保存原始 `ProxyOverride` 到运行时状态文件，正常退出或卸载时恢复

## 快速安装

前提：[Rust 工具链](https://rustup.rs)（MSVC 目标）

```powershell
git clone https://github.com/wulisususu/ProxyKeeper.git
cd ProxyKeeper
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

`install.ps1` 会：

1. 编译 release
2. 注册当前用户开机自启
3. 启动 ProxyKeeper

完成后托盘右下角会出现绿色对勾图标。

## 固定代理端口

本项目的目标环境固定使用：

```text
127.0.0.1:7897
```

代码中仍保留该固定值。

但 ProxyKeeper **不会主动把系统代理改成 7897，也不会重新打开已经关闭的系统代理**。它只检查当前值是否符合预期；不符合时写入 `proxy-keeper.log`。

这样可以避免 ProxyKeeper 与 Clash / V2Ray 等代理客户端互相抢 `ProxyEnable` / `ProxyServer`。

## 配置：bypass.txt

与 exe 同目录（默认构建位置为 `target\release\bypass.txt`），首次启动自动生成默认列表：

```text
localhost
127.*
192.168.*
10.*
172.16.*
...(172.17 ~ 172.31)
www.openkylin.top
115.190.225.138
```

规则：

- 一行一个直连地址
- `#` 开头为注释
- 保存后约 2 秒生效
- 有效配置最终会自动附加 `<local>`
- 文件为空或没有有效条目时，会回退到内置默认列表
- 在 Windows 设置里手动修改 `ProxyOverride`，运行中的 ProxyKeeper 会恢复为 `bypass.txt` 内容
- 修改 `ProxyEnable` 或 `ProxyServer` 不会被 ProxyKeeper 改回去

## 剪贴板地址解析

托盘左键和 `add` 命令使用同一套解析逻辑。

接受示例：

```text
https://example.com/path
example.com:8080
127.0.0.1:3000
http://[2001:db8::1]:8080/path
2001:db8::1
https://gitlab/path
```

拒绝示例：

```text
hello
C:\Users\test
abc:def
example.com:99999
https://-bad.example.com
ftp://example.com/file
```

IPv6 写入 `ProxyOverride` 时会统一规范为方括号形式，例如：

```text
[2001:db8::1]
```

## 命令行模式

添加直连地址：

```powershell
proxy-keeper.exe add https://example.com/some/path
proxy-keeper.exe add
```

第二种形式会读取剪贴板。

恢复 ProxyKeeper 启动前保存的 `ProxyOverride`：

```powershell
proxy-keeper.exe restore
```

## 退出与恢复

第一次开始接管 `ProxyOverride` 前，ProxyKeeper 会记录原来的值。

从托盘选择：

```text
退出并恢复原始绕过设置
```

会：

1. 停止守护线程继续回写
2. 恢复原来的 `ProxyOverride`
3. 刷新 WinINet 设置
4. 删除运行时状态文件
5. 退出

`ProxyEnable` 和 `ProxyServer` 从始至终不会被 ProxyKeeper 修改，因此不需要恢复。

## 卸载

推荐：

```powershell
powershell -ExecutionPolicy Bypass -File .\uninstall.ps1
```

脚本会：

1. 停止正在运行的 ProxyKeeper
2. 调用 `proxy-keeper.exe restore` 恢复原始 `ProxyOverride`
3. 删除 HKCU 开机自启项

然后删除项目目录即可。

## 实现说明

- 注册表：
  - `HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings\ProxyOverride`：读写并守护
  - `ProxyEnable`：只读诊断
  - `ProxyServer`：只读诊断，固定预期值 `127.0.0.1:7897`
- 注册表监听：`RegNotifyChangeKeyValue`
- 外部变更 debounce：约 300ms
- `bypass.txt` 更新：mtime 轮询，约 2 秒
- 生效：`InternetSetOption(SETTINGS_CHANGED | REFRESH)`
- 日志：exe 同目录 `proxy-keeper.log`
- 原始状态：exe 同目录 `proxy-keeper.state`，属于运行时文件，不提交 Git

仅支持 Windows 10/11。
