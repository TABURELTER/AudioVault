"""Inspect the real Tauri WebView through its opt-in local debugging port."""
import base64
import json
import sys
import urllib.request
from pathlib import Path
import websocket
sys.stdout.reconfigure(encoding='utf-8')

class Desktop:
    def __init__(self, port=9223):
        targets = json.load(urllib.request.urlopen(f'http://127.0.0.1:{port}/json/list'))
        pages = [target for target in targets if target.get('type') == 'page']
        fallback = next((target for target in pages if target['url'] == 'about:blank'), pages[0])
        target = next((target for target in pages if target['url'].startswith(('http://tauri.localhost', 'http://127.0.0.1:5173'))), fallback)
        self.ws = websocket.create_connection(target['webSocketDebuggerUrl'], timeout=30, suppress_origin=True)
        self.seq = 0

    def call(self, method, params=None):
        self.seq += 1
        self.ws.send(json.dumps(dict(id=self.seq, method=method, params=params or {})))
        while True:
            result = json.loads(self.ws.recv())
            if result.get('id') == self.seq:
                if 'error' in result:
                    raise RuntimeError(result['error'])
                return result.get('result', {})

    def evaluate(self, expression):
        r = self.call('Runtime.evaluate', dict(expression=expression, returnByValue=True, awaitPromise=True))
        if 'exceptionDetails' in r:
            raise RuntimeError(r['exceptionDetails'])
        return r.get('result', {}).get('value')

    def screenshot(self, path):
        data = self.call('Page.captureScreenshot', dict(format='png'))['data']
        Path(path).write_bytes(base64.b64decode(data))

if __name__ == '__main__':
    app = Desktop()
    if sys.argv[1] == 'shot':
        app.screenshot(sys.argv[2])
    else:
        print(json.dumps(app.evaluate(sys.argv[2]), ensure_ascii=False, indent=2))
