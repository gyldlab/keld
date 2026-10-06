import Cocoa
import WebKit

final class Handler: NSObject, WKURLSchemeHandler {
    var seen: [String] = []
    func webView(_ webView: WKWebView, start task: WKURLSchemeTask) {
        let url = task.request.url!.absoluteString
        seen.append(url)
        FileHandle.standardError.write("REQ \(url)\n".data(using: .utf8)!)
        var body = Data(); var mime = "text/plain"
        if url.hasSuffix("page.html") {
            mime = "text/html"
            body = "<html><body><img src='rel.png'><script>fetch('rel.txt').then(r=>r.text()).then(t=>{window.webkit.messageHandlers.k.postMessage('fetch-ok:'+t)}).catch(e=>window.webkit.messageHandlers.k.postMessage('fetch-err:'+e)); try{localStorage.setItem('a','b'); window.webkit.messageHandlers.k.postMessage('ls-ok')}catch(e){window.webkit.messageHandlers.k.postMessage('ls-err:'+e)}; window.webkit.messageHandlers.k.postMessage('origin:'+location.origin+' href:'+location.href); window.webkit.messageHandlers.k.postMessage('Notification:'+typeof Notification+' perm:'+(typeof Notification!=='undefined'?Notification.permission:'n/a')); navigator.clipboard.writeText('x').then(()=>window.webkit.messageHandlers.k.postMessage('clip-write-ok')).catch(e=>window.webkit.messageHandlers.k.postMessage('clip-write-err:'+e.name)); navigator.permissions.query({name:'geolocation'}).then(r=>window.webkit.messageHandlers.k.postMessage('geo-perm:'+r.state)).catch(e=>window.webkit.messageHandlers.k.postMessage('geo-perm-err:'+e.name)); navigator.geolocation.getCurrentPosition(p=>window.webkit.messageHandlers.k.postMessage('geo-ok'),e=>window.webkit.messageHandlers.k.postMessage('geo-err:'+e.code))</script></body></html>".data(using: .utf8)!
        } else if url.hasSuffix("rel.txt") { body = "TXT".data(using: .utf8)! }
        let resp = HTTPURLResponse(url: task.request.url!, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": mime, "Content-Length": String(body.count)])!
        task.didReceive(resp); task.didReceive(body); task.didFinish()
    }
    func webView(_ webView: WKWebView, stop task: WKURLSchemeTask) {}
}
final class Msg: NSObject, WKScriptMessageHandler {
    func userContentController(_ u: WKUserContentController, didReceive m: WKScriptMessage) {
        FileHandle.standardError.write("MSG \(m.body)\n".data(using: .utf8)!)
    }
}
let app = NSApplication.shared
let cfg = WKWebViewConfiguration()
let h = Handler(); let m = Msg()
cfg.setURLSchemeHandler(h, forURLScheme: "safe-file")
cfg.userContentController.add(m, name: "k")
let wv = WKWebView(frame: NSRect(x: 0, y: 0, width: 200, height: 200), configuration: cfg)
print("handlesURLScheme(safe-file) =", WKWebView.handlesURLScheme("safe-file"))
wv.load(URLRequest(url: URL(string: "safe-file:///dir/sub/page.html")!))
DispatchQueue.main.asyncAfter(deadline: .now() + 4) {
    print("SEEN:", h.seen)
    exit(0)
}
app.run()
