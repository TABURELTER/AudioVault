"""Runtime regression checks and screenshots against the actual Tauri app.

Launch with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223.
Read-only catalog checks use the real backend. Mutation checks replace IPC with
temporary fixtures and restore it, so the user's collection remains intact.
Requires Python and websocket-client, already available on this machine.
"""
import ctypes
import json
import time
from pathlib import Path
from ui_cdp import Desktop

app = Desktop()
output = Path(__file__).resolve().parents[1] / 'artifacts' / 'ui'
output.mkdir(parents=True, exist_ok=True)
checks = []

def check(name, condition, details=None):
    if not condition:
        raise AssertionError(f'{name}: {details}')
    checks.append(dict(name=name, passed=True, details=details))
    print(f'PASS {name}', flush=True)

def wait(expression, timeout=15):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        result = app.evaluate(f'Boolean({expression})')
        if result:
            return result
        time.sleep(.12)
    raise TimeoutError(expression)

def click(selector):
    app.evaluate(f'document.querySelector({json.dumps(selector)}).click()')
    time.sleep(.15)

def navigate(label):
    app.evaluate(f'Array.from(document.querySelectorAll(".nav-item")).find(b => b.querySelector("span")?.textContent === {json.dumps(label)}).click()')
    time.sleep(.3)

def value(selector, value):
    app.evaluate(f'''(() => {{ const el = document.querySelector({json.dumps(selector)});
      Object.getOwnPropertyDescriptor(el.tagName === 'SELECT' ? HTMLSelectElement.prototype : HTMLInputElement.prototype, 'value').set.call(el, {json.dumps(value)});
      el.dispatchEvent(new Event(el.tagName === 'SELECT' ? 'change' : 'input', {{ bubbles: true }})); }})()''')
    time.sleep(.3)

def key(name, modifiers=0):
    virtual = {'Enter':13, 'ArrowDown':40, 'ArrowUp':38, 'Escape':27, 'Tab':9}.get(name, 0)
    params = dict(type='keyDown', key=name, modifiers=modifiers, windowsVirtualKeyCode=virtual)
    if name == 'Enter':
        params.update(text='\r', unmodifiedText='\r', code='Enter')
    app.call('Input.dispatchKeyEvent', params)
    app.call('Input.dispatchKeyEvent', dict(type='keyUp', key=name, modifiers=modifiers, windowsVirtualKeyCode=virtual))
    time.sleep(.1)

def size(width, height, scale=1):
    app.call('Emulation.setDeviceMetricsOverride', dict(width=width, height=height, deviceScaleFactor=scale, mobile=False))
    time.sleep(.2)

def screenshot(name):
    app.screenshot(str(output / f'{name}.png'))

def layout():
    return app.evaluate('''(() => {
      const elements = [...document.querySelectorAll('.book-columns, .ebook-columns, .toolbar, .detail-panel, .storage-row, .comparison, .sidebar nav')];
      return { width: innerWidth, height: innerHeight, bodyOverflow: document.documentElement.scrollWidth > innerWidth,
        overflowing: elements.filter(e => e.scrollWidth > e.clientWidth + 2).map(e => e.className),
        mountedRows: document.querySelectorAll('.book-row').length };
    })()''')

app.evaluate('location.reload()')
wait('document.querySelectorAll(".book-row").length > 0')
app.evaluate('window.__uiErrors = []; window.addEventListener("error", e => window.__uiErrors.push(e.message)); window.addEventListener("unhandledrejection", e => window.__uiErrors.push(String(e.reason)));')
stats = app.evaluate("window.__TAURI_INTERNALS__.invoke('get_stats')")
check('Real indexed catalog', stats['total_books'] > 1000, {k: stats[k] for k in ['total_books', 'online_volumes', 'total_ebooks']})

for width, height in [(1280,720), (1366,768), (1440,900), (1920,1080)]:
    size(width, height)
    state = layout()
    check(f'Library layout {width}x{height}', not state['bodyOverflow'] and not state['overflowing'], state)
    check(f'Virtualization {width}x{height}', state['mountedRows'] < 35, state['mountedRows'])
    screenshot(f'library-{width}x{height}')

