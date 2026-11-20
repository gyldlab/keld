# GH-1081 round-1 research only: read-only X11 census of top-level windows
# (root children and their children) with _NET_WM_PID and title. Uses only
# libX11 through ctypes, so no x11-utils package is needed on the runner.
# Prints one JSON line; never maps, moves or sends events to any window.
# Round 2 adds read-only `pixels <id>` and `popups` modes.
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


class XWindowAttributes(ctypes.Structure):
    _fields_ = [
        ("x", ctypes.c_int), ("y", ctypes.c_int), ("width", ctypes.c_int),
        ("height", ctypes.c_int), ("border_width", ctypes.c_int), ("depth", ctypes.c_int),
        ("visual", ctypes.c_void_p), ("root", Window), ("class_", ctypes.c_int),
        ("bit_gravity", ctypes.c_int), ("win_gravity", ctypes.c_int),
        ("backing_store", ctypes.c_int), ("backing_planes", ctypes.c_ulong),
        ("backing_pixel", ctypes.c_ulong), ("save_under", ctypes.c_int),
        ("colormap", ctypes.c_ulong), ("map_installed", ctypes.c_int),
        ("map_state", ctypes.c_int), ("all_event_masks", ctypes.c_long),
        ("your_event_mask", ctypes.c_long), ("do_not_propagate_mask", ctypes.c_long),
        ("override_redirect", ctypes.c_int), ("screen", ctypes.c_void_p),
    ]


x11.XGetWindowAttributes.restype = ctypes.c_int
x11.XGetWindowAttributes.argtypes = [ctypes.c_void_p, Window, ctypes.POINTER(XWindowAttributes)]
x11.XTranslateCoordinates.restype = ctypes.c_int
x11.XTranslateCoordinates.argtypes = [
    ctypes.c_void_p, Window, Window, ctypes.c_int, ctypes.c_int,
    ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int), ctypes.POINTER(Window),
]
x11.XGetImage.restype = ctypes.c_void_p
x11.XGetImage.argtypes = [
    ctypes.c_void_p, Window, ctypes.c_int, ctypes.c_int, ctypes.c_uint, ctypes.c_uint,
    ctypes.c_ulong, ctypes.c_int,
]
x11.XGetPixel.restype = ctypes.c_ulong
x11.XGetPixel.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int]
x11.XDestroyImage.argtypes = [ctypes.c_void_p]


def attributes(window):
    attrs = XWindowAttributes()
    if not x11.XGetWindowAttributes(display, window, ctypes.byref(attrs)):
        return None
    return attrs


def pixels(window):
    # GH-1081 round 2: read-only sample of what the screen shows inside the
    # window (root-window pixels at the window's absolute geometry).
    attrs = attributes(window)
    if attrs is None:
        return {"ok": False, "error": "no attributes"}
    root = x11.XDefaultRootWindow(display)
    x = ctypes.c_int()
    y = ctypes.c_int()
    child = Window()
    x11.XTranslateCoordinates(display, window, root, 0, 0, ctypes.byref(x), ctypes.byref(y),
                              ctypes.byref(child))
    image = x11.XGetImage(display, root, x.value, y.value, attrs.width, attrs.height,
                          0xFFFFFFFF, 2)
    if not image:
        return {"ok": False, "error": "XGetImage failed"}
    dark = light = total = 0
    try:
        for row in range(16):
            for column in range(16):
                px = int(attrs.width * (0.2 + 0.6 * column / 15))
                py = int(attrs.height * (0.2 + 0.6 * row / 15))
                value = x11.XGetPixel(image, px, py)
                red, green, blue = (value >> 16) & 255, (value >> 8) & 255, value & 255
                total += 1
                if max(red, green, blue) < 64:
                    dark += 1
                if min(red, green, blue) > 200:
                    light += 1
        center = x11.XGetPixel(image, attrs.width // 2, attrs.height // 2)
    finally:
        x11.XDestroyImage(image)
    return {"ok": True, "x": x.value, "y": y.value, "width": attrs.width,
            "height": attrs.height, "dark": dark / total, "light": light / total,
            "center": "#%06x" % (center & 0xFFFFFF), "map_state": attrs.map_state}


def popups():
    # Viewable override-redirect top-level windows (GTK context menus).
    found = []
    for window in children(x11.XDefaultRootWindow(display)):
        attrs = attributes(window)
        if attrs is not None and attrs.override_redirect and attrs.map_state == 2:
            found.append({"id": window, "width": attrs.width, "height": attrs.height})
    return found


def census():
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
    return rows


mode = sys.argv[1] if len(sys.argv) > 1 else "census"
if mode == "pixels":
    print("X11_PIXELS " + json.dumps(pixels(int(sys.argv[2])), sort_keys=True))
elif mode == "popups":
    print("X11_POPUPS " + json.dumps(popups(), sort_keys=True))
else:
    print("X11_CENSUS " + json.dumps(census(), sort_keys=True))
