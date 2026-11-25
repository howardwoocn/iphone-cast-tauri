// SPDX-License-Identifier: GPL-3.0-or-later
// Independent Windows/Tauri bridge around the pinned upstream UxPlay receiver.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <windowsx.h>
#include <shellapi.h>
#include <gst/gst.h>
#include <gst/video/videooverlay.h>
#include <gst/video/video.h>
#include <io.h>
#include <fcntl.h>
#include <atomic>
#include <chrono>
#include <cstdio>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#include <vector>
#include "third_party/json.hpp"
#include "bridge.h"
#include "geometry.h"

using json=nlohmann::json;
using Clock=std::chrono::steady_clock;
int uxplay_main(int argc,char**argv);
static constexpr UINT WM_COMMAND_LINE=WM_APP+1,WM_LAYOUT_VIDEO=WM_APP+2,WM_CLEAR_VIDEO=WM_APP+3;
static HWND parent_window=nullptr,video_window=nullptr;
static DWORD ui_thread=0;
static FILE* event_output=nullptr;
static std::mutex output_mutex,pipelines_mutex;
static std::vector<GstElement*> audio_pipelines;
static std::atomic<int> source_width{0},source_height{0};
static std::atomic<bool> focus_mode{false},visible{true},quitting{false};
static std::atomic<unsigned long long> received{0},presented{0},dropped{0};
static std::atomic<unsigned long long> decoded_serial{0},last_presented_serial{0};
static std::atomic<bool> stream_active{false};
static std::string decoder_name;
static std::atomic<bool> hardware_decoder{false},fallback_requested{false};
static json viewport={{"x",0},{"y",0},{"width",0},{"height",0},{"scale",1}};
static double audio_volume=0.8;
static bool audio_muted=false;
static bool dragging=false;
static int pointer_x=0,pointer_y=0;
static Clock::time_point last_move;

