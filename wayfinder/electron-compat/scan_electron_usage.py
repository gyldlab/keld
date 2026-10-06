import json, os, re, sys, collections
root=sys.argv[1]; name=sys.argv[2]
api=json.load(open('electron-api.json'))
entities={e['name']:e for e in api}
module_names={e['name'] for e in api if e['type'] in ('Module','Class')}
# collect member names per entity
members=collections.defaultdict(set); events=collections.defaultdict(set)
for e in api:
    for key in ('methods','instanceMethods','staticMethods'):
        for m in e.get(key) or []: members[e['name']].add(m['name'])
    for key in ('events','instanceEvents'):
        for m in e.get(key) or []: events[e['name']].add(m['name'])
files=[]
for dp,dn,fn in os.walk(root):
    dn[:]=[d for d in dn if d not in ('node_modules','.git','dist','out','build','.webpack','resources') ]
    for f in fn:
        if f.endswith(('.js','.ts','.mjs','.cjs','.vue','.tsx','.jsx')) and not f.endswith('.d.ts'):
            files.append(os.path.join(dp,f))
imp_re=re.compile(r"""(?:import\s*(?:type\s*)?\{([^}]*)\}\s*from\s*['"]electron(?:/(?:main|renderer|common))?['"]|const\s*\{([^}]*)\}\s*=\s*require\(\s*['"]electron['"]\s*\)|import\s+\*\s+as\s+(\w+)\s+from\s*['"]electron['"]|(?:const|let|var)\s+(\w+)\s*=\s*require\(\s*['"]electron['"]\s*\)|import\s+(\w+)\s+from\s*['"]electron['"])""")
mod_counts=collections.Counter(); mod_files=collections.defaultdict(set)
member_counts=collections.Counter(); member_files=collections.defaultdict(set)
event_counts=collections.Counter(); event_files=collections.defaultdict(set)
ns_aliases=set()
texts={}
for f in files:
    try: t=open(f,encoding='utf-8',errors='ignore').read()
    except: continue
    texts[f]=t
    for m in imp_re.finditer(t):
        names=(m.group(1) or m.group(2) or '')
        for n in re.split(r'[,\n]',names):
            n=n.strip().split(' as ')[0].strip()
            if n and n in module_names:
                mod_counts[n]+=1; mod_files[n].add(os.path.relpath(f,root))
        for g in (m.group(3),m.group(4),m.group(5)):
            if g: ns_aliases.add(g)
# namespace usage electron.X
for f,t in texts.items():
    for alias in ns_aliases|{'electron'}:
        for m in re.finditer(r'\b'+re.escape(alias)+r'\.(\w+)\b',t):
            n=m.group(1)
            if n in module_names:
                mod_counts[n]+=1; mod_files[n].add(os.path.relpath(f,root))
# member usage: for each imported/used module+class, count .member( and 'event'
used=set(mod_counts)|{'BrowserWindow','WebContents','Session','Menu','MenuItem','Tray','Notification','DownloadItem','WebFrameMain','NativeImage','Cookies','WebRequest','MessagePortMain','UtilityProcess','ClientRequest','BaseWindow'}
for f,t in texts.items():
    rel=os.path.relpath(f,root)
    for ent in used:
        for mem in members.get(ent,()):
            if len(mem)<4: continue
            c=len(re.findall(r'\.'+re.escape(mem)+r'\s*\(',t))
            if c: member_counts[(ent,mem)]+=c; member_files[(ent,mem)].add(rel)
        for ev in events.get(ent,()):
            if len(ev)<4: continue
            c=len(re.findall(r"""['"]"""+re.escape(ev)+r"""['"]""",t))
            if c: event_counts[(ent,ev)]+=c; event_files[(ent,ev)].add(rel)