# Check library pagination against real data, including the last page.
size(1366,768)
for _ in range(10):
    footer = app.evaluate('document.querySelector(".collection-footer").innerText')
    if 'Загрузить ещё' not in footer:
        break
    app.evaluate('document.querySelector(".virtual-list").scrollTop = 100000000')
    time.sleep(.5)
check('Library pages reach full collection', str(stats['total_books']) in app.evaluate('document.querySelector(".collection-footer").innerText').replace('\u00a0', ''), footer)
check('Scrolled list remains virtualized', app.evaluate('document.querySelectorAll(".book-row").length') < 35)
app.evaluate('document.querySelector(".virtual-list").scrollTop = 0')
time.sleep(.2)
app.evaluate('document.querySelector(".book-list").focus()')
key('ArrowDown')
check('Arrow navigation focuses a book', app.evaluate('!!document.activeElement.dataset.bookId'))
key('Enter')
wait('document.querySelector(".detail-summary h2")')
check('Enter opens selected book', app.evaluate('!!document.querySelector(".detail-panel")'))
key('Escape')
key('k', 2)
check('Ctrl K focuses search', app.evaluate('document.activeElement.dataset.search') == 'global')
value('[data-search="global"]', 'Кинг')
wait('document.querySelectorAll(".book-row").length > 0 && document.querySelector(".collection-footer").innerText.includes("книг") && !document.querySelector(".collection-footer").innerText.includes("Обновление")')
check('Cyrillic library search', app.evaluate('document.querySelectorAll(".book-row").length') > 0)
screenshot('library-search')
click('.book-row .title-button')
wait('document.querySelector(".detail-summary h2")')
detail_title = app.evaluate('document.querySelector(".detail-summary h2").textContent')
app.evaluate('window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {event:"app_event",payload:"volumes_changed"})')
time.sleep(.3)
check('Volume refresh retains selected inspector', app.evaluate('document.querySelector(".detail-summary h2")?.textContent') == detail_title)
state = layout()
check('Nonmodal details preserve collection', app.evaluate('!!document.querySelector(".book-list")') and not state['bodyOverflow'] and not state['overflowing'], state)
screenshot('book-details')
click('#book-tab-tracks')
check('Track list is virtualized', app.evaluate('document.querySelectorAll(".track-row").length') < 35)
key('Escape')
check('Escape closes details', app.evaluate('!document.querySelector(".detail-panel")'))
click('.book-row .plain-icon')
wait('document.querySelector(".context-menu")')
menu = app.evaluate('(() => { const r = document.querySelector(".context-menu").getBoundingClientRect(); return { right:r.right, bottom:r.bottom }; })()')
check('Context menu stays in viewport', menu['right'] <= 1366 and menu['bottom'] <= 768, menu)
check('Menu has keyboard focus', app.evaluate('!!document.activeElement.closest(".context-menu")'))
screenshot('book-context-menu')
key('Escape')
value('[data-search="global"]', 'unlikely-no-book-8492791')
wait('document.querySelector(".empty-state") && document.querySelector(".collection-footer").innerText.includes("0 книг")')
screenshot('library-no-results')
value('[data-search="global"]', '')
wait('document.querySelectorAll(".book-row").length > 0')
value('select[aria-label="Доступность книг"]', 'offline')
wait('document.querySelectorAll(".book-row").length > 0 && !document.querySelector(".collection-footer").innerText.includes("Обновление")')
check('Offline filter', app.evaluate('[...document.querySelectorAll(".book-storage small")].every(e => ["Недоступна", "Отключён"].includes(e.textContent))'))
screenshot('library-offline')
value('select[aria-label="Доступность книг"]', 'all')
click('button[aria-label="Обложки"]')
wait('document.querySelector(".book-card")')
screenshot('library-covers')
check('Cover grid is virtualized', app.evaluate('document.querySelectorAll(".book-card").length') < 100)
click('button[aria-label="Список"]')

