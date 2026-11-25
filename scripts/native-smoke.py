"""Exercise the actual packaged native process in a disposable test window.

No phone is simulated: this tests only the explicitly named generated video source.
Use --seconds 600 for the sustained rendering run. Results are local artifacts.
"""
import argparse, ctypes, json, os, queue, subprocess, threading, time, tkinter as tk
from pathlib import Path

parser=argparse.ArgumentParser()
parser.add_argument('--seconds',type=int,default=12)
parser.add_argument('--airplay',action='store_true')
parser.add_argument('--hidden',action='store_true',help='No interactive window; service/lifecycle checks only')
args=parser.parse_args()
root_path=Path(__file__).resolve().parents[1]
runtime=root_path/'src-tauri/resources/runtime'
cache=root_path/'.cache'
cache.mkdir(exist_ok=True)
ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
window=tk.Tk()
if args.hidden:window.withdraw()
window.title('澄镜 · Native pipeline verification')
window.geometry('1000x700')
window.configure(background='#101419')
window.update()
environment=os.environ.copy()
environment.update(GST_PLUGIN_PATH=str(runtime/'gstreamer-1.0'),GST_PLUGIN_SYSTEM_PATH=str(runtime/'gstreamer-1.0'),GST_PLUGIN_SCANNER=str(runtime/'gst-plugin-scanner.exe'),GST_REGISTRY=str(cache/'smoke-registry.bin'),CAST_DATA_DIR=str(cache))
events=queue.Queue()
log=(cache/'native-smoke.log').open('wb')
command=[str(runtime/'cast-receiver.exe'),'--parent',str(window.winfo_id()),'--name','ChengJing Verification']
if not args.airplay:command.append('--test')
process=subprocess.Popen(command,cwd=runtime,env=environment,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=log,creationflags=0x08000000)
def read_events():
    for line in process.stdout:
        try:events.put(json.loads(line))
        except (ValueError,UnicodeDecodeError):events.put({'event':'invalid-output','raw':repr(line[:200])})
threading.Thread(target=read_events,daemon=True).start()
def send(**value):
    process.stdin.write((json.dumps({'v':1,**value})+'\n').encode());process.stdin.flush()
send(cmd='viewport',rect={'x':0,'y':0,'width':1000,'height':700,'scale':1})
# Invalid JSON/version must not crash the receiver or mutate the running state.
process.stdin.write(b'not-json\n{"v":999,"cmd":"stop"}\n');process.stdin.flush()
started=time.monotonic()
records=[]
failures=[]
ready=False
video=False
closed=False
finished=False
actual_duration=0.0
def finish():
    global finished,closed,actual_duration
    if finished:return
    finished=True
    actual_duration=time.monotonic()-started
    try:
        send(cmd='stop')
        deadline=time.monotonic()+3
        while process.poll() is None and time.monotonic()<deadline:
            window.update_idletasks();window.update();time.sleep(0.01)
        if process.poll() is None:raise subprocess.TimeoutExpired(command,3)
        if process.returncode!=0:failures.append(f'Nonzero shutdown exit code: {process.returncode}')
        closed=True
    except (OSError,subprocess.TimeoutExpired):
        process.kill();process.wait();failures.append('Receiver did not stop cleanly')
    try:process.stdin.close()
    except OSError:pass
    window.destroy()
def poll():
    global ready,video
    while not events.empty():
        event=events.get();records.append(event)
        if event.get('v')!=1:failures.append('Invalid protocol output')
        kind=event.get('event')
        if kind=='ready':ready=True
        if kind=='video':video=True
        if kind in ('error','invalid-output'):failures.append(event)
    elapsed=time.monotonic()-started
    if process.poll() is not None:failures.append(f'Early exit: {process.returncode}');finish();return
    if elapsed>args.seconds:finish();return
    window.after(50,poll)
window.protocol('WM_DELETE_WINDOW',finish)
window.after(50,poll)
window.mainloop()
log.close()
stats=[e for e in records if e.get('event')=='stats'][2:]
mean_received=sum(e['receivedFps'] for e in stats)/len(stats) if stats else 0
mean_presented=sum(e['presentedFps'] for e in stats)/len(stats) if stats else 0
if not ready:failures.append('Missing ready handshake')
if not args.airplay and not video:failures.append('Missing generated video frames')
if not args.airplay and not stats:failures.append('Missing frame statistics')
if actual_duration<args.seconds-0.2:failures.append('Test interrupted before the requested duration')
report={'source':'airplay-service-only' if args.airplay else 'generated-1080p-60fps','requestedDurationSeconds':args.seconds,'durationSeconds':actual_duration,'hidden':args.hidden,'ready':ready,'video':video,'closedCleanly':closed,'meanReceivedFps':mean_received,'meanPresentedFps':mean_presented,'samples':len(stats),'failures':failures,'events':records}
output=cache/('airplay-smoke.json' if args.airplay else 'native-hidden-smoke.json' if args.hidden else 'native-performance.json')
output.write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in report.items() if k!='events'},ensure_ascii=False,indent=2))
raise SystemExit(1 if failures else 0)
