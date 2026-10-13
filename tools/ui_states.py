"""Exercise uncommon UI states in a headless Edge preview with in-memory IPC fixtures.

Catalog files, credentials, downloads and duplicate decisions are never mutated.
These screenshots are named *-fixture to distinguish them from real-catalog QA.
"""
import json
import time
from pathlib import Path
from ui_cdp import Desktop

app = Desktop(port=9224)
app.call('Page.enable')
output = Path(__file__).resolve().parents[1] / 'artifacts' / 'ui'
results = []

def wait(expression, timeout=12):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if app.evaluate(f'Boolean({expression})'):
            return
        time.sleep(.12)
    raise AssertionError(expression)

def check(name, expression):
    condition = app.evaluate(expression)
    if not condition:
        raise AssertionError(name)
    results.append(dict(name=name, passed=True))
    print('PASS ' + name, flush=True)

def click(selector):
    app.evaluate(f'document.querySelector({json.dumps(selector)}).click()')
    time.sleep(.15)

def nav(label):
    app.evaluate(f'[...document.querySelectorAll(".nav-item")].find(b => b.querySelector("span")?.textContent === {json.dumps(label)}).click()')
    time.sleep(.4)

def value(selector, value):
    app.evaluate(f'''(() => {{ const el = document.querySelector({json.dumps(selector)});
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, {json.dumps(value)});
      el.dispatchEvent(new Event('input', {{ bubbles: true }})); }})()''')
    time.sleep(.3)

def shot(name):
    app.screenshot(str(output / (name + '-fixture.png')))

