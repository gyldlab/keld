# GH-1081 round-1 research only: read-only X11 census of top-level windows
# (root children and their children) with _NET_WM_PID and title. Uses only
# libX11 through ctypes, so no x11-utils package is needed on the runner.
# Prints one JSON line; never maps, moves or sends events to any window.
import ctypes
import ctypes.util
import json
import sys

library = ctypes.util.find_library("X11") or "libX11.so.6"
x11 = ctypes.CDLL(library)
Window = ctypes.c_ulong
Atom = ctypes.c_ulong
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XDefaultRootWindow.restype = Window
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XInternAtom.restype = Atom
x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
x11.XQueryTree.restype = ctypes.c_int
x11.XQueryTree.argtypes = [
    ctypes.c_void_p, Window, ctypes.POINTER(Window), ctypes.POINTER(Window),
    ctypes.POINTER(ctypes.POINTER(Window)), ctypes.POINTER(ctypes.c_uint),
]
x11.XGetWindowProperty.restype = ctypes.c_int
x11.XGetWindowProperty.argtypes = [
    ctypes.c_void_p, Window, Atom, ctypes.c_long, ctypes.c_long, ctypes.c_int, Atom,
    ctypes.POINTER(Atom), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_ulong),
    ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.POINTER(ctypes.c_ubyte)),
]
x11.XFree.argtypes = [ctypes.c_void_p]

display = x11.XOpenDisplay(None)
if not display:
    print("X11_CENSUS_UNAVAILABLE no display")
    sys.exit(3)
ANY_PROPERTY_TYPE = 0
NET_WM_PID = x11.XInternAtom(display, b"_NET_WM_PID", 0)
NET_WM_NAME = x11.XInternAtom(display, b"_NET_WM_NAME", 0)
WM_NAME = 39  # XA_WM_NAME


def window_property(window, atom):
    actual_type = Atom()
    actual_format = ctypes.c_int()
    items = ctypes.c_ulong()
    remaining = ctypes.c_ulong()
    data = ctypes.POINTER(ctypes.c_ubyte)()
    status = x11.XGetWindowProperty(
        display, window, atom, 0, 1024, 0, ANY_PROPERTY_TYPE, ctypes.byref(actual_type),
        ctypes.byref(actual_format), ctypes.byref(items), ctypes.byref(remaining),
        ctypes.byref(data))
    if status != 0 or not data:
        return None
    try:
        if actual_format.value == 32:
            # Xlib returns 32-bit items as C longs.
            values = ctypes.cast(data, ctypes.POINTER(ctypes.c_long))
            return [values[index] for index in range(items.value)]
        if actual_format.value == 8:
            return bytes(data[index] for index in range(items.value))
        return None
    finally:
        x11.XFree(data)


def children(window):
    root = Window()
    parent = Window()
    listed = ctypes.POINTER(Window)()
    count = ctypes.c_uint()
    if not x11.XQueryTree(display, window, ctypes.byref(root), ctypes.byref(parent),
                          ctypes.byref(listed), ctypes.byref(count)):
        return []
    try:
        return [listed[index] for index in range(count.value)]
    finally:
        if listed:
            x11.XFree(listed)


rows = []
for depth, window in [(1, child) for child in children(x11.XDefaultRootWindow(display))]:
    for level, candidate in [(depth, window)] + [(depth + 1, nested) for nested in children(window)]:
        name = window_property(candidate, NET_WM_NAME) or window_property(candidate, WM_NAME)
        pid = window_property(candidate, NET_WM_PID)
        if name is None and pid is None:
            continue
        rows.append({
            "id": candidate,
            "depth": level,
            "pid": pid[0] if isinstance(pid, list) and pid else None,
            "name": name.decode("utf-8", "replace") if isinstance(name, bytes) else None,
        })
print("X11_CENSUS " + json.dumps(rows, sort_keys=True))