for label, filename in [('Хранилища','storages'), ('Дубликаты','duplicates'), ('Проблемы','issues'), ('RuTracker','rutracker'), ('Загрузки','downloads'), ('Оптимизация','optimizer'), ('Задачи','tasks'), ('Настройки','settings'), ('Электронные книги','ebooks'), ('Поиск','search')]:
    navigate(label)
    time.sleep(.5)
    state = layout()
    check(f'{label} layout', not state['bodyOverflow'] and not state['overflowing'], state)
    screenshot(filename)
    size(1280,720)
    state = layout()
    check(f'{label} minimum layout', not state['bodyOverflow'] and not state['overflowing'], state)
    screenshot(filename + '-1280x720')
    size(1366,768)

navigate('Проблемы')
wait('document.querySelectorAll(".issue-row").length > 0')
app.evaluate('document.querySelector(".virtual-list").scrollTop = 1000000')
wait('document.querySelector(".collection-footer").innerText.includes("200")')
check('Inbox loads beyond first 100 issues', app.evaluate('document.querySelectorAll(".issue-row").length < 25'))
value('select[aria-label="Категория замечаний"]', 'missing_title')
wait('document.querySelectorAll(".issue-kind").length > 0 && [...document.querySelectorAll(".issue-kind")].every(e=>e.textContent === "Проверьте название")')
check('Inbox category uses backend filter', app.evaluate('[...document.querySelectorAll(".issue-kind")].every(e=>e.textContent === "Проверьте название")'))
screenshot('issues-filtered')
navigate('Поиск')
key('f', 2)
check('Ctrl F focuses page search', app.evaluate('document.activeElement.dataset.search === "local"'))
value('[data-search="local"]', 'lukyanenko')
wait('document.querySelectorAll(".book-row").length > 0')
check('Transliteration search uses real FTS backend', app.evaluate('document.querySelectorAll(".book-row").length') > 0)
screenshot('search-results')
navigate('Хранилища')
volume_ids = app.evaluate("window.__TAURI_INTERNALS__.invoke('get_volumes').then(v => v.map(x => x.id))")
click('.storage-row .storage-actions .btn-quiet')
wait('document.querySelectorAll(".book-row").length > 0')
check('Storage books uses volume filter', 'Seagate' in app.evaluate('document.querySelector(".collection-footer").innerText'))
screenshot('storage-books')
click('.toolbar-left .plain-icon')
navigate('Электронные книги')
wait('document.querySelectorAll(".ebook-columns.book-row").length > 0')
check('Ebooks use server pagination', app.evaluate('document.querySelectorAll(".ebook-columns.book-row").length') < 35)
click('.ebook-columns.book-row .title-button')
wait('document.querySelector(".detail-summary h2")')
check('Ebook details retain collection', app.evaluate('!!document.querySelector(".ebook-columns")'))
ebook_title = app.evaluate('document.querySelector(".detail-summary h2").textContent')
app.evaluate('window.__TAURI_INTERNALS__.invoke("plugin:event|emit", {event:"app_event",payload:"library_changed"})')
time.sleep(.3)
check('Catalog refresh retains ebook inspector', app.evaluate('document.querySelector(".detail-summary h2")?.textContent') == ebook_title)
state = layout()
check('Ebook details layout', not state['bodyOverflow'] and not state['overflowing'], state)
screenshot('ebook-details')
key('Escape')

# 125% equivalent content viewport: 1280/1.25 by 720/1.25.
navigate('Фонотека')
size(1024,576,1.25)
state = layout()
check('125 percent equivalent layout', not state['bodyOverflow'] and not state['overflowing'], state)
screenshot('library-125-percent')
size(1366,768)

check('No runtime JS errors', not app.evaluate('window.__uiErrors'), app.evaluate('window.__uiErrors'))
(output / 'validation.json').write_text(json.dumps(dict(catalog=stats, checks=checks), ensure_ascii=False, indent=2), encoding='utf-8')
print(f'{len(checks)} runtime checks passed; screenshots saved to {output}', flush=True)
