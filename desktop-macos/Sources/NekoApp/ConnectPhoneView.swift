import AppKit
import WebKit

/// The "Connect a phone" guide: a plain HTML page (`Resources/Guide/index.html`) with animated steps.
/// The page is self-contained and also opens in a browser; the app only feeds it this computer's
/// address and settings, so the values it highlights are the real ones.
@MainActor
final class ConnectPhoneView: NSView, WKNavigationDelegate, WKScriptMessageHandler {
    private let model: FlowModel
    private var web: WKWebView!
    private var loaded = false
    private var lastJSON = ""

    init(model: FlowModel) {
        self.model = model
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        let cfg = WKWebViewConfiguration()
        cfg.userContentController.add(self, name: "neko")
        // Values exist before the page's own script runs, so the first frame already shows the right address.
        cfg.userContentController.addUserScript(WKUserScript(
            source: "window.NEKO = \(ConnectPhoneView.json(Self.values(model)));",
            injectionTime: .atDocumentStart, forMainFrameOnly: true))
        web = WKWebView(frame: .zero, configuration: cfg)
        web.navigationDelegate = self
        web.translatesAutoresizingMaskIntoConstraints = false
        web.setValue(false, forKey: "drawsBackground")
        addSubview(web)
        web.pin(to: self)
        if let url = ConnectPhoneView.guideURL {
            web.loadFileURL(url, allowingReadAccessTo: url.deletingLastPathComponent())
        }
    }
    required init?(coder: NSCoder) { fatalError() }

    /// The packaged app keeps the guide next to the fonts; `swift run` finds it in the resource bundle.
    static var guideURL: URL? {
        let dir = Bundle.main.url(forResource: "Guide", withExtension: nil) ?? Bundle.module.url(forResource: "Guide", withExtension: nil)
        return dir?.appendingPathComponent("index.html")
    }

    /// Pushes the current address and settings into the page without reloading it, so a playing step keeps going.
    func update() {
        let json = ConnectPhoneView.json(Self.values(model))
        guard loaded, json != lastJSON else { return }
        lastJSON = json
        web.evaluateJavaScript("window.nekoUpdate && window.nekoUpdate(\(json))", completionHandler: nil)
    }

    /// For the screenshot harness: web content is not captured by `cacheDisplay`.
    func snapshot(to path: String) {
        var done = false
        web.takeSnapshot(with: nil) { image, _ in
            if let tiff = image?.tiffRepresentation, let png = NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:]) {
                try? png.write(to: URL(fileURLWithPath: path))
            }
            done = true
        }
        let end = Date().addingTimeInterval(5)
        while !done && Date() < end { RunLoop.main.run(until: Date().addingTimeInterval(0.05)) }
    }

    var isLoaded: Bool { loaded }

    static func values(_ m: FlowModel) -> [String: Any] {
        let ips = m.admin.sdk?.lanAddresses ?? []
        // The Wi-Fi address is usually a private one; VPNs and containers add others.
        var ip = ""
        for prefix in ["192.168.", "10.", "172."] where ip.isEmpty {
            ip = ips.first(where: { $0.hasPrefix(prefix) }) ?? ""
        }
        if ip.isEmpty { ip = ips.first ?? "" }
        let port = m.listen.split(separator: ":").last.map(String.init) ?? "8899"
        var v: [String: Any] = ["ip": ip, "port": port.isEmpty ? "8899" : port, "listening": !m.listen.isEmpty]
        if let sdk = m.admin.sdk { v["lan"] = sdk.lan; v["sdkPort"] = String(sdk.port) }
        return v
    }

    private static func json(_ v: [String: Any]) -> String {
        guard let d = try? JSONSerialization.data(withJSONObject: v, options: [.sortedKeys]), let s = String(data: d, encoding: .utf8) else { return "{}" }
        return s
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        loaded = true
        lastJSON = ConnectPhoneView.json(Self.values(model))
    }

    // Only the bundled page loads here; anything else opens in the browser.
    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction,
                 decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = action.request.url, !url.isFileURL else { decisionHandler(.allow); return }
        if action.navigationType == .linkActivated { NSWorkspace.shared.open(url) }
        decisionHandler(.cancel)
    }

    func userContentController(_ c: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let body = message.body as? [String: Any], let text = body["copy"] as? String else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}
