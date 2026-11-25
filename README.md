# 澄镜 · iPhone Cast

独立开发的 Windows 11 x64 iPhone 无线投屏工具。界面采用 Tauri 2 + React，音视频通过局域网 AirPlay 接收，鼠标输入通过 Windows BLE HID 发送。

## 运行

构建后的安装包在 `artifacts/`，开发版入口为：

```powershell
npm ci
npm run native:build
npm run desktop
```

首次构建需安装 Rust MSVC、Visual Studio C++ Build Tools、Windows SDK、WebView2，以及本项目独立的 MSYS2 UCRT64 工具链。完整说明见 [构建说明](docs/BUILD.md)。

## 使用

1. 手机与电脑连接同一可互通局域网，打开应用并点击“开始接收投屏”。
2. 在 iPhone 控制中心打开“屏幕镜像”，选择“澄镜 iPhone Cast”。
3. 开启鼠标控制，在手机蓝牙列表中配对电脑，在“设置 → 辅助功能 → 触控 → 辅助触控”中开启辅助触控。
4. F11 进入专注模式：无边框、无操作栏，窗口贴合手机比例。Esc 返回。按住 Alt 拖动移动窗口，Alt + 拖动边角等比缩放；普通点击覆盖完整手机画面。

声音输出到 Windows 默认播放设备。画面显示区域支持移动、点击、长按、拖动和滚轮；不支持键盘和多点触控。

0.1.1 修复按住拖动：点击起点继续使用绝对定位，按住移动时使用包含按钮状态的相对鼠标报告，静止长按每 100ms 保活。拖动幅度会受 iPhone 辅助触控的跟踪灵敏度影响；它不保证与电脑指针逐像素重合。

## 验证范围

- 60fps 是接收端请求的目标。面板统计实际接收/呈现帧率，不用静止画面的重复呈现凑帧率。
- 准确点击采用绝对指针定位与独立鼠标按钮报告，不以相对位移模拟准确定位。
- **iPhone 17 / iOS 27 的准确点击、横竖屏控制与真实音视频仍需真机验收。**本机测试源不能代替真机验证。结果见 [验证记录](docs/VALIDATION.md)。
- 蓝牙控制需要电脑支持 BLE 外设模式。蓝牙不可用时可继续接收画面和声音。
- AirPlay 服务发现依赖 Apple Bonjour 系统服务；本机已安装，分发包不包含 Apple 安装程序。受保护内容、应用限制及网络隔离可能阻止画面或音频传输。

## 项目结构

- `src/`：中文界面、连接引导、声音控制与状态展示。
- `src-tauri/`：Rust 生命周期、IPC、Windows BLE HID、窗口与诊断。
- `native/`：原生 Direct3D 11 视频区域、GStreamer 桥接、坐标处理与测试。
- `vendor/UxPlay/`：从官方固定版本获取的接收端，保留源码与最小桥接修改。
- `scripts/`：独立构建、运行库收集、测试、网络配置与打包。

本项目不依赖同级其他项目的源码、工具链或构建产物。GPL-3.0-or-later；第三方来源及许可见 [licenses/NOTICE.md](licenses/NOTICE.md)。
