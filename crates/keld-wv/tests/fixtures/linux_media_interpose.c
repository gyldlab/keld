/*
 * KEL-132 evidence fixture, not product code. Supported only on Linux's
 * dynamic-loader LD_PRELOAD contract (https://man7.org/linux/man-pages/man8/ld.so.8.html)
 * with Keld's pinned WebKit2GTK 4.1 ABI; the permission default and API are
 * documented at https://webkitgtk.org/reference/webkit2gtk/stable/class.UserMediaPermissionRequest.html.
 * CI compiles this against its installed WebKitGTK headers and fails on any
 * missing/interposition-incompatible symbol rather than claiming other OSes.
 */
#define _GNU_SOURCE

#include <dlfcn.h>
#include <fcntl.h>
#include <glib.h>
#include <limits.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <sys/syscall.h>
#include <unistd.h>
#include <webkit2/webkit2.h>

typedef void (*load_uri_fn)(WebKitWebView *, const gchar *);
typedef void (*permission_fn)(WebKitPermissionRequest *);
typedef gulong (*signal_connect_data_fn)(gpointer, const gchar *, GCallback,
                                         gpointer, GClosureNotify,
                                         GConnectFlags);

static void *required_symbol(const char *name) {
  void *symbol = dlsym(RTLD_NEXT, name);
  if (symbol == NULL) {
    const char *error = dlerror();
    dprintf(STDERR_FILENO, "KELD_MEDIA_INTERPOSE_FAIL symbol=%s error=%s\n",
            name, error == NULL ? "unknown" : error);
    _exit(125);
  }
  return symbol;
}

static load_uri_fn real_load_uri(void) {
  void *symbol = required_symbol("webkit_web_view_load_uri");
  load_uri_fn function = NULL;
  memcpy(&function, &symbol, sizeof(function));
  return function;
}

static permission_fn real_deny(void) {
  void *symbol = required_symbol("webkit_permission_request_deny");
  permission_fn function = NULL;
  memcpy(&function, &symbol, sizeof(function));
  return function;
}

static signal_connect_data_fn real_signal_connect_data(void) {
  void *symbol = required_symbol("g_signal_connect_data");
  signal_connect_data_fn function = NULL;
  memcpy(&function, &symbol, sizeof(function));
  return function;
}

static const char *required_nonce(void) {
  const char *nonce = getenv("KELD_MEDIA_NONCE");
  if (nonce == NULL || nonce[0] == '\0') {
    dprintf(STDERR_FILENO,
            "KELD_MEDIA_INTERPOSE_FAIL KELD_MEDIA_NONCE is unset\n");
    _exit(125);
  }
  return nonce;
}

static const char *current_exe(char path[PATH_MAX]) {
  const ssize_t length = readlink("/proc/self/exe", path, PATH_MAX - 1);
  if (length < 0 || length >= PATH_MAX - 1) {
    dprintf(STDERR_FILENO,
            "KELD_MEDIA_INTERPOSE_FAIL cannot resolve /proc/self/exe\n");
    _exit(125);
  }
  path[length] = '\0';
  return path;
}

static void trace_line(const char *format, ...) {
  const char *path = getenv("KELD_MEDIA_TRACE");
  if (path == NULL || path[0] == '\0') {
    dprintf(STDERR_FILENO,
            "KELD_MEDIA_INTERPOSE_FAIL KELD_MEDIA_TRACE is unset\n");
    _exit(125);
  }
  int fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
  if (fd < 0) {
    dprintf(STDERR_FILENO, "KELD_MEDIA_INTERPOSE_FAIL cannot open trace\n");
    _exit(125);
  }
  va_list args;
  va_start(args, format);
  vdprintf(fd, format, args);
  va_end(args);
  close(fd);
}

void webkit_web_view_load_uri(WebKitWebView *web_view, const gchar *uri) {
  char exe[PATH_MAX];
  WebKitSettings *settings = webkit_web_view_get_settings(web_view);
  webkit_settings_set_enable_media_stream(settings, TRUE);
  webkit_settings_set_enable_mock_capture_devices(settings, TRUE);
  trace_line("setup nonce=%s mock_capture_devices=true webview=%p exe=%s pid=%ld tid=%ld uri=%s\n",
             required_nonce(), (void *)web_view, current_exe(exe), (long)getpid(),
             (long)syscall(SYS_gettid), uri);
  real_load_uri()(web_view, uri);
}

gulong g_signal_connect_data(gpointer instance, const gchar *detailed_signal,
                             GCallback c_handler, gpointer data,
                             GClosureNotify destroy_data,
                             GConnectFlags connect_flags) {
  const gulong handler_id = real_signal_connect_data()(
      instance, detailed_signal, c_handler, data, destroy_data, connect_flags);
  if (g_strcmp0(detailed_signal, "permission-request") == 0) {
    char exe[PATH_MAX];
    Dl_info caller = {0};
    const void *return_address = __builtin_return_address(0);
    const char *caller_name = "unknown";
    if (return_address != NULL && dladdr(return_address, &caller) != 0 &&
        caller.dli_fname != NULL) {
      caller_name = caller.dli_fname;
    }
    trace_line("registration nonce=%s signal=permission-request handler=%lu webview=%p caller=%s exe=%s pid=%ld tid=%ld\n",
               required_nonce(), handler_id, instance, caller_name,
               current_exe(exe), (long)getpid(), (long)syscall(SYS_gettid));
  }
  return handler_id;
}

void webkit_permission_request_deny(WebKitPermissionRequest *request) {
  char exe[PATH_MAX];
  const char *kind = "other";
  if (WEBKIT_IS_USER_MEDIA_PERMISSION_REQUEST(request)) {
    WebKitUserMediaPermissionRequest *media =
        WEBKIT_USER_MEDIA_PERMISSION_REQUEST(request);
    if (webkit_user_media_permission_is_for_video_device(media)) {
      kind = "camera";
    } else if (webkit_user_media_permission_is_for_audio_device(media)) {
      kind = "microphone";
    }
  }

  Dl_info caller = {0};
  const void *return_address = __builtin_return_address(0);
  const char *caller_name = "unknown";
  if (return_address != NULL && dladdr(return_address, &caller) != 0 &&
      caller.dli_fname != NULL) {
    caller_name = caller.dli_fname;
  }

  const gboolean force_allow =
      g_strcmp0(getenv("KELD_MEDIA_FORCE_ALLOW"), "1") == 0;
  trace_line("callback nonce=%s kind=%s action=%s caller=%s exe=%s pid=%ld tid=%ld\n",
             required_nonce(), kind, force_allow ? "force_allow" : "deny",
             caller_name, current_exe(exe), (long)getpid(),
             (long)syscall(SYS_gettid));
  if (force_allow) {
    webkit_permission_request_allow(request);
  } else {
    real_deny()(request);
  }
}
