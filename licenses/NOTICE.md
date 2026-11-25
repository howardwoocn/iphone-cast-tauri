# 第三方来源

本项目按 GPL-3.0-or-later 提供，根目录 `LICENSE` 与 `UxPlay-GPL-3.0.txt` 包含完整许可文本。

- **UxPlay v1.73.6**，FDH2/UxPlay，提交 `21eef8df25d91e12635c36d8176ad192725baca2`。完整接收端源码在 `vendor/UxPlay`，原有版权与子组件许可保留。修改点由 `scripts/patch-upstream.py` 记录。
- **nlohmann/json v3.11.3**，MIT，单头文件保留版权声明，许可见 `nlohmann-json-MIT.txt`。
- **Apple mDNSResponder DNS-SD 头文件 2200.140.11**，源文件包含 BSD-3-Clause 声明。本项目不包含 Apple Bonjour 的二进制安装包。
- **iPhone Use**，MIT，固定参考提交 `bc0fe36e78ec5a3af095c813574e3a8b7bbea4dd`。参考其绝对指针定位和独立鼠标按钮报告的协议思路，Windows 实现独立编写；附参考项目许可。
- **GStreamer、GLib、FFmpeg、OpenSSL、libplist 及相关运行库**：从独立 MSYS2 官方软件包获取；精确版本记录在 `dependencies/msys2-packages.txt`，实际分发文件哈希记录在 `dependencies/runtime-lock.json`。收集脚本同时复制工具链的许可文件到 `licenses/runtime`。
- **Tauri、React、Vite、TypeScript、Lucide**：版本固定于 npm/Cargo 锁文件，各包保留其开源许可。

上游与依赖地址汇总见 `dependencies/upstream.json`。MSYS2 包构建配方和源代码信息可在 https://github.com/msys2/MINGW-packages 按锁定版本查找。对外再分发时需同时提供本项目对应源码、修改记录及各依赖许可要求的源代码材料。
