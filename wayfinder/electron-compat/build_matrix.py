import json, csv, collections, glob, os
api=json.load(open('electron-api.json'))
fam_of={}
for f in glob.glob('families/*.json'):
    fam=os.path.basename(f)[:-5]
    for e in json.load(open(f)): fam_of[e['name']]=fam
# tiers per arch 04 §4 + KEL-10; never list
TIER={
 'app':'1','BrowserWindow':'1','BaseWindow':'1','ipcMain':'1','ipcRenderer':'1','contextBridge':'1','dialog':'1','shell':'1','Menu':'1','MenuItem':'1','Tray':'1','clipboard':'1','ClipboardItem':'1','Notification':'1','screen':'1','nativeImage':'1','NativeImage':'1','process':'1','WebContents':'1','IpcMainEvent':'1','IpcMainInvokeEvent':'1','IpcRendererEvent':'1','BrowserWindowConstructorOptions':'1','BaseWindowConstructorOptions':'1','WebPreferences':'1','Rectangle':'1','Point':'1','Size':'1','Display':'1','FileFilter':'1','NotificationAction':'1','NotificationResponse':'1','MenuItemBadge':'1','KeyboardEvent':'1','WindowOpenHandlerResponse':'1','WindowStatePersistence':'1','RenderProcessGoneDetails':'1','NavigationEntry':'1','NavigationHistory':'1','WebFrameMain':'1','webFrameMain':'1','webFrame':'1','webContents':'1','InputEvent':'1','KeyboardInputEvent':'1','MouseInputEvent':'1','MouseWheelInputEvent':'1','Referrer':'1','PostBody':'1','UploadData':'1','UploadFile':'1','UploadRawData':'1','PreloadScript':'1','PreloadScriptRegistration':'1','WebSource':'1','PrintToPDFOptions':'1','PrintToPDFMargins':'1','PrinterInfo':'1','ClipboardBookmark':'1','ShortcutDetails':'1','ActivationArguments':'1','CommandLine':'1','Dock':'1','webUtils':'1',
 'globalShortcut':'2','powerMonitor':'2','powerSaveBlocker':'2','safeStorage':'2','session':'2','Session':'2','Cookies':'2','Cookie':'2','WebRequest':'2','WebRequestFilter':'2','protocol':'2','CustomScheme':'2','ProtocolRequest':'2','ProtocolResponse':'2','ProtocolResponseUploadData':'2','FilePathWithHeaders':'2','MimeTypedBuffer':'2','utilityProcess':'2','UtilityProcess':'2','parentPort':'2','MessageChannelMain':'2','MessagePortMain':'2','autoUpdater':'2','crashReporter':'2','CrashReport':'2','nativeTheme':'2','systemPreferences':'2','UserDefaultTypes':'2','DownloadItem':'2','ProxyConfig':'2','ResolvedEndpoint':'2','ResolvedHost':'2','PermissionRequest':'2','FilesystemPermissionRequest':'2','MediaAccessPermissionRequest':'2','OpenExternalPermissionRequest':'2','Certificate':'2','CertificatePrincipal':'2','Debugger':'2','Task':'2','JumpListCategory':'2','JumpListItem':'2','ThumbarButton':'2','View':'2','WebContentsView':'2','TouchBar':'2','TouchBarButton':'2','TouchBarColorPicker':'2','TouchBarGroup':'2','TouchBarLabel':'2','TouchBarOtherItemsProxy':'2','TouchBarPopover':'2','TouchBarScrubber':'2','TouchBarSegmentedControl':'2','TouchBarSlider':'2','TouchBarSpacer':'2','ScrubberItem':'2','SegmentedControlSegment':'2','ShareMenu':'2','SharingItem':'2','pushNotifications':'2','WindowSessionEndEvent':'2','ColorSpace':'2','ProcessMetric':'2','ProcessMemoryInfo':'2','MemoryInfo':'2','CPUUsage':'2','MemoryUsageDetails':'2','GPUFeatureStatus':'2','EnableHeapProfilingOptions':'2','WebAuthnAccount':'2','ServiceWorkers':'2','ServiceWorkerMain':'2','ServiceWorkerInfo':'2','SharedWorkerInfo':'2','IpcMainServiceWorker':'2','IpcMainServiceWorkerEvent':'2','IpcMainServiceWorkerInvokeEvent':'2','SharedDictionaryInfo':'2','SharedDictionaryUsageInfo':'2','inAppPurchase':'2','Product':'2','ProductDiscount':'2','ProductSubscriptionPeriod':'2','PaymentDiscount':'2','Transaction':'2','ImageView':'2',
 'webviewTag':'3','BrowserView':'3','desktopCapturer':'3','DesktopCapturerSource':'3','net':'3','ClientRequest':'3','IncomingMessage':'3','WebSocket':'3','WebSocketOptions':'3','netLog':'3','contentTracing':'3','TraceCategoriesAndOptions':'3','TraceConfig':'3','Extensions':'3','Extension':'3','ExtensionInfo':'3','BluetoothDevice':'3','HIDDevice':'3','SerialPort':'3','USBDevice':'3',
 'sharedTexture':'never','OffscreenSharedTexture':'never','SharedTextureHandle':'never','SharedTextureImportTextureInfo':'never','SharedTextureImported':'never','SharedTextureImportedSubtle':'never','SharedTextureSubtle':'never','SharedTextureSyncToken':'never','SharedTextureTransfer':'never',
}
SUBSYS={'F01-app-lifecycle':'@keld/electron + @keld/api + keld-core lifecycle + keld-native(deeplink/autostart/dock)','F02-windowing':'@keld/electron + keld-core window registry + keld-wv','F03-webcontents-navigation':'@keld/electron + keld-wv + keld-compat (webContents routing)','F04-ipc-bridge':'@keld/electron (main+renderer shim) + kipc + keld-runtime roles/virtual ports','F05-session-network-protocol':'keld-compat host emulation + keld-wv + keld-guard net scopes','F06-native-desktop':'keld-native brokers + keld-guard system grants + @keld/api','F07-renderer-webview-tag':'@keld/web renderer shim + keld-wv (multi-webview)','F08-diagnostics-crash-tracing':'keld-runtime crash ledger + keld-core census','F09-update-packaging':'keld-update + keld-pack + keld-cli migrate'}
LINEAR={'F01-app-lifecycle':'KEL-72,KEL-142,KEL-139','F02-windowing':'KEL-139,KEL-142,KEL-143,KEL-135','F03-webcontents-navigation':'KEL-79,KEL-132,KEL-142','F04-ipc-bridge':'KEL-80,KEL-75,KEL-136,KEL-98,KEL-142','F05-session-network-protocol':'KEL-79,KEL-135,KEL-74','F06-native-desktop':'KEL-71,KEL-130,KEL-102,KEL-89,KEL-76','F07-renderer-webview-tag':'KEL-79,KEL-142,KEL-132','F08-diagnostics-crash-tracing':'KEL-105,KEL-116,KEL-90,KEL-129','F09-update-packaging':'KEL-53,KEL-254,KEL-270,KEL-103,KEL-141,KEL-19'}
# inherited members: compute for classes with extends
ents={e['name']:e for e in api}
def member_rows(e):
    for key,kind in [('methods','method'),('instanceMethods','instance-method'),('staticMethods','static-method'),('events','event'),('instanceEvents','instance-event'),('properties','property'),('instanceProperties','instance-property'),('staticProperties','static-property')]:
        for m in e.get(key) or []:
            yield kind,m
    if e.get('constructorMethod'): yield 'constructor',{'name':'constructor','parameters':e['constructorMethod'].get('parameters')}
