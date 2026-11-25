import { useCallback, useEffect, useRef, useState } from 'react';
import { Activity, ArrowDownToLine, Bluetooth, Check, CircleHelp, Expand, Monitor, Play, Radio, Settings2, Smartphone, Square, Volume2, VolumeX, Wifi, X } from 'lucide-react';
import { CastStatus, command, desktop, initialStatus, subscribe } from './bridge';
import metadata from '../package.json';
import SpeakerView from './SpeakerView';

const receiverLabels = { stopped: '未连接', starting: '正在启动', listening: '等待 iPhone', streaming: '投屏中', error: '连接异常' };
const btLabels = { off: '未开启', starting: '正在开启', advertising: '等待配对', connected: '已连接', error: '不可用' };

export default function App() {
  const [status, setStatus] = useState<CastStatus>(initialStatus);
  const [mode, setMode] = useState<'cast' | 'speaker'>('cast');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [dialog, setDialog] = useState<'help' | 'settings' | null>(null);
  const [name, setName] = useState(initialStatus.receiverName);
  const viewport = useRef<HTMLDivElement>(null);
  const active = status.receiver === 'listening' || status.receiver === 'streaming';
  const run = useCallback(async (operation: () => Promise<unknown>) => {
    setError(''); setBusy(true);
    try { await operation(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  }, []);

  useEffect(() => {
    let disposed = false; let cleanup: (() => void) | undefined;
    let first = true;
    subscribe(value => { if (!disposed) { setStatus(value); if (first) { setName(value.receiverName); first = false; } } }).then(fn => { if (disposed) fn(); else cleanup = fn; }).catch(e => setError(String(e)));
    return () => { disposed = true; cleanup?.(); };
  }, []);

  useEffect(() => {
    const sync = () => {
      if (!desktop || mode !== 'cast' || !viewport.current || dialog || status.focus) return;
      const r = viewport.current.getBoundingClientRect();
      command('set_viewport', { rect: { x: r.x, y: r.y, width: r.width, height: r.height, scale: devicePixelRatio } }).catch(e => setError(String(e)));
    };
    const observer = new ResizeObserver(sync);
    if (viewport.current) observer.observe(viewport.current);
    window.addEventListener('resize', sync); sync();
    return () => { observer.disconnect(); window.removeEventListener('resize', sync); };
  }, [dialog, mode, status.focus, status.receiver]);

  useEffect(() => {
    if (desktop) command('set_video_visible', { visible: !dialog && mode === 'cast' }).catch(e => setError(String(e)));
  }, [dialog, mode, status.receiver]);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.key === 'F11' && mode === 'cast') || (e.key === 'Escape' && status.focus)) {
        e.preventDefault(); run(() => command('set_focus', { enabled: !status.focus }));
      } else if (e.key === 'Escape') setDialog(null);
    };
    window.addEventListener('keydown', key);
    return () => window.removeEventListener('keydown', key);
  }, [mode, run, status.focus]);

  const switchMode = (next: 'cast' | 'speaker') => {
    if (next === mode || busy) return;
    run(async () => {
      if (desktop) {
        if (next === 'speaker' && status.receiver !== 'stopped') await command('stop_receiver');
        if (next === 'cast') await command('disconnect_speaker');
      }
      setMode(next);
    });
  };

  if (status.focus) return <div className="focus-surface" aria-label="手机专注模式，按 Esc 返回" />;

  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><Radio size={23} /></span><strong>澄镜</strong></div>
      <button className={`nav-item mode-nav ${mode === 'cast' ? 'selected' : ''}`} disabled={busy} onClick={() => switchMode('cast')}><Smartphone size={22} /><span>手机投屏</span></button>
      <button className={`nav-item mode-nav ${mode === 'speaker' ? 'selected' : ''}`} disabled={busy} onClick={() => switchMode('speaker')}><Bluetooth size={22} /><span>蓝牙音箱</span></button>
      <div className="sidebar-bottom">
        <button className="nav-item utility-nav" aria-label="连接帮助" title="连接帮助" onClick={() => setDialog('help')}><CircleHelp size={20} /></button>
        <button className="nav-item utility-nav" aria-label="设置" title="设置" onClick={() => setDialog('settings')}><Settings2 size={20} /></button>
      </div>
    </aside>

    <main>
      {mode === 'speaker' ? <SpeakerView cast={status} onBusy={setBusy} /> : <>
      <div className="mode-layout">
      <header><div className="header-title"><h1>手机投屏</h1>{!desktop && <span className="preview-notice">界面预览 · 桌面应用可使用全部功能</span>}</div><div className="header-actions"><button className="icon-button" aria-label="专注模式" title="专注模式 · F11" disabled={!active} onClick={() => run(() => command('set_focus', { enabled: true }))}><Expand size={18} /></button></div></header>
      <section className="workspace">
        <div className="display-card">
          <div className={`viewport ${status.receiver === 'streaming' ? 'has-stream' : ''}`} ref={viewport}>
            {status.receiver !== 'streaming' && <div className="empty-display">
              <div className="device-illustration"><div className="orbit orbit-one" /><div className="orbit orbit-two" /><div className="phone-outline"><span /><Radio size={29} /><i /></div><div className="wifi-float"><Wifi size={20} /></div></div>
              <h2>{active ? '等待 iPhone 连接' : '等待投屏'}</h2>
              {active && <p>控制中心 → 屏幕镜像 →「{status.receiverName}」</p>}

              {active && <div className="waiting"><span />正在广播 · {status.receiverName}</div>}
            </div>}
          </div>
        </div>

        <aside className="details-panel">
          <div className="inspector-content">
          <section className="inspector-card">
            <h3 className="card-heading">连接状态</h3>
            <div className="card-body">
              <div className={`state-line ${status.receiver === 'streaming' ? 'is-live' : ''}`}><i />{status.receiver === 'streaming' ? '已连接' : receiverLabels[status.receiver]}</div>
              {status.receiver === 'streaming' && <div className="device-line"><Smartphone size={18} />iPhone</div>}
              <div className="info-row receiver-row"><span>接收名称</span><strong>{status.receiverName}</strong></div>
              <button className="secondary-button" disabled={busy || status.receiver === 'starting'} onClick={() => run(() => active ? command('stop_receiver') : command('start_receiver', { name, testMode: false }))}>{active ? <Square size={15} /> : <Play size={15} />}{active ? '停止投屏' : '开始接收投屏'}</button>
            </div>
          </section>
          <section className="inspector-card">
            <h3 className="card-heading">鼠标控制</h3>
            <div className="card-body">
              <div className={`state-line ${status.bluetooth === 'connected' ? 'is-live' : ''}`}><Bluetooth size={20} /><i />{btLabels[status.bluetooth]}</div>
              <button className="secondary-button" disabled={busy || status.bluetooth === 'starting'} onClick={() => run(() => command(status.bluetooth === 'advertising' || status.bluetooth === 'connected' ? 'stop_bluetooth' : 'start_bluetooth'))}><Monitor size={15} />{status.bluetooth === 'advertising' || status.bluetooth === 'connected' ? '关闭鼠标控制' : '开启鼠标控制'}</button>
              {status.bluetooth !== 'connected' && status.bluetooth !== 'off' && <p className="status-message" role="status">{status.bluetoothMessage}</p>}
            </div>
          </section>
          <section className="inspector-card">
            <h3 className="card-heading">声音</h3>
            <div className="card-body">
              <div className="info-row audio-state"><span>播放状态</span><strong>{status.muted ? '已静音' : status.audio || '待连接'}</strong></div>
              <div className="audio-control"><button className="icon-button" title={status.muted ? '取消静音' : '静音'} onClick={() => run(() => command('set_audio', { volume: status.volume, muted: !status.muted }))}>{status.muted ? <VolumeX size={17} /> : <Volume2 size={17} />}</button><input aria-label="音量" type="range" min="0" max="1" step="0.01" value={status.volume} onChange={e => { const volume = Number(e.target.value); setStatus(s => ({ ...s, volume })); command('set_audio', { volume, muted: status.muted }).catch(e => setError(String(e))); }} /><span>{Math.round(status.volume * 100)}%</span></div>
            </div>
          </section>
          <section className="inspector-card">
            <h3 className="card-heading">传输状态</h3>
            <div className="card-body metrics">
              <div className="info-row"><span>接收帧率</span><strong className={status.receiver === 'streaming' ? 'live-value' : ''}>{status.receivedFps.toFixed(1)} fps</strong></div>
              <div className="info-row"><span>呈现帧率</span><strong className={status.receiver === 'streaming' ? 'live-value' : ''}>{status.presentedFps.toFixed(1)} fps</strong></div>
              <div className="info-row"><span>分辨率</span><strong>{status.width ? `${status.width} × ${status.height}` : '—'}</strong></div>
              <div className="info-row"><span>视频解码</span><strong>{status.decoder || '待连接'}</strong></div>
              {status.testMode && <p className="test-label">当前为测试源</p>}
            </div>
          </section>
          </div>
        </aside>
      </section>
      </div>
      <footer><div><span className={active ? 'signal-dot' : 'idle-dot'} />{status.message}</div><span className="footer-note"><Wifi size={14} />{status.receiver === 'streaming' ? '投屏已连接' : receiverLabels[status.receiver]}<span>·</span><Bluetooth size={14} />蓝牙 {btLabels[status.bluetooth]}</span></footer>
      </>}
      {error && <div role="alert" className="error-toast"><span>{error}</span><button className="icon-button" title="关闭提示" onClick={() => setError('')}><X size={15} /></button></div>}
    </main>
      {dialog && <div className="modal-backdrop" onClick={() => setDialog(null)}><section className="modal" role="dialog" aria-modal="true" aria-labelledby="dialog-title" onClick={e => e.stopPropagation()}><button className="modal-close icon-button" title="关闭" onClick={() => setDialog(null)}><X size={20} /></button><span className="eyebrow">澄镜 · IPHONE CAST</span><h2 id="dialog-title">{dialog === 'help' ? '连接帮助' : '设置'}</h2>{dialog === 'help' ? <><div className="help-row"><Wifi /><div><h3>画面与声音</h3><p>电脑与 iPhone 连接同一 Wi-Fi。开始接收后，在控制中心 → 屏幕镜像中选择「{status.receiverName}」。</p></div></div><div className="help-row"><Bluetooth /><div><h3>鼠标控制</h3><p>配对电脑后，前往 设置 → 辅助功能 → 触控 → 辅助触控，开启辅助触控及「执行触控手势」，即可用鼠标点击和滑动。</p></div></div><div className="help-row"><Bluetooth /><div><h3>蓝牙音箱与播放控制</h3><p>先在 Windows 蓝牙设置中配对手机，再到蓝牙音箱模式刷新并连接。媒体控制使用同一套蓝牙 HID 服务，请确认配对的是当前播放的手机。升级新增媒体按键后可能需要忽略电脑并重新配对一次，之后切换模式无需重新配对。首次播放请在手机音乐应用中开始播放。</p></div></div><div className="help-row"><Expand /><div><h3>专注模式</h3><p>按 F11 进入专注模式，Esc 返回。按住 Alt 拖动移动窗口，Alt + 拖动边角等比缩放。</p></div></div></> : <><label className="field-label">AirPlay 接收名称<input className="text-input" maxLength={48} value={name} disabled={active} onChange={e => setName(e.target.value)} /></label><p>新名称将在下次开始接收时生效。声音输出到 Windows 默认播放设备。</p><div className="setting-row"><div><strong>60fps 目标</strong><p>实际帧率由手机、内容与网络决定。</p></div><Check size={18} /></div><div className="setting-row"><div><strong>原生硬件解码</strong><p>优先 Direct3D 11，不可用时使用软件解码。</p></div><Check size={18} /></div><button className="secondary-button" disabled={active || busy} onClick={() => run(async () => { setDialog(null); await command('start_receiver', { name, testMode: true }); })}><Activity size={16} />运行 60fps 本机测试源</button><button className="secondary-button diagnostics" onClick={() => run(() => command('export_diagnostics'))}><ArrowDownToLine size={16} />导出诊断报告</button><div className="setting-row version-row"><strong>版本</strong><span>v{metadata.version}</span></div></>}<div className="modal-foot"><Smartphone size={15} />Windows 11 x64 · iPhone · 本地连接</div></section></div>}
  </div>;
}

