"""Apply only the narrow Windows bridge hooks to pristine UxPlay 1.73.6.

Idempotent; fails if an upstream anchor changed rather than silently drifting.
The checked-in vendor tree already contains these hooks.
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def change(file, old, new):
    path = ROOT / file
    text = path.read_text(encoding='utf-8')
    if new in text:
        return
    if old not in text:
        raise RuntimeError(f'Upstream anchor missing: {file}: {old[:70]}')
    path.write_text(text.replace(old, new), encoding='utf-8', newline='\n')

change('vendor/UxPlay/renderers/video_renderer.c', '#include "video_renderer.h"', '#include "video_renderer.h"\n#include "bridge.h"')
change('vendor/UxPlay/renderers/video_renderer.c', '            g_assert (renderer_type[i]->pipeline);', '            g_assert (renderer_type[i]->pipeline);\n            cast_video_pipeline(renderer_type[i]->pipeline);')
change('vendor/UxPlay/renderers/video_renderer.c', 'gst_message_parse_error (message, &err, &debug);', 'gst_message_parse_error (message, &err, &debug);\n        cast_video_error(err->message);')
change('vendor/UxPlay/renderers/video_renderer.c', '                g_string_append(launch, "videoscale ! ");', '                if (!strstr(converter, "d3d11convert")) g_string_append(launch, "videoscale ! ");')
change('vendor/UxPlay/renderers/video_renderer.c', '        //gst_element_set_state (renderer->playbin, GST_STATE_NULL);', '        cast_disconnected();\n        //gst_element_set_state (renderer->playbin, GST_STATE_NULL);')
change('vendor/UxPlay/renderers/audio_renderer.c', '#include "audio_renderer.h"', '#include "audio_renderer.h"\n#include "bridge.h"')
change('vendor/UxPlay/renderers/audio_renderer.c', '        g_assert (renderer_type[i]->pipeline);', '        g_assert (renderer_type[i]->pipeline);\n        cast_audio_pipeline(renderer_type[i]->pipeline);')
change('vendor/UxPlay/renderers/audio_renderer.c', 'gst_message_parse_error (message, &err, &debug);', 'gst_message_parse_error (message, &err, &debug);\n        cast_audio_error(err->message);')
change('vendor/UxPlay/renderers/audio_renderer.c', '        gst_object_unref (renderer_type[i]->pipeline);', '        cast_audio_removed(renderer_type[i]->pipeline);\n        gst_object_unref (renderer_type[i]->pipeline);')
change('vendor/UxPlay/renderers/audio_renderer.c', '    g_object_set(renderer->volume, "volume", volume, NULL);', '    cast_remote_volume(renderer->volume, volume);')
change('vendor/UxPlay/renderers/audio_renderer.c', '        g_main_loop_quit( (GMainLoop *) loop);', '        /* Audio failure is isolated: keep video mirroring alive. */')
change('vendor/UxPlay/uxplay.cpp', '#include "lib/dnssd.h"', '#include "lib/dnssd.h"\n#include "../../native/bridge.h"')
change('vendor/UxPlay/uxplay.cpp', 'extern "C" void video_reset(void *cls, reset_type_t type) {', 'extern "C" void video_reset(void *cls, reset_type_t type) {\n    if (type == RESET_TYPE_RTP_SHUTDOWN || type == RESET_TYPE_HLS_SHUTDOWN || type == RESET_TYPE_NOHOLD) cast_disconnected();')
change('vendor/UxPlay/uxplay.cpp', '    if (open_connections == 0) {', '    if (open_connections == 0) {\n        cast_disconnected();')
change('vendor/UxPlay/uxplay.cpp', '    reconnect:\n', '    cast_ready();\n    reconnect:\n')
change('vendor/UxPlay/uxplay.cpp', 'static BOOL WINAPI CtrlHandler(DWORD signal) {', 'extern "C" void cast_request_shutdown(void) {\n    if (gmainloop) g_idle_add(handle_signal, NULL);\n}\n\nstatic BOOL WINAPI CtrlHandler(DWORD signal) {')
change('vendor/UxPlay/uxplay.cpp', '    if (!SetConsoleCtrlHandler(CtrlHandler, TRUE)) {\n        LOGE("Could not set control handler");\n        exit(1);\n    }', '#ifndef IPHONE_CAST_BRIDGE\n    if (!SetConsoleCtrlHandler(CtrlHandler, TRUE)) {\n        LOGE("Could not set control handler");\n        exit(1);\n    }\n#endif')
# Windows upstream dynamically resolves DNS-SD from dnssd.dll; no Bonjour SDK import library is needed.
change('vendor/UxPlay/lib/CMakeLists.txt', '      target_link_libraries(airplay ${DNSSD} )', '      # DNS-SD functions are loaded from the system dnssd.dll at runtime.')
change('vendor/UxPlay/lib/CMakeLists.txt', '      find_path(DNSSD_INCLUDE_DIR dns_sd.h HINTS ${BONJOUR_SDK}/Include )', '      find_path(DNSSD_INCLUDE_DIR dns_sd.h HINTS ${CMAKE_CURRENT_SOURCE_DIR}/../../../native/third_party NO_DEFAULT_PATH)')
print('UxPlay bridge hooks applied.')
