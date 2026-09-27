use std::io::Write;
use std::net::TcpStream;

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn profile_nonce_script() -> &'static str {
    r#"async function profileNonce(database,key,value){
 const request=indexedDB.open(database,1);
 const db=await new Promise((resolve,reject)=>{request.onupgradeneeded=()=>request.result.createObjectStore("state");request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);});
 if(value)await new Promise((resolve,reject)=>{const tx=db.transaction("state","readwrite");tx.objectStore("state").put(value,key);tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);});
 const current=await new Promise((resolve,reject)=>{const tx=db.transaction("state","readonly");const read=tx.objectStore("state").get(key);read.onsuccess=()=>resolve(read.result||"");read.onerror=()=>reject(read.error);});
 db.close();return current;
}"#
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn query_origin_html(phase: &str, seed: &str, nonce_script: &str) -> String {
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>KEL-135 media query {phase}</title>
<script>
const phase={phase:?}, value={seed:?}, key="keld-kel135-query-nonce";
{nonce_script}
async function state(name){{try{{return (await navigator.permissions.query({{name}})).state;}}catch(error){{return "error-"+error.name;}}}}
async function run(){{const local=await profileNonce("keld-kel135-query",key,value),camera=await state("camera"),microphone=await state("microphone");await fetch("/report?"+new URLSearchParams({{phase,local,media:"not-requested",camera,microphone}}));}}
run().catch(error=>fetch("/report?"+new URLSearchParams({{phase,local:"",media:"error-"+error.name,camera:"unavailable",microphone:"unavailable"}})));
</script>"#
    )
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn profile_origin_html(phase: &str, seed: Option<&str>) -> String {
    let seed = seed.unwrap_or_default();
    let nonce_script = profile_nonce_script();
    if phase.starts_with("query-") {
        return query_origin_html(phase, seed, nonce_script);
    }
    if phase.starts_with("media-nonce-") {
        return format!(
            r#"<!doctype html><meta charset="utf-8"><title>KEL-135 media nonce {phase}</title>
<script>
const phase={phase:?}, key="keld-kel135-media-nonce";
{nonce_script}
async function run(){{
 const channel=new BroadcastChannel("keld-kel135-media-nonce");
 const committed=new Promise(resolve=>{{channel.onmessage=resolve;}});
 let local=await profileNonce("keld-kel135-media",key,"");
 if(!local&&phase==="media-nonce-reuse"){{await committed;local=await profileNonce("keld-kel135-media",key,"");}}
 channel.close();
 await fetch("/report?"+new URLSearchParams({{phase,local,media:"not-requested"}}));
}}
run().catch(error=>fetch("/report?"+new URLSearchParams({{phase,local:"",media:"error-"+error.name}})));
</script>"#
        );
    }
    if let Some(media_phase) = phase.strip_prefix("media-") {
        let Some((mode, kind)) = media_phase.split_once('-') else {
            panic!("invalid media fixture phase");
        };
        let constraint = match kind {
            "camera" => "{video:true}",
            "microphone" => "{audio:true}",
            _ => panic!("invalid media fixture kind"),
        };
        return format!(
            r#"<!doctype html><meta charset="utf-8"><title>KEL-135 media {phase}</title>
<script>
const phase={phase:?}, mode={mode:?}, kind={kind:?}, value={seed:?}, key="keld-kel135-media-nonce";
{nonce_script}
let nonce="";
fetch("/script-started?phase="+encodeURIComponent(phase)+"&media="+Boolean(navigator.mediaDevices&&navigator.mediaDevices.getUserMedia)).catch(()=>{{}});
async function permissionState(){{try{{if(!navigator.permissions||!navigator.permissions.query)return "unavailable";return (await navigator.permissions.query({{name:kind}})).state;}}catch(error){{return "error-"+error.name;}}}}
async function capture(){{
 const stream=await navigator.mediaDevices.getUserMedia({constraint});const tracks=stream.getTracks(),expected=kind==="camera"?"video":"audio";
 const live=tracks.length===1&&tracks[0].kind===expected&&tracks[0].readyState==="live";
 const deviceIdPresent=Boolean(tracks[0]&&tracks[0].getSettings().deviceId),label=tracks[0]?.label||"";
 let frameProgress="not-applicable";
 try{{
  if(live&&expected==="video"){{
   const video=document.createElement("video");video.muted=true;video.playsInline=true;video.style.display="none";video.srcObject=stream;document.documentElement.append(video);
   try{{
    await video.play();
    frameProgress=await new Promise(resolve=>{{
     if(typeof video.requestVideoFrameCallback!=="function"){{resolve("unsupported");return;}}
     const timeout=setTimeout(()=>resolve("timeout"),10000);let first=null;
     const onFrame=(_,metadata)=>{{if(first!==null&&metadata.presentedFrames>first){{clearTimeout(timeout);resolve("progressed");}}else{{first=metadata.presentedFrames;video.requestVideoFrameCallback(onFrame);}}}};
     video.requestVideoFrameCallback(onFrame);
    }});
   }}finally{{video.pause();video.srcObject=null;video.remove();}}
  }}
 }}finally{{for(const track of tracks)track.stop();}}
 return {{result:live?"resolved-"+expected+"-live":"invalid-track-state",deviceIdPresent,label,frameProgress}};
}}
const labelHex=label=>Array.from(new TextEncoder().encode(label),byte=>byte.toString(16).padStart(2,"0")).join("");
async function run(){{
 nonce=await profileNonce("keld-kel135-media",key,mode==="seed"?value:"");
 if(mode==="seed"){{const channel=new BroadcastChannel("keld-kel135-media-nonce");channel.postMessage("committed");channel.close();}}
 const permissionBefore=await permissionState();let permissionAfter="unavailable",media="",mediaRepeat="not-requested",deviceIdPresent="unavailable",deviceLabel="",repeatLabel="",frameProgress="unavailable",repeatFrameProgress="not-requested";
 try{{const first=await capture();media=first.result;deviceIdPresent=String(first.deviceIdPresent);deviceLabel=first.label;frameProgress=first.frameProgress;permissionAfter=await permissionState();if(mode==="seed"){{try{{const repeat=await capture();mediaRepeat=repeat.result;repeatLabel=repeat.label;repeatFrameProgress=repeat.frameProgress;}}catch(error){{mediaRepeat="error-"+error.name;}}}}}}
 catch(error){{media="error-"+error.name;permissionAfter=await permissionState();}}
 await fetch("/report?"+new URLSearchParams({{phase,local:nonce,media,media_repeat:mediaRepeat,device_id_present:deviceIdPresent,device_label_hex:labelHex(deviceLabel),repeat_label_hex:labelHex(repeatLabel),frame_progress:frameProgress,repeat_frame_progress:repeatFrameProgress,permission_before:permissionBefore,permission_after:permissionAfter}}));
}}
run().catch(error=>fetch("/report?"+new URLSearchParams({{phase,local:nonce,media:"error-"+error.name,media_repeat:"unavailable",permission_before:"unavailable",permission_after:"unavailable"}})));
</script>"#
        );
    }
    format!(
        r#"<!doctype html><meta charset="utf-8"><title>KEL-135 {phase}</title>
<script>
const phase={phase:?}, value={seed:?}, key="keld-kel135-profile-state";
const report=(local,cookie,idb,cache,sw)=>fetch("/report?"+new URLSearchParams({{phase,local,cookie,idb,cache,sw}}));
const cookie=()=>{{const item=document.cookie.split("; ").find(part=>part.startsWith("keld_kel135="));return item?item.slice("keld_kel135=".length):"";}};
function openDb(){{return new Promise((resolve,reject)=>{{const request=indexedDB.open("keld-kel135-profile",1);request.onupgradeneeded=()=>request.result.createObjectStore("state");request.onsuccess=()=>resolve(request.result);request.onerror=()=>reject(request.error);}});}}
async function readCache(){{try{{const cache=await caches.open("keld-kel135-profile");const response=await cache.match("/keld-cache-state");return response?await response.text():"";}}catch{{return "";}}}}
async function run(){{
 if(phase==="seed"){{
  localStorage.setItem(key,value);document.cookie="keld_kel135="+value+"; path=/; max-age=3600; SameSite=Lax";
  const db=await openDb();await new Promise((resolve,reject)=>{{const tx=db.transaction("state","readwrite");tx.objectStore("state").put(value,"value");tx.oncomplete=resolve;tx.onerror=()=>reject(tx.error);}});db.close();
  const cache=await caches.open("keld-kel135-profile");await cache.put("/keld-cache-state",new Response(value));
  const registration=await navigator.serviceWorker.register("/sw.js");await navigator.serviceWorker.ready;
  report(localStorage.getItem(key)||"",cookie(),value,await readCache(),registration.active?"true":"false");
 }} else {{
  let idb="";try{{const db=await openDb();idb=await new Promise((resolve,reject)=>{{const tx=db.transaction("state","readonly");const request=tx.objectStore("state").get("value");request.onsuccess=()=>resolve(request.result||"");request.onerror=()=>reject(request.error);}});db.close();}}catch{{}}
  const registrations=await navigator.serviceWorker.getRegistrations();const sw=registrations.some(registration=>Boolean(registration.active));
  report(localStorage.getItem(key)||"",cookie(),idb,await readCache(),sw?"true":"false");
 }}
}}
run().catch(()=>report("ERROR","ERROR","ERROR","ERROR","ERROR"));
</script>"#
    )
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn write_profile_http(stream: &mut TcpStream, status: u16, body: &str) {
    write_profile_http_type(stream, status, "text/html; charset=utf-8", body);
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn write_profile_http_type(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) {
    let reason = if status == 200 {
        "OK"
    } else if status == 204 {
        "No Content"
    } else {
        "Bad Request"
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .expect("write KEL-135 origin response");
}

#[cfg(all(target_os = "macos", feature = "profile-test-hooks"))]
pub(crate) fn service_worker_script() -> &'static str {
    "self.addEventListener('install', event => event.waitUntil(self.skipWaiting())); self.addEventListener('activate', event => event.waitUntil(self.clients.claim()));"
}
