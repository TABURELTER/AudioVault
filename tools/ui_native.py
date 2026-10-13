"""Verify native Windows chrome and capture the actual desktop window."""
import ctypes
from ctypes import wintypes
import json
from pathlib import Path
import time
import sys
from PIL import ImageGrab
from ui_cdp import Desktop

app = Desktop()
app.call('Emulation.clearDeviceMetricsOverride')
user = ctypes.windll.user32
user.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
user.FindWindowW.restype = wintypes.HWND
hwnd = user.FindWindowW(None, 'AudioVault')
if not hwnd:
    raise RuntimeError('AudioVault window not found')

user.ShowWindow(hwnd, 9)
time.sleep(.2)
user.ShowWindow(hwnd, 6)
minimized = bool(user.IsIconic(hwnd))
user.ShowWindow(hwnd, 9)
user.ShowWindow(hwnd, 3)
maximized = bool(user.IsZoomed(hwnd))
user.ShowWindow(hwnd, 9)
time.sleep(.3)

window = wintypes.RECT()
client = wintypes.RECT()
user.GetWindowRect(hwnd, ctypes.byref(window))
user.GetClientRect(hwnd, ctypes.byref(client))
extra_width = window.right - window.left - client.right
extra_height = window.bottom - window.top - client.bottom
user.SetWindowPos(hwnd, None, 30, 30, 1366 + extra_width, 768 + extra_height, 0x0004)
user.SetForegroundWindow(hwnd)
time.sleep(.5)
user.GetWindowRect(hwnd, ctypes.byref(window))
caption_x = window.left + 250
caption_y = window.top + 16
caption_hit = user.SendMessageW(hwnd, 0x0084, 0, (caption_y << 16) | (caption_x & 0xffff))
# HTCAPTION validates the OS drag region without sending mouse input to other apps.
dragged = caption_hit == 2
style = user.GetWindowLongW(hwnd, -16)
result = dict(minimize=minimized, maximize=maximized, restored=not bool(user.IsIconic(hwnd)), caption_drag_region=dragged, native_caption=bool(style & 0x00C00000), native_system_menu=bool(style & 0x00080000), viewport=app.evaluate('({width:innerWidth,height:innerHeight,scale:devicePixelRatio})'))
assert all(result[key] for key in ['minimize','maximize','restored','caption_drag_region','native_caption','native_system_menu']), result
output = Path(__file__).resolve().parents[1] / 'artifacts' / 'ui'
ImageGrab.grab(bbox=(window.left,window.top,window.right,window.bottom), all_screens=True).save(output / 'native-window.png')
if '--close' in sys.argv:
    user.PostMessageW(hwnd, 0x0010, 0, 0)
    deadline = time.monotonic() + 5
    while user.IsWindow(hwnd) and time.monotonic() < deadline:
        time.sleep(.1)
    result['close'] = not bool(user.IsWindow(hwnd))
    assert result['close'], result
(output / 'native-validation.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
print(json.dumps(result, indent=2))
