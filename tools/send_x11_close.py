"""Send the normal WM_DELETE_WINDOW request to a CI window without a window manager."""
import ctypes
import ctypes.util
import sys


class ClientData(ctypes.Union):
    _fields_ = [("b", ctypes.c_byte * 20), ("s", ctypes.c_short * 10), ("l", ctypes.c_long * 5)]


class ClientMessage(ctypes.Structure):
    _fields_ = [
        ("type", ctypes.c_int), ("serial", ctypes.c_ulong), ("send_event", ctypes.c_int),
        ("display", ctypes.c_void_p), ("window", ctypes.c_ulong), ("message_type", ctypes.c_ulong),
        ("format", ctypes.c_int), ("data", ClientData),
    ]


class XEvent(ctypes.Union):
    # XEvent is defined as a union padded to 24 longs in Xlib.
    _fields_ = [("client", ClientMessage), ("pad", ctypes.c_long * 24)]


x11 = ctypes.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
x11.XOpenDisplay.argtypes = [ctypes.c_char_p]
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XInternAtom.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int]
x11.XInternAtom.restype = ctypes.c_ulong
x11.XSendEvent.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_long, ctypes.POINTER(XEvent)]
x11.XSendEvent.restype = ctypes.c_int
x11.XFlush.argtypes = [ctypes.c_void_p]
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]

display = x11.XOpenDisplay(None)
if not display:
    raise RuntimeError("cannot open X11 display")
window = int(sys.argv[1], 0)
event = XEvent()
event.client.type = 33  # ClientMessage
event.client.display = display
event.client.window = window
event.client.message_type = x11.XInternAtom(display, b"WM_PROTOCOLS", 0)
event.client.format = 32
event.client.data.l[0] = x11.XInternAtom(display, b"WM_DELETE_WINDOW", 0)
if not x11.XSendEvent(display, window, 0, 0, ctypes.byref(event)):
    raise RuntimeError("could not send WM_DELETE_WINDOW")
x11.XFlush(display)
x11.XCloseDisplay(display)
