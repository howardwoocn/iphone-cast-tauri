import { useEffect, useState } from 'react';
import { Bluetooth, Play, Pause, RefreshCw, SkipBack, SkipForward, Smartphone, Volume2, X } from 'lucide-react';
import { CastStatus, command, desktop, initialSpeakerStatus, SpeakerDevice, SpeakerStatus } from './bridge';

const labels = { off: '未连接', connected: '已连接', disconnected: '已断开', error: '连接异常' };
export default function SpeakerView({ cast, onBusy }: { cast: CastStatus; onBusy: (busy: boolean) => void }) {
  const [status, setStatus] = useState<SpeakerStatus>(initialSpeakerStatus);
  const [devices, setDevices] = useState<SpeakerDevice[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const connected = status.state === 'connected';
  const playing = connected && cast.mediaPlayback === 'playing';
  const toggleLabel = playing ? '暂停' : cast.mediaPlayback === 'paused' ? '播放' : '播放/暂停（状态未获取）';
  const refresh = async () => setDevices(await command<SpeakerDevice[]>('list_speaker_devices'));
  const run = async (operation: () => Promise<unknown>) => {
    setError(''); setNotice(''); setBusy(true); onBusy(true);
    try { await operation(); } catch (e) { setError(String(e)); }
    finally { setBusy(false); onBusy(false); }
  };

  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const value = await command<SpeakerStatus>('get_speaker_status');
        if (!disposed) setStatus(value);
      } catch (e) { if (!disposed) setError(String(e)); }
      if (!disposed) timer = setTimeout(poll, 2000);
    };
    command<SpeakerDevice[]>('list_speaker_devices').then(value => { if (!disposed) setDevices(value); }).catch(e => { if (!disposed) setError(String(e)); });
    void poll();
    return () => { disposed = true; clearTimeout(timer); };
  }, []);

  const connect = (device: SpeakerDevice) => run(async () => {
    setStatus(await command<SpeakerStatus>('connect_speaker', { deviceId: device.id }));
    if (cast.bluetooth === 'off' || cast.bluetooth === 'error') await command('start_bluetooth');
  });
  const control = (action: string) => run(async () => {
    await command('send_media_control', { action });
    setNotice('媒体按键已发送');
  });

  return <>
    <header><h1>蓝牙音箱</h1><div className={`connection-pill ${connected ? 'live' : ''}`}><i />{labels[status.state]}</div></header>
    {!desktop && <div className="preview-notice">界面预览 · 桌面应用可使用全部功能</div>}
    <section className="workspace">
      <div className="display-card speaker-display">
        <div className="display-top"><div>音频接收<span className="subtle">BLUETOOTH</span></div></div>
        <div className="speaker-stage">
          <div className="speaker-content">
            <div className="playback-controls" aria-label="手机播放控制">
              {([{ action: 'previous', label: '上一首', Icon: SkipBack }, { action: 'toggle', label: toggleLabel, Icon: playing ? Pause : Play }, { action: 'next', label: '下一首', Icon: SkipForward }]).map(({ action, label, Icon }) => <button key={action} aria-label={label} title={label} className={`media-button ${action === 'toggle' ? 'media-toggle' : ''}`} disabled={!desktop || !connected || !cast.mediaReady || busy} onClick={() => control(action)}><span className="media-glyph"><Icon size={27} strokeWidth={1.5} /></span></button>)}
            </div>
            <h2>{status.deviceName || '等待手机连接'}</h2>
            <div className={`state-line ${connected ? 'is-live' : ''}`}><i />{connected ? '音频已连接' : labels[status.state]}</div>
            <p>{busy ? '正在处理…' : notice || status.message}</p>
            {connected && !cast.mediaReady && <p className="media-hint">蓝牙媒体控制尚未连接</p>}
          </div>
          <div className="sound-rings" aria-hidden="true" />
        </div>
        <div className="display-toolbar speaker-toolbar"><span><Volume2 size={18} />Windows 默认播放设备</span><small>音量由手机与 Windows 调整</small></div>
      </div>
      <aside className="details-panel">
        <section className="inspector-card">
          <div className="card-heading speaker-heading"><h3>手机设备</h3><button className="refresh-button" disabled={!desktop || busy} onClick={() => run(refresh)}><RefreshCw size={14} />刷新</button></div>
          <div className="card-body device-list">
            {devices.length === 0 && <p className="status-message">未发现手机，请先在 Windows 蓝牙设置中配对，然后刷新。</p>}
            {devices.map(device => <div className="speaker-device" key={device.id}><Smartphone size={23} /><div><strong>{device.name || '蓝牙设备'}</strong><small>{connected && status.deviceId === device.id ? '已连接' : '已配对'}</small></div><button className="secondary-button" disabled={busy || !desktop} onClick={() => connected && status.deviceId === device.id ? run(async () => setStatus(await command<SpeakerStatus>('disconnect_speaker'))) : connect(device)}>{connected && status.deviceId === device.id ? '断开连接' : '连接'}</button></div>)}
          </div>
        </section>
        <section className="inspector-card"><h3 className="card-heading">连接状态</h3><div className="card-body metrics">
          <div className="info-row"><span>蓝牙音频</span><strong className={connected ? 'live-value' : ''}>{labels[status.state]}</strong></div>
          <div className="info-row"><span>当前设备</span><strong>{status.deviceName || '—'}</strong></div>
          <div className="info-row"><span>传输方式</span><strong>A2DP</strong></div>
          <div className="info-row"><span>媒体控制</span><strong className={cast.mediaReady ? 'live-value' : ''}>{cast.mediaReady ? '已连接' : cast.bluetooth === 'advertising' ? '等待配对' : cast.bluetooth === 'starting' ? '正在开启' : '未连接'}</strong></div>
          {!cast.mediaReady && <button className="secondary-button media-connect" disabled={!desktop || busy || cast.bluetooth === 'starting' || cast.bluetooth === 'advertising'} onClick={() => run(() => command('start_bluetooth'))}>{cast.bluetooth === 'advertising' ? '等待手机配对' : '开启媒体控制'}</button>}
        </div></section>
        <section className="inspector-card"><h3 className="card-heading">声音输出</h3><div className="card-body metrics"><div className="info-row"><span>输出设备</span><strong>系统默认</strong></div><div className="info-row"><span>音量控制</span><strong>手机与 Windows</strong></div></div></section>
      </aside>
    </section>
    <footer><div><span className={connected ? 'signal-dot' : 'idle-dot'} />{connected ? `已连接 ${status.deviceName}` : status.message}</div><span className="footer-note"><Bluetooth size={14} />蓝牙音频</span></footer>
    {error && <div role="alert" className="error-toast"><span>{error}</span><button className="icon-button" title="关闭提示" onClick={() => setError('')}><X size={15} /></button></div>}
  </>;
}