# config signals
pkg=json.load(open(os.path.join(root,'package.json'))) if os.path.exists(os.path.join(root,'package.json')) else {}
deps={**pkg.get('dependencies',{}),**pkg.get('devDependencies',{}),**pkg.get('optionalDependencies',{})}
native_suspects=[d for d in deps if any(k in d for k in ('sqlite','pty','keytar','sharp','native','node-gyp','serialport','usb','ffi','fsevents','chokidar','nodegit','canvas','bcrypt','argon','leveldown','zeromq','electron-rebuild','@parcel/watcher','nsfw','registry','winreg'))]
signals={}
alltext='\n'.join(texts.values())
for key,pat in {
 'contextIsolation_false': r'contextIsolation\s*:\s*false','nodeIntegration_true': r'nodeIntegration\s*:\s*true','sandbox_false': r'sandbox\s*:\s*false','preload': r'preload\s*:','sendSync': r'\.sendSync\(','invoke': r'\.invoke\(','handle': r'ipcMain\.handle\(','registerSchemesAsPrivileged': r'registerSchemesAsPrivileged','protocol_handle': r'protocol\.(handle|registerFileProtocol|registerStringProtocol|registerBufferProtocol|registerStreamProtocol|interceptFileProtocol)','autoUpdater': r'autoUpdater','electron-updater': r'electron-updater','electron-store': r'electron-store','electron-builder_cfg': r'electron-builder','electron-forge': r'@electron-forge|electron-forge','setAsDefaultProtocolClient': r'setAsDefaultProtocolClient','requestSingleInstanceLock': r'requestSingleInstanceLock','second-instance': r'second-instance','webview_tag': r'<webview|webviewTag','BrowserView': r'BrowserView','WebContentsView': r'WebContentsView','setWindowOpenHandler': r'setWindowOpenHandler','contextBridge.exposeInMainWorld': r'exposeInMainWorld','session.defaultSession': r'session\.(defaultSession|fromPartition)','webContents.send': r'webContents\.send\(','setPermissionRequestHandler': r'setPermissionRequestHandler','globalShortcut': r'globalShortcut\.','safeStorage': r'safeStorage\.','powerMonitor': r'powerMonitor','crashReporter': r'crashReporter','utilityProcess': r'utilityProcess','MessageChannelMain': r'MessageChannelMain','desktopCapturer': r'desktopCapturer','nativeTheme': r'nativeTheme','systemPreferences': r'systemPreferences','TouchBar': r'TouchBar','app.dock': r'app\.dock','setProgressBar': r'setProgressBar','setLoginItemSettings': r'setLoginItemSettings','printToPDF': r'printToPDF','webContents.print': r'webContents\.print\(','executeJavaScript': r'executeJavaScript','openDevTools': r'openDevTools','app.getPath': r'app\.getPath\(','shell.openExternal': r'shell\.openExternal','shell.showItemInFolder': r'showItemInFolder','shell.trashItem': r'trashItem','dialog.showOpenDialog': r'showOpenDialog','dialog.showSaveDialog': r'showSaveDialog','dialog.showMessageBox': r'showMessageBox','Menu.buildFromTemplate': r'buildFromTemplate','Menu.setApplicationMenu': r'setApplicationMenu','popup': r'\.popup\(','Tray': r'new Tray\(','Notification': r'new Notification\(','clipboard': r'clipboard\.','screen': r'screen\.(getPrimaryDisplay|getAllDisplays|getCursorScreenPoint|getDisplayNearestPoint|getDisplayMatching)','nativeImage': r'nativeImage\.','process.versions.electron': r'process\.versions\.electron','process.type': r'process\.type','app.isPackaged': r'app\.isPackaged','app.setAboutPanelOptions': r'setAboutPanelOptions','app.on(activate)': r"on\(\s*['\"]activate['\"]",'before-quit': r"['\"]before-quit['\"]",'will-quit': r"['\"]will-quit['\"]",'window-all-closed': r"['\"]window-all-closed['\"]",'ready-to-show': r"['\"]ready-to-show['\"]",'did-finish-load': r"['\"]did-finish-load['\"]",'will-navigate': r"['\"]will-navigate['\"]",'new-window': r"['\"]new-window['\"]",'web-contents-created': r"['\"]web-contents-created['\"]",'certificate-error': r"['\"]certificate-error['\"]",'loadFile': r'\.loadFile\(','loadURL': r'\.loadURL\(','file://': r'file://','custom scheme app://': r"['\"](app|safe-file|zettlr|drawio)://",
}.items():
    c=len(re.findall(pat,alltext)); 
    if c: signals[key]=c
res={'app':name,'root':root,'files_scanned':len(files),'electron_dep':deps.get('electron'),'modules':{k:{'count':v,'files':sorted(mod_files[k])[:12],'file_count':len(mod_files[k])} for k,v in mod_counts.most_common()},'members_top':[{'entity':k[0],'member':k[1],'count':v,'file_count':len(member_files[k])} for k,v in member_counts.most_common(120)],'events_top':[{'entity':k[0],'event':k[1],'count':v,'file_count':len(event_files[k])} for k,v in event_counts.most_common(80)],'native_suspects':native_suspects,'deps_count':len(deps),'signals':signals}
json.dump(res,open(f'corpus/{name}.usage.json','w'),indent=1)
print(f"== {name} == files={len(files)} electron={deps.get('electron')} deps={len(deps)}")
print("MODULES:", ', '.join(f"{k}:{v['file_count']}f/{v['count']}" for k,v in res['modules'].items()))
print("NATIVE SUSPECTS:", native_suspects)
print("SIGNALS:", json.dumps(signals))
print("TOP MEMBERS:", ', '.join(f"{m['entity']}.{m['member']}:{m['count']}" for m in res['members_top'][:60]))
print("TOP EVENTS:", ', '.join(f"{m['entity']}:{m['event']}:{m['count']}" for m in res['events_top'][:40]))
