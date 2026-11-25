import { useEffect, useRef, useState } from 'react';
import { Bluetooth, Play, Pause, RefreshCw, SkipBack, SkipForward, Volume2, VolumeX, X } from 'lucide-react';
import { CastStatus, command, desktop, initialSpeakerStatus, SpeakerDevice, SpeakerStatus } from './bridge';
import VolumeSlider from './VolumeSlider';

const labels = { off: '未连接', connected: '已连接', disconnected: '已断开', error: '连接异常' };
export default function SpeakerView({ cast, onBusy }: { cast: CastStatus; onBusy: (busy: boolean) => void }) {
  const [status, setStatus] = useState<SpeakerStatus>(initialSpeakerStatus);
  const [devices, setDevices] = useState<SpeakerDevice[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [connectingDevice, setConnectingDevice] = useState<string | null>(null);
  const [volumeDraft, setVolumeDraft] = useState<number | null>(null);
  const volumeTimer = useRef<ReturnType<typeof setTimeout>>();
  const volumeSequence = useRef(0);
  const previousVolume = useRef(0.5);
  const mediaPending = useRef(false);
  const [playbackDraft, setPlaybackDraft] = useState<'playing' | 'paused' | null>(null);
  const playbackTarget = useRef<'playing' | 'paused' | null>(null);
  const playbackTimer = useRef<ReturnType<typeof setTimeout>>();
  const playbackRequest = useRef(0);
  const mounted = useRef(true);
  const connected = status.state === 'connected';
  const reportedVolume = connected ? cast.mediaVolume : null;
  const shownVolume = volumeDraft ?? reportedVolume;
  const volumeAvailable = desktop && connected && cast.mediaReady && reportedVolume !== null;
  useEffect(() => {
    if (volumeDraft === null && reportedVolume !== null && reportedVolume > 0) previousVolume.current = reportedVolume;
  }, [reportedVolume, volumeDraft]);
  useEffect(() => {
    clearTimeout(volumeTimer.current); volumeSequence.current++; setVolumeDraft(null);
  }, [connected, cast.mediaReady, status.deviceId]);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; clearTimeout(volumeTimer.current); clearTimeout(playbackTimer.current); volumeSequence.current++; playbackRequest.current++; };
  }, []);
  const adjustVolume = (value: number) => {
    clearTimeout(volumeTimer.current);
    const sequence = ++volumeSequence.current;
    if (value === 0 && shownVolume !== null && shownVolume > 0) previousVolume.current = shownVolume;
    setVolumeDraft(value); setError('');
    volumeTimer.current = setTimeout(async () => {
      try { await command<number | null>('set_phone_volume', { volume: value }); }
      catch (e) { if (mounted.current && sequence === volumeSequence.current) setError(String(e)); }
      finally {
        if (mounted.current && sequence === volumeSequence.current) setVolumeDraft(null);
      }
    }, 140);
  };
  useEffect(() => {
    if (!connected || !cast.mediaReady) {
      clearTimeout(playbackTimer.current); playbackTarget.current = null; setPlaybackDraft(null); playbackRequest.current++;
    } else if (playbackTarget.current === cast.mediaPlayback) {
      clearTimeout(playbackTimer.current); playbackTarget.current = null; setPlaybackDraft(null);
    }
  }, [connected, cast.mediaReady, cast.mediaPlayback]);
  const playing = connected && (playbackDraft ?? cast.mediaPlayback) === 'playing';
  const toggleLabel = playbackDraft !== null ? `${playing ? '暂停' : '播放'}（等待手机确认）` : playing ? '暂停' : cast.mediaPlayback === 'paused' ? '播放' : '播放/暂停（状态未获取）';
  const refresh = async () => setDevices(await command<SpeakerDevice[]>('list_speaker_devices'));
  const run = async (operation: () => Promise<unknown>) => {
    setError(''); setBusy(true); onBusy(true);
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
    const initialize = async () => {
      try {
        const [available, current] = await Promise.all([
          command<SpeakerDevice[]>('list_speaker_devices'),
          command<SpeakerStatus>('get_speaker_status')
        ]);
        if (disposed) return;
        setDevices(available); setStatus(current);
        if (available.length === 1 && current.state !== 'connected') await connect(available[0]);
      } catch (e) { if (!disposed) setError(String(e)); }
      finally { if (!disposed) timer = setTimeout(poll, 2000); }
    };
    void initialize();
    return () => { disposed = true; clearTimeout(timer); };
  }, []);

  const connect = (device: SpeakerDevice) => run(async () => {
    setConnectingDevice(device.id);
    try {
      setStatus(await command<SpeakerStatus>('connect_speaker', { deviceId: device.id }));
      if (cast.bluetooth === 'off' || cast.bluetooth === 'error') await command('start_bluetooth');
    } finally { setConnectingDevice(null); }
  });
  const control = async (action: string) => {
    if (mediaPending.current || !connected || !cast.mediaReady) return;
    mediaPending.current = true;
    setError('');
    let request = 0;
    if (action === 'toggle') {
      request = ++playbackRequest.current;
      clearTimeout(playbackTimer.current);
      playbackTarget.current = playing ? 'paused' : 'playing';
      setPlaybackDraft(playbackTarget.current);
      playbackTimer.current = setTimeout(() => {
        if (mounted.current && playbackRequest.current === request && playbackTarget.current !== null) {
          playbackTarget.current = null; setPlaybackDraft(null);
          setError('未收到手机播放状态，请确认手机播放情况');
        }
      }, 5000);
    }
    try { await command('send_media_control', { action }); }
    catch (e) {
      if (mounted.current) {
        if (action === 'toggle' && playbackRequest.current === request) {
          clearTimeout(playbackTimer.current); playbackTarget.current = null; setPlaybackDraft(null);
        }
        setError(String(e));
      }
    }
    finally { mediaPending.current = false; }
  };

  return <>
    <div className="mode-layout">
    <header><div className="header-title"><h1>蓝牙音箱</h1>{!desktop && <span className="preview-notice">界面预览 · 桌面应用可使用全部功能</span>}</div></header>
    <section className="workspace">
      <div className="display-card speaker-display">
        <div className="speaker-stage">
          <div className="speaker-content">
            <div className="playback-controls" aria-label="手机播放控制">
              {([{ action: 'previous', label: '上一首', Icon: SkipBack }, { action: 'toggle', label: toggleLabel, Icon: playing ? Pause : Play }, { action: 'next', label: '下一首', Icon: SkipForward }]).map(({ action, label, Icon }) => <button key={action} aria-label={label} title={label} className={`media-button ${action === 'toggle' ? 'media-toggle' : ''}`} disabled={!desktop || !connected || !cast.mediaReady || busy} onClick={() => control(action)}><span className="media-glyph"><Icon size={27} strokeWidth={1.5} /></span></button>)}
            </div>
            <h2>{status.deviceName || '等待手机连接'}</h2>
            <div className={`state-line ${connected ? 'is-live' : ''}`}><i />{connected ? '已连接' : status.state === 'disconnected' ? '已断开' : '未连接'}</div>
            <p>{busy ? '正在处理…' : status.message}</p>
            {connected && !cast.mediaReady && <p className="media-hint">蓝牙媒体控制尚未连接</p>}
          </div>
          <div className="sound-rings" aria-hidden="true" />
        </div>
      </div>
      <aside className="details-panel">
          <div className="inspector-content">
        <section className="inspector-card">
          <div className="card-heading speaker-heading"><h3>设备列表</h3><button className="refresh-button" disabled={!desktop || busy} onClick={() => run(refresh)}><RefreshCw size={14} />刷新</button></div>
          <div className="card-body device-list">
            {devices.length === 0 && <p className="status-message">未发现手机，请先在 Windows 蓝牙设置中配对，然后刷新。</p>}
            {devices.map(device => {
              const selected = connected && status.deviceId === device.id;
              const name = device.name || '蓝牙设备';
              return <button className={`speaker-device ${selected ? 'is-connected' : ''}`} key={device.id} aria-label={`${selected ? '断开' : '连接'} ${name}`} aria-pressed={selected} title={`${name} · 点击${selected ? '断开' : '连接'}`} disabled={busy || !desktop} onClick={() => selected ? run(async () => setStatus(await command<SpeakerStatus>('disconnect_speaker'))) : connect(device)}>
                <span className="device-name">{name}</span>
                <span className="device-connection-state">{connectingDevice === device.id ? '连接中' : selected ? '已连接' : '已配对'}</span>
              </button>;
            })}
          </div>
        </section>
        <section className="inspector-card"><h3 className="card-heading">连接状态</h3><div className="card-body metrics">
          <div className="info-row"><span>当前设备</span><strong>{status.deviceName || ''}</strong></div>
          <div className="info-row"><span>蓝牙音频</span><strong className={connected ? 'live-value' : ''}>{labels[status.state]}</strong></div>
          <div className="info-row"><span>媒体控制</span><strong className={cast.mediaReady ? 'live-value' : ''}>{cast.mediaReady ? '已连接' : cast.bluetooth === 'advertising' ? '等待配对' : cast.bluetooth === 'starting' ? '正在开启' : '未连接'}</strong></div>
          <div className="info-row"><span>传输方式</span><strong>A2DP</strong></div>
          {!cast.mediaReady && <button className="secondary-button media-connect" disabled={!desktop || busy || cast.bluetooth === 'starting' || cast.bluetooth === 'advertising'} onClick={() => run(() => command('start_bluetooth'))}>{cast.bluetooth === 'advertising' ? '等待手机配对' : '开启媒体控制'}</button>}
        </div></section>
        <section className="inspector-card"><h3 className="card-heading">音量控制</h3><div className="card-body metrics">
          <div className="audio-control" aria-label="手机音量控制">
            <button className="icon-button" aria-label={shownVolume === 0 ? '恢复手机音量' : '手机静音'} title={shownVolume === 0 ? '恢复手机音量' : '手机静音'} disabled={!volumeAvailable || busy} onClick={() => adjustVolume(shownVolume === 0 ? previousVolume.current : 0)}>{shownVolume === 0 ? <VolumeX size={17} /> : <Volume2 size={17} />}</button>
            <VolumeSlider label="手机音量" disabled={!volumeAvailable || busy} value={shownVolume} onChange={adjustVolume} />
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
