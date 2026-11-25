import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type CastStatus = {
  receiver: 'stopped' | 'starting' | 'listening' | 'streaming' | 'error';
  bluetooth: 'off' | 'starting' | 'advertising' | 'connected' | 'error';
  mediaReady: boolean;
  mediaPlayback: 'unknown' | 'playing' | 'paused';
  mediaVolume: number | null;
  message: string; bluetoothMessage: string; receiverName: string;
  receivedFps: number; presentedFps: number; dropped: number;
  width: number; height: number; decoder: string; audio: string;
  volume: number; muted: boolean; focus: boolean; testMode: boolean;
};
export const initialStatus: CastStatus = {
  receiver: 'stopped', bluetooth: 'off', mediaReady: false, mediaPlayback: 'unknown', mediaVolume: null, message: '准备连接你的 iPhone',
  bluetoothMessage: '蓝牙控制尚未开启', receiverName: '澄镜 iPhone Cast',
  receivedFps: 0, presentedFps: 0, dropped: 0, width: 0, height: 0,
  decoder: '', audio: '', volume: 0.8, muted: false, focus: false, testMode: false
};
export const desktop = '__TAURI_INTERNALS__' in window;
export async function command<T = void>(name: string, args?: Record<string, unknown>): Promise<T> {
  if (!desktop) throw new Error('当前是浏览器预览。请运行桌面应用连接手机。');
  return invoke<T>(name, args);
}
export async function subscribe(fn: (status: CastStatus) => void): Promise<() => void> {
  if (!desktop) return () => {};
  const unlisten = await listen<CastStatus>('cast-status', event => fn(event.payload));
  fn(await command<CastStatus>('get_status'));
  return unlisten;
}

export type SpeakerDevice = { id: string; name: string };
export type SpeakerStatus = { state: 'off' | 'connected' | 'disconnected' | 'error'; deviceId: string; deviceName: string; message: string };
export const initialSpeakerStatus: SpeakerStatus = { state: 'off', deviceId: '', deviceName: '', message: '选择已配对的手机以接收声音' };
