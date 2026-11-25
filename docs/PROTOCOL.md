# 本地接收端接口 v1

父进程启动 `cast-receiver.exe --parent <HWND> --name <UTF-8 name>`；`--test` 使用明确标注的 1080p/60fps 测试源，`--software` 禁用硬件解码。父进程用 Windows Job 管理接收端，父进程异常退出会结束子进程。

stdin/stdout 是 UTF-8 JSON Lines，每行一个对象，包含 `v: 1`。stderr 写入日志；UxPlay 的 stdout 也重定向到日志，不混入协议。协议版本不匹配和无效 JSON 不更新应用状态。

| 命令 | 参数 | 行为 |
| --- | --- | --- |
| `viewport` | `rect: {x,y,width,height,scale}` | 前端 CSS 像素与实际 DPI；等比嵌入画面 |
| `relayout` | 无 | 父窗口尺寸改变后重新布局 |
| `visible` | `visible: boolean` | 打开 Tauri 弹窗时隐藏原生画面并释放按键 |
| `focus` | `enabled: boolean` | 专注模式覆盖父窗口客户区 |
| `audio` | `volume: 0..1, muted: boolean` | 音量、静音 |
| `stop` | 无 | 停止接收、释放输入、结束进程 |

事件通过 `event` 字段区分：`ready`、`video`（width/height）、`stats`（receivedFps/presentedFps/dropped）、`audio`（state）、`disconnected`、`error`/`warning`（message）、`decoder-fallback`、`pointer`（kind/x/y/wheel）、`focus`。

指针坐标 x/y 是原生实际视频区域中的 0..1 归一化坐标；DPI 与留空区域在原生层处理。HID 使用 0..32767 绝对轴，定位后独立发送按钮报告。移动可合并，按下/抬起不得丢弃。失焦、隐藏、停止与断连发送释放报告。

0.1.1 的 `pointer` 事件增加可选 `dx`、`dy`，单位为按 Windows DPI 换算的相对鼠标计数。悬停和点击起点走绝对报告；按住拖动走同时包含按钮与非零 dx/dy 的相对报告。合并移动事件必须累加位移，不能只保留最后一个 delta。超过单份报告范围的移动拆成 ±127 内的多份报告；DPI 换算的小数余量累计到后续移动。静止按住时每 100ms 重发按钮报告，松开时先发送尚未处理的最后一段位移，再释放按钮。视频子窗口获得焦点不视为离开应用。

UI 到 Rust 的命令包括 `get_status`、`start_receiver`、`stop_receiver`、`start_bluetooth`、`stop_bluetooth`、`set_viewport`、`set_audio`、`set_focus`、`set_video_visible`、`export_diagnostics`。状态通过 `cast-status` 事件发送。所有控制接口仅在本应用 IPC 上开放，不监听局域网 HTTP 端口。