parent_members={}
for e in api:
    if e.get('extends') and e['extends'] in ents:
        p=ents[e['extends']]
        parent_members[e['name']]={(k,m['name']) for k,m in member_rows(p)}
# demand
demand=collections.Counter()
for app in ('drawio-desktop','zettlr','electron-quick-start'):
    u=json.load(open(f'corpus/{app}.usage.json'))
    for m in u['members_top']: demand[(m['entity'],m['member'],app)]+=m['count']
    for m in u['events_top']: demand[(m['entity'],m['event'],app)]+=m['count']
    for mod,v in u['modules'].items(): demand[(mod,'*',app)]+=v['file_count']
# live Keld status (KEL-72 / scoreboard)
LIVE={('app','whenReady'):'BEHAVIOR_MATCH',('app','isReady'):'PARTIAL',('app','quit'):'BEHAVIOR_MATCH ▲ (Promise<void>)',('app','on'):'PARTIAL ▲ (listener isolation)',('app','off'):'PARTIAL',('app','removeListener'):'PARTIAL',('app','ready'):'BEHAVIOR_MATCH',('app','window-all-closed'):'BEHAVIOR_MATCH',('process','type'):'SCAFFOLDED',('process','versions'):'SCAFFOLDED ▲ ("0.0.1")'}
rows=[]
for e in api:
    fam=fam_of[e['name']]; tier=TIER.get(e['name'],'?')
    proc=e.get('process') or {}
    procs='/'.join(k for k in ('main','renderer','utility') if proc.get(k))
    for kind,m in member_rows(e):
        plat=','.join((p.get('name') if isinstance(p,dict) else str(p)) for p in (m.get('platforms') or []))
        tags=','.join(m.get('additionalTags') or [])
        inherited = e.get('extends') if (kind,m['name']) in parent_members.get(e['name'],set()) else ''
        d_dr=demand.get((e['name'],m['name'],'drawio-desktop'),0); d_z=demand.get((e['name'],m['name'],'zettlr'),0); d_q=demand.get((e['name'],m['name'],'electron-quick-start'),0)
        status=LIVE.get((e['name'],m['name']),'NEVER' if tier=='never' else 'UNSUPPORTED')
        rows.append({'entity':e['name'],'entity_type':e['type'],'member':m['name'],'kind':kind,'process':procs,'platforms':plat,'tags':tags,'inherited_from':inherited,'family':fam,'tier':tier,'keld_status':status,'keld_subsystem':SUBSYS[fam],'linear_owners':LINEAR[fam],'demand_drawio':d_dr,'demand_zettlr':d_z,'demand_quickstart':d_q,'github_ticket':'','electron_doc':(e.get('websiteUrl') or '')})
with open('compat-matrix.tsv','w',newline='') as f:
    w=csv.DictWriter(f,fieldnames=list(rows[0].keys()),delimiter='\t'); w.writeheader(); w.writerows(rows)
# summary
c=collections.Counter(r['tier'] for r in rows); inh=sum(1 for r in rows if r['inherited_from']); direct=len(rows)-inh
print('rows',len(rows),'inherited',inh,'direct',direct); print('by tier',dict(c))
fc=collections.Counter(r['family'] for r in rows if not r['inherited_from']); print('direct by family',dict(fc))
ent_tier=collections.Counter(TIER.get(e['name'],'?') for e in api); print('entities by tier',dict(ent_tier))
print('untiered entities',[e['name'] for e in api if e['name'] not in TIER])
