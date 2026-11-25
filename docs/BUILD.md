# Windows 11 x64 构建

## 环境

- Node.js 18+、npm。
- Rust stable，`x86_64-pc-windows-msvc`，Cargo 在 `%USERPROFILE%\.cargo\bin`。
- Visual Studio 2022 Build Tools：“使用 C++ 的桌面开发”，包括 MSVC v143、Windows 11 SDK。
- Microsoft Edge WebView2 Runtime。
- Apple Bonjour 服务（AirPlay DNS-SD 服务发现）。UxPlay 通过系统的 `dnssd.dll` 加载此服务。本项目不修改或复用其他应用的 Bonjour 配置。
- 独立安装的 MSYS2 UCRT64，默认路径 `.tools/msys64`。本机使用官方 `msys2-base-x86_64-20261120.tar.xz` 解压创建，没有使用同级项目工具链。

在 UCRT64 shell 安装原生依赖：

```sh
pacman -Syu
pacman -S --needed mingw-w64-ucrt-x86_64-gcc mingw-w64-ucrt-x86_64-cmake mingw-w64-ucrt-x86_64-ninja mingw-w64-ucrt-x86_64-pkgconf mingw-w64-ucrt-x86_64-gstreamer mingw-w64-ucrt-x86_64-gst-plugins-base mingw-w64-ucrt-x86_64-gst-plugins-good mingw-w64-ucrt-x86_64-gst-plugins-bad mingw-w64-ucrt-x86_64-gst-libav mingw-w64-ucrt-x86_64-libplist mingw-w64-ucrt-x86_64-openssl
```

`.cargo/config.toml` 对本项目使用 rsproxy 国内镜像；其他网络可移除 source replacement，恢复 crates.io。npm 安装可通过命令行指定 registry，不修改系统 npm 配置。

## 开发与测试

```powershell
$env:PATH="$env:USERPROFILE\.cargo\bin;$env:PATH"
npm ci
npm run native:build
npm run desktop
npm test
```

仅预览界面用 `npm run dev`。浏览器预览不会模拟连接成功，也无法接收手机。

原生接收端使用 MSYS2/GCC，Tauri 使用 Rust/MSVC；两者仅通过 JSON Lines 进程管道和窗口句柄通信，不跨编译器传递 C++ ABI 对象。

自定义 MSYS2 安装位置：

```powershell
.\scripts\Build-Native.ps1 -MsysRoot C:\msys64
```

上游版本固定为 UxPlay v1.73.6（提交与源码哈希位于 `dependencies/`）。已提交的 vendor 源码包含最小桥接修改；重新获取官方源码后运行 `python scripts/patch-upstream.py`。该脚本幂等并会检查原始锚点。

## 打包

```powershell
npm run package
```

输出 NSIS 安装包到 `artifacts/`。运行库从本项目工具链按 DLL 导入递归收集，GStreamer 插件采用固定列表；缺少 DLL 或插件时构建失败。运行库文件 SHA-256 和 MSYS2 软件包版本写入 `dependencies/runtime-lock.json`、`dependencies/msys2-packages.txt`。

打包工具缓存保存在本项目 `src-tauri/target/.tauri`，不依赖其他项目。打包同时生成源码压缩包，排除工具链、二进制运行库、缓存、日志和 AirPlay 密钥。

安装目标需要 WebView2 与 Bonjour。Tauri 安装器可引导 WebView2 安装；Bonjour 需用户自行从 Apple 官方来源安装。应用不会自动修改防火墙或系统安全设置。

如需允许开发版的私有网络入站，可自行在管理员 PowerShell 执行 `scripts/Configure-Network.ps1`。规则仅允许指定接收端程序，不关闭防火墙。安装版程序路径不同，需使用对应的系统防火墙提示或规则。