app.call('Emulation.setDeviceMetricsOverride', dict(width=1280, height=720, deviceScaleFactor=1, mobile=False))
fixture_bootstrap = """
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 1, unregisterCallback: () => {},
  invoke: async (cmd,args={}) => {
    if (cmd === 'get_stats') return {total_books:100000,total_copies:100000,total_duration_ms:0,total_size_bytes:0,online_volumes:1,offline_volumes:1,duplicates_count:1,issues_count:3,total_ebooks:0};
    if (cmd === 'get_library_books') return window.__fixtureBooks.slice(args.offset || 0,(args.offset || 0)+(args.limit || 250));
    if (cmd === 'get_volumes') return [{id:1,label:'Основной архив',mount_path:'G:',online:true,storage_class:'hdd',fs_type:'NTFS',capacity_bytes:1000000000000,free_bytes:240000000000,book_count:50000,roots_count:1},{id:2,label:'Отключённый архив',mount_path:'F:',online:false,storage_class:'hdd',fs_type:'NTFS',capacity_bytes:1000000000000,free_bytes:150000000000,book_count:50000,roots_count:1}];
    if (cmd === 'get_tasks') return [{id:10,kind:'scan',label:'Scan',description:'G: / Основной архив',progress:.48,state:'running'},{id:11,kind:'duplicates',label:'Поиск дубликатов',description:'Проверка коллекции',progress:0,state:'queued'}];
    if (cmd === 'get_optimizations') return [];
    if (cmd.startsWith('plugin:event|')) return 1;
    throw new Error('Unsupported fixture read: ' + cmd);
  }
};
"""
fixture_bootstrap += '''(() => {
  window.__fixtureOriginal = window.__TAURI_INTERNALS__.invoke;
  window.__fixtureCalls = [];
  window.__fixtureBooks = Array.from({length:100000}, (_,i) => ({id:i+900000,title:`Книга ${i} — A very long title / Очень длинное название, часть первой истории`,authors:i % 3 ? 'Автор не указан' : 'Александр Сергеевич Пушкин',narrators:i % 2 ? '' : 'Константин Константинович',series:i % 4 ? undefined : 'Очень длинная серия',series_index:i,duration_ms:i % 2 ? 0 : 42000000,size:840000000,format:'MP3',copies_count:1,online:i % 2 === 0,volume_name:i % 2 ? 'Отключённый архив' : 'Основной архив'}));
  window.__fixtureDownloads = [{id:301,name:'Очень длинное название аудиокниги / A long torrent release title with chapters',state:'downloading',progress:48,total_bytes:900000000,downloaded_bytes:432000000,down_speed:'2.4 MB/s',up_speed:'64 KB/s',eta_str:'3 мин',seeds:18,peers:4},{id:302,name:'Вторая запись — приостановленная загрузка',state:'paused',progress:8,total_bytes:780000000,downloaded_bytes:62400000,down_speed:'0 B/s',up_speed:'0 B/s',eta_str:'',seeds:0,peers:0}];
  window.__fixtureIssues = Array.from({length:3},(_,i)=>({id:600+i,book_id:900000+i,kind:'unknown_author',kind_label:'Unknown author',book_title:`Книга ${i}`,book_authors:'',message:'No author found'}));
  window.__fixturePairs = [{id:700,title:'Одна книга — две записи',authors:'Автор',score:.95,verdict:'Same recording',evidence:'acoustic fingerprints match',a:{copy_id:1,volume_name:'Архив A',narrator:'Чтец',format:'MP3',size:850000000,duration_ms:42000000,online:true,rel_path:'audio/book-a'},b:{copy_id:2,volume_name:'Архив B',narrator:'Чтец',format:'M4B',size:420000000,duration_ms:42000000,online:false,rel_path:'audio/book-b'}}];
  window.__TAURI_INTERNALS__.invoke = async (cmd,args={}) => {
    const writes = ['scan_all','scan_volume','transcode_copy','start_transcode','cancel_task','open_file','reveal_folder','save_settings','save_credentials','add_storage_folder','download_torrent','pause_download','resume_download','delete_download','ignore_issue','resolve_duplicate'];
    if (writes.includes(cmd)) window.__fixtureCalls.push({cmd,args});
    if (cmd === 'get_library_books') {
      if (args.search === 'stress') return window.__fixtureBooks;
      if (args.search === 'long') return window.__fixtureBooks.slice(0,6);
      if (args.search === 'loading') return new Promise(resolve => { window.__fixtureRelease = resolve; });
      if (args.search === 'error') throw new Error('Simulated internal Rust exception details');
      if (args.search === 'empty') return [];
    }
    if (cmd === 'get_downloads') return [...window.__fixtureDownloads];
    if (cmd === 'pause_download' || cmd === 'resume_download') { window.__fixtureDownloads.find(d=>d.id===args.id).state = cmd === 'pause_download' ? 'paused' : 'downloading'; return true; }
    if (cmd === 'delete_download') { window.__fixtureDownloads = window.__fixtureDownloads.filter(d=>d.id!==args.id); return true; }
    if (cmd === 'get_issues') return window.__fixtureIssues.filter(i => !args.kind || i.kind === args.kind).slice(args.offset || 0, (args.offset || 0) + (args.limit || 100));
    if (cmd === 'ignore_issue') { window.__fixtureIssues = window.__fixtureIssues.filter(d=>d.id!==args.issueId); return true; }
    if (cmd === 'get_duplicates') return [...window.__fixturePairs];
    if (cmd === 'resolve_duplicate') { window.__fixturePairs = []; return true; }
    if (cmd === 'pick_folder') throw new Error('Native dialog unavailable');
    if (cmd === 'search_torrents') throw new Error('Cloudflare Turnstile bb_data');
    if (writes.includes(cmd)) return true;
    return window.__fixtureOriginal(cmd,args);
  };
})()'''
app.call('Page.addScriptToEvaluateOnNewDocument', dict(source=fixture_bootstrap))
app.call('Page.navigate', dict(url='http://127.0.0.1:5173'))
wait('document.querySelector(".book-row")')
try:
    value('[data-search="global"]', 'stress')
    wait(r'document.querySelector(".collection-footer").innerText.replace(/\D/g, "").includes("100000")')
    check('100000-book DOM remains bounded', 'document.querySelectorAll(".book-row").length < 35')
    app.evaluate('document.querySelector(".virtual-list").scrollTop = 100000000')
    time.sleep(.2)
    check('100000-book last rows render', 'document.body.innerText.includes("99999")')
    shot('library-100000')
    value('[data-search="global"]', 'long')
    wait('document.querySelectorAll(".book-row").length === 6')
    check('Long titles fit row columns', '[...document.querySelectorAll(".book-row")].every(e => e.scrollWidth <= e.clientWidth + 2)')
    check('Unknown duration displays dash', 'document.body.innerText.includes("—")')
    shot('library-long-metadata')
    value('[data-search="global"]', 'loading')
    wait('window.__fixtureRelease')
    check('Loading retains existing collection', 'document.querySelectorAll(".book-row").length === 6')
    shot('library-background-loading')
    app.evaluate('window.__fixtureRelease(window.__fixtureBooks.slice(0,6)); window.__fixtureRelease = null')
    value('[data-search="global"]', 'empty')
    wait('document.querySelector(".empty-state")')
    shot('library-empty')
    value('[data-search="global"]', 'loading')
    wait('window.__fixtureRelease && document.querySelector(".loading-rows")')
    shot('library-skeleton')
    app.evaluate('window.__fixtureRelease([]); window.__fixtureRelease = null')
    value('[data-search="global"]', 'error')
    wait('document.querySelector(".notice.error")')
    check('IPC error is explained without raw exception', '!document.body.innerText.includes("Simulated internal") && document.body.innerText.includes("Не удалось загрузить каталог")')
    shot('library-error')
    value('[data-search="global"]', '')
    app.evaluate('document.querySelector(".notice.error button")?.click()')
    nav('Загрузки')
    wait('document.querySelectorAll(".activity-row").length === 2')
    shot('downloads-populated')
    click('button[aria-label="Приостановить загрузку"]')
    check('Pause calls existing IPC', 'window.__fixtureCalls.some(x=>x.cmd==="pause_download" && x.args.id===301)')
    click('button[aria-label="Продолжить загрузку"]')
    check('Resume calls existing IPC', 'window.__fixtureCalls.some(x=>x.cmd==="resume_download" && x.args.id===301)')
    click('.activity-row .storage-actions button:last-child')
    check('Download removal opens confirmation', '!!document.querySelector("[role=dialog]")')
    app.call('Input.dispatchKeyEvent',dict(type='keyDown',key='Tab',modifiers=8))
    app.call('Input.dispatchKeyEvent',dict(type='keyUp',key='Tab',modifiers=8))
    check('Dialog traps keyboard focus', '!!document.activeElement.closest(".modal")')
    shot('download-confirmation')
    click('.modal .btn-secondary')
    check('Cancel does not remove download', '!window.__fixtureCalls.some(x=>x.cmd==="delete_download")')
    click('.activity-row .storage-actions button:last-child')
    click('.modal .btn-danger')
    check('Confirmed removal uses existing IPC', 'window.__fixtureCalls.some(x=>x.cmd==="delete_download" && x.args.id===301)')
    nav('Проблемы')
    wait('document.querySelectorAll(".issue-row").length === 3')
    click('input[aria-label="Выбрать все замечания"]')
    click('.toolbar-right .btn-secondary')
    wait('window.__fixtureCalls.filter(x=>x.cmd==="ignore_issue").length === 3')
    check('Bulk dismissal retains original command', 'window.__fixtureCalls.filter(x=>x.cmd==="ignore_issue").length===3')
    nav('Дубликаты')
    wait('document.querySelector(".duplicate-review")?.innerText.includes("Одна книга — две записи")')
    click('.review-actions .btn-secondary')
    check('Duplicate keep-both decision preserves command', 'window.__fixtureCalls.some(x=>x.cmd==="resolve_duplicate" && x.args.status==="keep_both")')
    nav('Хранилища')
    click('.page-intro .storage-actions .btn-secondary')
    wait('document.querySelector(".modal input")')
    check('Manual folder fallback focuses path', 'document.activeElement === document.querySelector(".modal input")')
    shot('add-folder')
    app.call('Input.dispatchKeyEvent',dict(type='keyDown',key='Escape'))
    app.call('Input.dispatchKeyEvent',dict(type='keyUp',key='Escape'))
    check('Escape closes folder dialog', '!document.querySelector(".modal")')
    nav('RuTracker')
    value('[data-search="local"]', 'Кинг')
    click('.toolbar .btn-primary')
    wait('document.querySelector(".notice.error")')
    check('RuTracker protection yields useful recovery action', 'document.body.innerText.includes("cookie") && document.querySelector(".notice.error button").textContent.includes("Настройки")')
    shot('rutracker-error')
    check('Fixture mutations did not invoke real backend', 'window.__fixtureCalls.every(x=>["pause_download","resume_download","delete_download","ignore_issue","resolve_duplicate"].includes(x.cmd))')
finally:
    app.evaluate('window.__TAURI_INTERNALS__.invoke = window.__fixtureOriginal')

(output / 'state-validation.json').write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding='utf-8')
print(f'{len(results)} state checks passed', flush=True)