static void emit(json event) {
    event["v"]=1;
    const auto line=event.dump();
    std::lock_guard<std::mutex> lock(output_mutex);
    if(event_output){fwrite(line.data(),1,line.size(),event_output);fputc('\n',event_output);fflush(event_output);}
}
static void release_pointer(){dragging=false;if(GetCapture()==video_window)ReleaseCapture();emit({{"event","pointer"},{"kind","release"}});}
static void layout_video() {
    if(!video_window||!parent_window)return;
    RECT client;GetClientRect(parent_window,&client);
    int x=0,y=0,width=client.right,height=client.bottom;
    if(!focus_mode){
        double scale=viewport.value("scale",1.0);
        x=int(std::round(viewport.value("x",0.0)*scale));y=int(std::round(viewport.value("y",0.0)*scale));
        width=int(std::round(viewport.value("width",0.0)*scale));height=int(std::round(viewport.value("height",0.0)*scale));
    }
    auto fitted=cast::fit(width,height,source_width,source_height);
    SetWindowPos(video_window,HWND_TOP,x+fitted.x,y+fitted.y,fitted.width,fitted.height,SWP_NOACTIVATE);
    ShowWindow(video_window,visible&&stream_active&&fitted.width>0?SW_SHOWNOACTIVATE:SW_HIDE);
}
static void pointer(const char* kind,int x,int y,int wheel=0){
    RECT r;GetClientRect(video_window,&r);double nx=0,ny=0;
    // While dragging outside the image, clamp to the edge; never leave a button stuck.
    if(dragging){x=std::clamp(x,0,std::max(0,int(r.right)-1));y=std::clamp(y,0,std::max(0,int(r.bottom)-1));}
    if(!cast::normalize(x,y,r.right,r.bottom,nx,ny))return;
    const bool motion=dragging&&(strcmp(kind,"move")==0||strcmp(kind,"up")==0);
    const double scale=std::max(1.0,GetDpiForWindow(video_window)/96.0);
    const double dx=motion?(x-pointer_x)/scale:0.0,dy=motion?(y-pointer_y)/scale:0.0;
    if(strcmp(kind,"wheel")!=0){pointer_x=x;pointer_y=y;}
    emit({{"event","pointer"},{"kind",kind},{"x",nx},{"y",ny},{"wheel",wheel},{"dx",dx},{"dy",dy}});
}
static LRESULT CALLBACK window_proc(HWND hwnd,UINT msg,WPARAM wp,LPARAM lp){
    switch(msg){
    case WM_ERASEBKGND:return 1;
    case WM_PAINT:{PAINTSTRUCT paint;HDC dc=BeginPaint(hwnd,&paint);FillRect(dc,&paint.rcPaint,(HBRUSH)GetStockObject(BLACK_BRUSH));EndPaint(hwnd,&paint);return 0;}
    case WM_MOUSEACTIVATE:return MA_ACTIVATE;
    case WM_LBUTTONDOWN:{
        SetFocus(hwnd);
        int x=GET_X_LPARAM(lp),y=GET_Y_LPARAM(lp);
        if(focus_mode&&(GetAsyncKeyState(VK_MENU)&0x8000)){
            RECT r;GetClientRect(hwnd,&r);int edge=std::max(5,int(GetDpiForWindow(hwnd)/24));
            bool left=x<edge,right=x>=r.right-edge,top=y<edge,bottom=y>=r.bottom-edge;
            int hit=0;
            if(left&&top)hit=HTTOPLEFT;else if(right&&top)hit=HTTOPRIGHT;
            else if(left&&bottom)hit=HTBOTTOMLEFT;else if(right&&bottom)hit=HTBOTTOMRIGHT;
            // The narrow sides move the window, corners resize it.
            else hit=HTCAPTION;
            if(hit){release_pointer();ReleaseCapture();SendMessageW(parent_window,WM_NCLBUTTONDOWN,hit,GetMessagePos());return 0;}
        }
        dragging=true;SetCapture(hwnd);pointer("down",x,y);return 0;
    }
    case WM_LBUTTONUP:pointer("up",GET_X_LPARAM(lp),GET_Y_LPARAM(lp));dragging=false;ReleaseCapture();return 0;
    case WM_MOUSEMOVE:{auto now=Clock::now();if(now-last_move>=std::chrono::milliseconds(16)){pointer("move",GET_X_LPARAM(lp),GET_Y_LPARAM(lp));last_move=now;}return 0;}
    case WM_MOUSEWHEEL:{POINT p{GET_X_LPARAM(lp),GET_Y_LPARAM(lp)};ScreenToClient(hwnd,&p);int delta=GET_WHEEL_DELTA_WPARAM(wp)/WHEEL_DELTA;pointer("wheel",p.x,p.y,std::clamp(delta,-127,127));return 0;}
    case WM_CAPTURECHANGED:if(dragging)release_pointer();return 0;
    case WM_KILLFOCUS:if((HWND)wp!=hwnd&&!IsChild(hwnd,(HWND)wp))release_pointer();return 0;
    case WM_KEYDOWN:if(wp==VK_F11||(wp==VK_ESCAPE&&focus_mode)){emit({{"event","focus"}});return 0;}break;
    case WM_CLOSE:ShowWindow(hwnd,SW_HIDE);return 0;
    }
    return DefWindowProcW(hwnd,msg,wp,lp);
}
static void present_callback(GstElement*,void*,void*,void*){
    auto serial=decoded_serial.load();
    if(stream_active&&last_presented_serial.exchange(serial)!=serial)presented++;
}
struct FrameCounter{GstClockTime last=GST_CLOCK_TIME_NONE;int width=0,height=0;};
static GstPadProbeReturn frame_probe(GstPad*pad,GstPadProbeInfo*info,gpointer data){
    auto* counter=static_cast<FrameCounter*>(data);
    GstBuffer* buffer=GST_PAD_PROBE_INFO_BUFFER(info);
    if(!buffer)return GST_PAD_PROBE_OK;
    auto pts=GST_BUFFER_PTS(buffer);
    if(GST_CLOCK_TIME_IS_VALID(pts)&&pts==counter->last)return GST_PAD_PROBE_OK;
    counter->last=pts;decoded_serial++;
    GstCaps*caps=gst_pad_get_current_caps(pad);
    if(caps){
        GstVideoInfo video_info;
        if(gst_video_info_from_caps(&video_info,caps)){
            int width=GST_VIDEO_INFO_WIDTH(&video_info),height=GST_VIDEO_INFO_HEIGHT(&video_info);
            if(width!=counter->width||height!=counter->height||!stream_active){
                counter->width=width;counter->height=height;source_width=width;source_height=height;stream_active=true;
                emit({{"event","video"},{"width",width},{"height",height}});PostThreadMessageW(ui_thread,WM_LAYOUT_VIDEO,0,0);
            }
        }
        gst_caps_unref(caps);
    }
    return GST_PAD_PROBE_OK;
}
static GstBusSyncReply bus_sync(GstBus*,GstMessage*message,gpointer){
    if(gst_is_video_overlay_prepare_window_handle_message(message)){
        gst_video_overlay_set_window_handle(GST_VIDEO_OVERLAY(GST_MESSAGE_SRC(message)),(guintptr)video_window);
        gst_message_unref(message);return GST_BUS_DROP;
    }
    if(GST_MESSAGE_TYPE(message)==GST_MESSAGE_QOS){guint64 processed=0,skipped=0;GstFormat format;gst_message_parse_qos_stats(message,&format,&processed,&skipped);dropped=skipped;}
    return GST_BUS_PASS;
}
extern "C" void cast_video_pipeline(GstElement*pipeline){
    GstBus*bus=gst_element_get_bus(pipeline);gst_bus_set_sync_handler(bus,bus_sync,nullptr,nullptr);gst_object_unref(bus);
    // Count compressed frames at appsrc (or generated test frames), before decode.
    auto*source=gst_bin_get_by_name(GST_BIN(pipeline),"video_source");
    if(!source)source=gst_bin_get_by_name(GST_BIN(pipeline),"test_source");
    if(source){
        auto*pad=gst_element_get_static_pad(source,"src");
        gst_pad_add_probe(pad,GST_PAD_PROBE_TYPE_BUFFER,[](GstPad*,GstPadProbeInfo*info,gpointer data)->GstPadProbeReturn{
            auto*counter=static_cast<FrameCounter*>(data);auto*buffer=GST_PAD_PROBE_INFO_BUFFER(info);
            if(buffer){auto pts=GST_BUFFER_PTS(buffer);if(!GST_CLOCK_TIME_IS_VALID(pts)||pts!=counter->last){received++;counter->last=pts;}}
            return GST_PAD_PROBE_OK;
        },new FrameCounter,[](gpointer p){delete static_cast<FrameCounter*>(p);});
        gst_object_unref(pad);gst_object_unref(source);
    }
    const char*names[]={"d3d11videosink_h264","d3d11videosink_h265","d3d11videosink_jpeg","cast_sink"};
    for(auto name:names){
        GstElement*sink=gst_bin_get_by_name(GST_BIN(pipeline),name);if(!sink)continue;
        g_object_set(sink,"force-aspect-ratio",FALSE,"enable-navigation-events",FALSE,"emit-present",TRUE,"qos",TRUE,"max-lateness",gint64(50*GST_MSECOND),nullptr);
        gst_video_overlay_set_window_handle(GST_VIDEO_OVERLAY(sink),(guintptr)video_window);
        g_signal_connect(sink,"present",G_CALLBACK(present_callback),nullptr);
        GstPad*pad=gst_element_get_static_pad(sink,"sink");
        gst_pad_add_probe(pad,GST_PAD_PROBE_TYPE_BUFFER,frame_probe,new FrameCounter,[](gpointer p){delete static_cast<FrameCounter*>(p);});
        gst_object_unref(pad);gst_object_unref(sink);
    }
}
extern "C" void cast_audio_pipeline(GstElement*pipeline){
    std::lock_guard<std::mutex>lock(pipelines_mutex);
    // A reference stays alive until the upstream destroy hook unregisters it.
    // This serializes audio updates safely against pipeline rebuilds.
    audio_pipelines.push_back(GST_ELEMENT(gst_object_ref(pipeline)));
    auto*volume=gst_bin_get_by_name(GST_BIN(pipeline),"volume");
    if(volume){g_object_set(volume,"volume",audio_volume,"mute",audio_muted,nullptr);gst_object_unref(volume);}
    // Observe actual audio buffers instead of reporting audio merely because a pipeline exists.
    auto*src=gst_bin_get_by_name(GST_BIN(pipeline),"audio_source");
    if(src){auto*pad=gst_element_get_static_pad(src,"src");gst_pad_add_probe(pad,GST_PAD_PROBE_TYPE_BUFFER,[](GstPad*,GstPadProbeInfo*,gpointer)->GstPadProbeReturn{emit({{"event","audio"},{"state","正在播放"}});return GST_PAD_PROBE_REMOVE;},nullptr,nullptr);gst_object_unref(pad);gst_object_unref(src);}
}
extern "C" void cast_audio_removed(GstElement*pipeline){
    std::lock_guard<std::mutex>lock(pipelines_mutex);
    auto it=std::find(audio_pipelines.begin(),audio_pipelines.end(),pipeline);
    if(it!=audio_pipelines.end()){audio_pipelines.erase(it);gst_object_unref(pipeline);}
}
extern "C" void cast_remote_volume(GstElement*volume,double value){
    std::lock_guard<std::mutex>lock(pipelines_mutex);
    g_object_set_data_full(G_OBJECT(volume),"cast-client-volume",new double(value),[](gpointer p){delete static_cast<double*>(p);});
    g_object_set(volume,"volume",value*audio_volume,"mute",audio_muted,nullptr);
}
extern "C" void cast_disconnected(){stream_active=false;received=0;presented=0;dropped=0;source_width=0;source_height=0;emit({{"event","disconnected"}});PostThreadMessageW(ui_thread,WM_CLEAR_VIDEO,0,0);}
extern "C" void cast_ready(){emit({{"event","ready"},{"decoder",decoder_name}});}
extern "C" void cast_audio_error(const char*message){emit({{"event","audio"},{"state","播放失败"}});emit({{"event","warning"},{"message",std::string("音频播放失败：")+message}});}
extern "C" void cast_video_error(const char*message){
    cast_disconnected();
    if(hardware_decoder&&!fallback_requested.exchange(true))emit({{"event","decoder-fallback"},{"message",message}});
    else emit({{"event","error"},{"message",std::string("视频接收失败：")+message}});
}
static void change_audio(const json&command){
    double volume=command.value("volume",0.8);bool muted=command.value("muted",false);
    if(!std::isfinite(volume)||volume<0||volume>1)return;
    std::lock_guard<std::mutex>lock(pipelines_mutex);audio_volume=volume;audio_muted=muted;
    for(auto*pipeline:audio_pipelines){auto*element=gst_bin_get_by_name(GST_BIN(pipeline),"volume");if(element){auto*client=static_cast<double*>(g_object_get_data(G_OBJECT(element),"cast-client-volume"));g_object_set(element,"volume",audio_volume*(client?*client:1.0),"mute",audio_muted,nullptr);gst_object_unref(element);}}
}
static void process_command(const json&command){
    if(command.value("v",0)!=1)return;
    std::string name=command.value("cmd","");
    if(name=="stop"){quitting=true;PostQuitMessage(0);}
    else if(name=="viewport"){
        const auto&r=command.at("rect");
        for(auto key:{"x","y","width","height","scale"})if(!r.at(key).is_number()||!std::isfinite(r.at(key).get<double>()))return;
        viewport=r;layout_video();
    }else if(name=="focus"){focus_mode=command.value("enabled",false);layout_video();}
    else if(name=="visible"){visible=command.value("visible",true);if(!visible)release_pointer();layout_video();}
    else if(name=="relayout")layout_video();
    else if(name=="audio")change_audio(command);
}
static void test_source(){
    GError*error=nullptr;
    auto*pipeline=gst_parse_launch("videotestsrc name=test_source is-live=true pattern=smpte ! video/x-raw,format=NV12,width=1920,height=1080,framerate=60/1 ! d3d11upload ! d3d11videosink name=cast_sink sync=true",&error);
    if(error){cast_video_error(error->message);g_error_free(error);return;}
    cast_video_pipeline(pipeline);cast_ready();
    gst_element_set_state(pipeline,GST_STATE_PLAYING);
    GstBus*bus=gst_element_get_bus(pipeline);
    while(!quitting){auto*message=gst_bus_timed_pop_filtered(bus,100*GST_MSECOND,(GstMessageType)(GST_MESSAGE_ERROR|GST_MESSAGE_EOS));if(message){if(GST_MESSAGE_TYPE(message)==GST_MESSAGE_ERROR){GError*e=nullptr;gchar*debug=nullptr;gst_message_parse_error(message,&e,&debug);cast_video_error(e->message);g_clear_error(&e);g_free(debug);}gst_message_unref(message);break;}}
    gst_element_set_state(pipeline,GST_STATE_NULL);gst_object_unref(bus);gst_object_unref(pipeline);
}
int main(int argc,char**argv){
    // Keep the original stdout for JSON, redirect all upstream printf/g_print to stderr.
    event_output=_fdopen(_dup(_fileno(stdout)),"wb");_dup2(_fileno(stderr),_fileno(stdout));setvbuf(stdout,nullptr,_IONBF,0);
    int unicode_count=0;LPWSTR*unicode_args=CommandLineToArgvW(GetCommandLineW(),&unicode_count);
    std::vector<std::string>utf8_args;
    std::vector<char*>utf8_pointers;
    for(int i=0;i<unicode_count;i++){
        int length=WideCharToMultiByte(CP_UTF8,0,unicode_args[i],-1,nullptr,0,nullptr,nullptr);
        std::string value(length,'\0');WideCharToMultiByte(CP_UTF8,0,unicode_args[i],-1,&value[0],length,nullptr,nullptr);value.resize(length-1);utf8_args.push_back(std::move(value));
    }
    LocalFree(unicode_args);for(auto&arg:utf8_args)utf8_pointers.push_back(&arg[0]);argc=unicode_count;argv=utf8_pointers.data();
    std::string name="iPhone Cast";bool test=false,force_software=false;
    for(int i=1;i<argc;i++){
        std::string arg=argv[i];
        if(arg=="--parent"&&i+1<argc)parent_window=reinterpret_cast<HWND>(std::stoull(argv[++i]));
        else if(arg=="--name"&&i+1<argc)name=argv[++i];else if(arg=="--test")test=true;else if(arg=="--software")force_software=true;
    }
    if(!parent_window||!IsWindow(parent_window)){emit({{"event","error"},{"message","无效的 Tauri 父窗口"}});return 2;}
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    ui_thread=GetCurrentThreadId();MSG startup;PeekMessageW(&startup,nullptr,0,0,PM_NOREMOVE);
    WNDCLASSW klass{};klass.lpfnWndProc=window_proc;klass.hInstance=GetModuleHandleW(nullptr);klass.lpszClassName=L"ChengJingVideoSurface";klass.hCursor=LoadCursorW(nullptr,MAKEINTRESOURCEW(32512));klass.hbrBackground=(HBRUSH)GetStockObject(BLACK_BRUSH);RegisterClassW(&klass);
    video_window=CreateWindowExW(WS_EX_NOPARENTNOTIFY,klass.lpszClassName,L"iPhone Cast Video",WS_CHILD|WS_CLIPSIBLINGS,0,0,1,1,parent_window,nullptr,klass.hInstance,nullptr);
    if(!video_window){emit({{"event","error"},{"message","无法创建原生视频窗口"}});return 3;}
    gst_init(nullptr,nullptr);
    auto*hardware=gst_element_factory_make("d3d11h264dec",nullptr);bool hardware_ok=false;
    if(hardware){hardware_ok=!force_software&&gst_element_set_state(hardware,GST_STATE_READY)!=GST_STATE_CHANGE_FAILURE;gst_element_set_state(hardware,GST_STATE_NULL);gst_object_unref(hardware);}
    hardware_decoder=hardware_ok&&!test;
    decoder_name=hardware_ok?"D3D11 硬件解码":"软件解码";
    std::thread([]{
        std::string line;
        while(std::getline(std::cin,line)){
            if(line.size()>16384)continue;
            auto parsed=json::parse(line,nullptr,false);
            if(!parsed.is_object())continue;
            auto*command=new json(std::move(parsed));
            if(!PostThreadMessageW(ui_thread,WM_COMMAND_LINE,0,reinterpret_cast<LPARAM>(command)))delete command;
        }
        PostThreadMessageW(ui_thread,WM_QUIT,0,0);
    }).detach();
    std::thread([]{
        auto previous=Clock::now();unsigned long long last_received=0,last_presented=0;
        while(!quitting){std::this_thread::sleep_for(std::chrono::seconds(1));auto now=Clock::now();double elapsed=std::chrono::duration<double>(now-previous).count();auto r=received.load(),p=presented.load();emit({{"event","stats"},{"receivedFps",r>=last_received?double(r-last_received)/elapsed:0.0},{"presentedFps",p>=last_presented?double(p-last_presented)/elapsed:0.0},{"dropped",dropped.load()}});last_received=r;last_presented=p;previous=now;}
    }).detach();
    std::thread([=]{
        if(test){test_source();return;}
        std::vector<std::string>args={"cast-receiver","-n",name,"-nh","-fps","60","-s","1920x1080@60","-vs","d3d11videosink","-as","wasapisink","-vd",hardware_ok?"d3d11h264dec":"avdec_h264","-vc",hardware_ok?"d3d11convert":"videoconvert","-vsync","-reset","10"};
        // Keep trust material and configuration isolated from other AirPlay applications.
        if(auto*data=getenv("CAST_DATA_DIR")){
            args.push_back("-key");args.push_back(std::string(data)+"/receiver-key.pem");
            std::string config=std::string(data)+"/receiver.conf";
            FILE*file=fopen(config.c_str(),"ab");if(file)fclose(file);
            args.push_back("-rc");args.push_back(config);
        }
        std::vector<char*>pointers;for(auto&arg:args)pointers.push_back(&arg[0]);pointers.push_back(nullptr);
        int code=uxplay_main(int(args.size()),pointers.data());
        emit({{"event","error"},{"message","AirPlay 接收循环已退出"},{"code",code}});PostThreadMessageW(ui_thread,WM_QUIT,0,0);
    }).detach();
    MSG message;
    while(GetMessageW(&message,nullptr,0,0)>0){
        if(message.message==WM_COMMAND_LINE){auto*command=reinterpret_cast<json*>(message.lParam);try{process_command(*command);}catch(const std::exception&e){emit({{"event","warning"},{"message",std::string("无效的接收端命令：")+e.what()}});}delete command;}
        else if(message.message==WM_LAYOUT_VIDEO)layout_video();
        else if(message.message==WM_CLEAR_VIDEO){release_pointer();ShowWindow(video_window,SW_HIDE);InvalidateRect(video_window,nullptr,TRUE);}
        else{TranslateMessage(&message);DispatchMessageW(&message);}
    }
    quitting=true;release_pointer();ShowWindow(video_window,SW_HIDE);
    if(!test){cast_request_shutdown();Sleep(300);}
    // No detached thread is allowed to run during C++ static destruction.
    // The OS closes all handles; upstream normally finishes its cleanup first.
    std::_Exit(0);
}
