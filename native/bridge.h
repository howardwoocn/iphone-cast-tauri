#pragma once
#include <gst/gst.h>
#ifdef __cplusplus
extern "C" {
#endif
void cast_video_pipeline(GstElement *pipeline);
void cast_audio_pipeline(GstElement *pipeline);
void cast_audio_removed(GstElement *pipeline);
void cast_remote_volume(GstElement *volume, double value);
void cast_disconnected(void);
void cast_ready(void);
void cast_audio_error(const char *message);
void cast_video_error(const char *message);
void cast_request_shutdown(void);
#ifdef __cplusplus
}
#endif
