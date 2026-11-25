import { useEffect, useRef, useState } from 'react';
import { Bluetooth, Play, Pause, RefreshCw, SkipBack, SkipForward, Smartphone, Volume2, VolumeX, X } from 'lucide-react';
import { CastStatus, command, desktop, initialSpeakerStatus, SpeakerDevice, SpeakerStatus } from './bridge';

const labels = { off: '未连接', connected: '已连接', disconnected: '已断开', error: '连接异常' };
export default function SpeakerView({ cast, onBusy }: { cast: CastStatus; onBusy: (busy: boolean) => void }) {
  const [status, setStatus] = useState<SpeakerStatus>(initialSpeakerStatus);
  const [devices, setDevices] = useState<SpeakerDevice[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [volumeBusy, setVolumeBusy] = useState(false);
  const [volumeDraft, setVolumeDraft] = useState<number | null>(null);
  const volumeTimer = useRef<ReturnType<typeof setTimeout>>();
  const volumeSequence = useRef(0);
  const previousVolume = useRef(0.5);
  const connected = status.state === 'connected';
  const reportedVolume = connected ? cast.mediaVolume : null;
  const shownVolume = volumeDraft ?? reportedVolume;
  const volumeAvailable = desktop && connected && cast.mediaReady && reportedVolume !== null;
  useEffect(() => {
    if (!volumeBusy && reportedVolume !== null && reportedVolume > 0) previousVolume.current = reportedVolume;
  }, [reportedVolume, volumeBusy]);
  useEffect(() => () => { clearTimeout(volumeTimer.current); volumeSequence.current++; }, []);
  const adjustVolume = (value: number) => {
    clearTimeout(volumeTimer.current);
    const sequence = ++volumeSequence.current;
    setVolumeDraft(value); setVolumeBusy(true); onBusy(true); setError('');
    volumeTimer.current = setTimeout(async () => {
      try { await command<number | null>('set_phone_volume', { volume: value }); }
      catch (e) { if (sequence === volumeSequence.current) setError(String(e)); }
      finally {
        if (sequence === volumeSequence.current) { setVolumeDraft(null); setVolumeBusy(false); onBusy(false); }
      }
    }, 140);
  };
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
    setNotice(action.startsWith('volume_') ? '手机音量调节指令已发送' : '媒体按键已发送');
  });

  return <>
    <div className="mode-layout">
    <header><div className="header-title"><h1>蓝牙音箱</h1>{!desktop && <span className="preview-notice">界面预览 · 桌面应用可使用全部功能</span>}</div></header>
    <section className="workspace">
      <div className="display-card speaker-display">
        <div className="speaker-stage">
          <div className="speaker-content">
            <div className="playback-controls" aria-label="手机播放控制">
              {([{ action: 'previous', label: '上一首', Icon: SkipBack }, { action: 'toggle', label: toggleLabel, Icon: playing ? Pause : Play }, { action: 'next', label: '下一首', Icon: SkipForward }]).map(({ action, label, Icon }) => <button key={action} aria-label={label} title={label} className={`media-button ${action === 'toggle' ? 'media-toggle' : ''}`} disabled={!desktop || !connected || !cast.mediaReady || busy || volumeBusy} onClick={() => control(action)}><span className="media-glyph"><Icon size={27} strokeWidth={1.5} /></span></button>)}
            </div>
            <h2>{status.deviceName || '等待手机连接'}</h2>
            <div className={`state-line ${connected ? 'is-live' : ''}`}><i />{connected ? '已连接' : status.state === 'disconnected' ? '已断开' : '未连接'}</div>
            <p>{busy ? '正在处理…' : notice || status.message}</p>
            {connected && !cast.mediaReady && <p className="media-hint">蓝牙媒体控制尚未连接</p>}
          </div>
          <div className="sound-rings" aria-hidden="true" />
        </div>
      </div>
      <aside className="details-panel">
          <div className="inspector-content">
        <section className="inspector-card">
          <div className="card-heading speaker-heading"><h3>手机设备</h3><button className="refresh-button" disabled={!desktop || busy || volumeBusy} onClick={() => run(refresh)}><RefreshCw size={14} />刷新</button></div>
          <div className="card-body device-list">
            {devices.length === 0 && <p className="status-message">未发现手机，请先在 Windows 蓝牙设置中配对，然后刷新。</p>}
            {devices.map(device => <div className="speaker-device" key={device.id}><Smartphone size={23} /><div><strong>{device.name || '蓝牙设备'}</strong><small>{connected && status.deviceId === device.id ? '已连接' : '已配对'}</small></div><button className="secondary-button" disabled={busy || volumeBusy || !desktop} onClick={() => connected && status.deviceId === device.id ? run(async () => setStatus(await command<SpeakerStatus>('disconnect_speaker'))) : connect(device)}>{connected && status.deviceId === device.id ? '断开' : '连接'}</button></div>)}
          </div>
        </section>
        <section className="inspector-card"><h3 className="card-heading">连接状态</h3><div className="card-body metrics">
          <div className="info-row"><span>蓝牙音频</span><strong className={connected ? 'live-value' : ''}>{labels[status.state]}</strong></div>
          <div className="info-row"><span>当前设备</span><strong>{status.deviceName || '—'}</strong></div>
          <div className="info-row"><span>传输方式</span><strong>A2DP</strong></div>
          <div className="info-row"><span>媒体控制</span><strong className={cast.mediaReady ? 'live-value' : ''}>{cast.mediaReady ? '已连接' : cast.bluetooth === 'advertising' ? '等待配对' : cast.bluetooth === 'starting' ? '正在开启' : '未连接'}</strong></div>
          {!cast.mediaReady && <button className="secondary-button media-connect" disabled={!desktop || busy || volumeBusy || cast.bluetooth === 'starting' || cast.bluetooth === 'advertising'} onClick={() => run(() => command('start_bluetooth'))}>{cast.bluetooth === 'advertising' ? '等待手机配对' : '开启媒体控制'}</button>}
        </div></section>
        <section className="inspector-card"><h3 className="card-heading">音量控制</h3><div className="card-body metrics">
          <div className="audio-control" aria-label="手机音量控制">
            <button className="icon-button" aria-label={reportedVolume === 0 ? '恢复手机音量' : '手机静音'} title={reportedVolume === 0 ? '恢复手机音量' : '手机静音'} disabled={!volumeAvailable || busy || volumeBusy} onClick={() => adjustVolume(reportedVolume === 0 ? previousVolume.current : 0)}>{shownVolume === 0 ? <VolumeX size={17} /> : <Volume2 size={17} />}</button>
            <input aria-label="手机音量" title={reportedVolume === null ? '等待手机回传音量' : '手机音量'} type="range" min="0" max="1" step="0.01" disabled={!volumeAvailable || busy} value={shownVolume ?? 0} onChange={e => adjustVolume(Number(e.target.value))} />
            <span>{shownVolume === null ? '—' : `${Math.round(shownVolume * 100)}%`}</span>
          </div>
          {connected && cast.mediaReady && reportedVolume === null && <p className="status-message">等待手机回传音量</p>}
        </div></section>
          </div>
      </aside>
    </section>
    </div>
    <footer><div><span className={connected ? 'signal-dot' : 'idle-dot'} />{connected ? `已连接 ${status.deviceName}` : status.message}</div><span className="footer-note"><Bluetooth size={14} />蓝牙音频</span></footer>
    {error && <div role="alert" className="error-toast"><span>{error}</span><button className="icon-button" title="关闭提示" onClick={() => setError('')}><X size={15} /></button></div>}
  </>;
}
