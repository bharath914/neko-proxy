import AppKit
import NekoAPI

/// `NEKO_SHOT_DIR=/dir proxy` renders every screen to PNG from fixtures, like
/// the Linux app's `cargo test`, so the two can be compared side by side.
enum Fixtures {
    indirect enum J {
        case obj([(String, J)]), arr([J]), str(String), num(String), lit(String)
    }

    static func jsonLines(_ v: J) -> [Neko_Api_V1_JsonLine] {
        var out: [Neko_Api_V1_JsonLine] = []
        func tok(_ k: Neko_Api_V1_JsonTokenKind, _ t: String) -> Neko_Api_V1_JsonToken { .with { $0.kind = k; $0.text = t } }
        func scalar(_ v: J) -> Neko_Api_V1_JsonToken {
            switch v {
            case .str(let s): return tok(.string, "\"\(s)\"")
            case .num(let n): return tok(.number, n)
            case .lit(let l): return tok(.literal, l)
            default: return tok(.punct, "")
            }
        }
        func emit(_ v: J, depth: Int, key: String?, comma: Bool) {
            var lead: [Neko_Api_V1_JsonToken] = []
            if let key { lead = [tok(.key, "\"\(key)\""), tok(.punct, ": ")] }
            switch v {
            case .obj(let pairs):
                let at = out.count
                out.append(.with { $0.depth = UInt32(depth); $0.tokens = lead + [tok(.punct, "{")]; $0.foldEnd = -1 })
                for (i, (k, x)) in pairs.enumerated() { emit(x, depth: depth + 1, key: k, comma: i < pairs.count - 1) }
                out[at].foldEnd = Int32(out.count)
                out.append(.with { $0.depth = UInt32(depth); $0.tokens = [tok(.punct, "}" + (comma ? "," : ""))]; $0.foldEnd = -1 })
            case .arr(let items):
                let at = out.count
                out.append(.with { $0.depth = UInt32(depth); $0.tokens = lead + [tok(.punct, "[")]; $0.foldEnd = -1 })
                for (i, x) in items.enumerated() { emit(x, depth: depth + 1, key: nil, comma: i < items.count - 1) }
                out[at].foldEnd = Int32(out.count)
                out.append(.with { $0.depth = UInt32(depth); $0.tokens = [tok(.punct, "]" + (comma ? "," : ""))]; $0.foldEnd = -1 })
            default:
                out.append(.with { $0.depth = UInt32(depth); $0.tokens = lead + [scalar(v)] + (comma ? [tok(.punct, ",")] : []); $0.foldEnd = -1 })
            }
        }
        emit(v, depth: 0, key: nil, comma: false)
        return out
    }

    static let itemsJSON: J = .obj([
        ("page", .num("2")), ("per_page", .num("20")), ("total", .num("184")), ("has_more", .lit("true")),
        ("items", .arr([
            .obj([("sku", .str("SKU-1042")), ("title", .str("Aurora USB-C Charger")), ("price", .num("2499")), ("tags", .arr([.str("power"), .str("usb-c")]))]),
            .obj([("sku", .str("SKU-2210")), ("title", .str("Field Notebook")), ("price", .num("899")), ("tags", .arr([]))]),
        ])),
    ])

    static func flows() -> [Flow] {
        let pattern: [(String, String, String, UInt32, UInt64, Double, Neko_Api_V1_FlowSource?, Int)] = [
            ("GET", "docs.example.com", "/guides/getting-started", 200, 7168, 427, .merged, 0),
            ("GET", "api.example.com", "/v1/items?page=2&per_page=20", 200, 18432, 250, .sdk, 0),
            ("GET", "cdn.example.com", "/img/banner-12.webp", 200, 94208, 82, nil, 0),
            ("POST", "payments.example.com", "/v1/charge", 500, 88, 1870, .merged, 0),
            ("POST", "analytics.example.net", "/collect", 204, 0, 42, nil, 0),
            ("GET", "api.example.com", "/v1/config", 304, 0, 58, nil, 0),
            ("PUT", "api.example.com", "/v1/cart/77", 200, 612, 146, nil, 0),
            ("DELETE", "api.example.com", "/v1/cart/77/coupon", 404, 74, 117, nil, 0),
            ("CONNECT", "pay.example.com", "tunnel", 0, 0, 0, nil, 3),
            ("PATCH", "api.example.com", "/v1/me/settings", 200, 240, 109, nil, 0),
            ("GET", "api.example.com", "/v1/me", 200, 1228, 106, nil, 0),
            ("POST", "auth.example.com", "/oauth/token", 401, 196, 149, nil, 0),
        ]
        return (1...60).map { i in
            let p = pattern[(i - 1) % pattern.count]
            return Flow.with {
                $0.id = UInt64(i)
                $0.startedAtMs = 1_760_000_000_000 + Int64(i) * 700
                $0.durationMs = p.5
                $0.scheme = "https"; $0.host = p.1; $0.port = 443
                $0.method = p.0; $0.path = p.2; $0.status = p.3
                $0.protocol = p.0 == "CONNECT" ? "tunnel" : "h2"
                $0.resSize = p.4; $0.state = p.3 == 0 ? .complete : .complete
                $0.flags = UInt32(p.7)
                $0.contentType = "json"
                if let s = p.6 { $0.source = s }
            }
        }
    }

    static func hosts() -> [Neko_Api_V1_HostSummary] {
        [("docs.example.com", 5, 0, false), ("api.example.com", 30, 3, false), ("pay.example.com", 5, 0, true),
         ("analytics.example.net", 5, 0, false), ("payments.example.com", 5, 5, false), ("cdn.example.com", 5, 0, false),
         ("auth.example.com", 5, 5, false)].map { h in
            Neko_Api_V1_HostSummary.with { $0.host = h.0; $0.flows = UInt64(h.1); $0.errors = UInt64(h.2); $0.pinned = h.3; $0.decrypted = !h.3 }
        }
    }

    static func detail(_ id: UInt64) -> FlowDetail {
        let f = flows()[Int(id) - 1]
        return FlowDetail.with {
            $0.summary = f
            $0.url = f.method == "CONNECT" ? "https://\(f.host)" : "https://\(f.host)\(f.path)"
            $0.curl = "curl -X \(f.method) 'https://\(f.host)\(f.path)' -H 'accept: application/json'"
            $0.requestHeaders = [.with { $0.name = "accept"; $0.value = "application/json" }, .with { $0.name = "user-agent"; $0.value = "ShopApp/2.3 (iOS)" }]
            $0.responseHeaders = (0..<5).map { i in .with { $0.name = ["content-type", "content-encoding", "cache-control", "etag", "server"][i]; $0.value = ["application/json", "gzip", "max-age=60", "\"a1b2c3\"", "nginx"][i] } }
            $0.timings = .with { $0.dnsMs = 12; $0.connectMs = 31; $0.tlsMs = 54; $0.sendMs = 1; $0.waitMs = 118; $0.receiveMs = 34; $0.totalMs = 250 }
            let lines = jsonLines(itemsJSON)
            $0.responseBody = .with {
                $0.kind = .json; $0.contentType = "application/json"; $0.contentEncoding = "gzip"
                $0.wireSize = 1200; $0.decodedSize = 3277; $0.json = lines
                $0.text = lines.map { String(repeating: "  ", count: Int($0.depth)) + $0.tokens.map(\.text).joined() }.joined(separator: "\n")
            }
        }
    }

    @MainActor static func run(into dir: String) {
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        Theme.registerFonts()
        let model = FlowModel()
        model.loadFixture(flows: flows(), hosts: hosts(),
                          devices: ["Pixel 8", "Pixel 7 API 35", "Asha's iPhone"].map { n in .with { $0.deviceID = n; $0.deviceName = n } },
                          pending: 1) { detail($0) }
        let wc = MainWindowController(model: model)
        let w = wc.window!
        w.setContentSize(NSSize(width: 1320, height: 820))
        w.orderBack(nil)

        func snap(_ name: String) {
            RunLoop.main.run(until: Date().addingTimeInterval(0.4))
            w.contentView?.layoutSubtreeIfNeeded()
            guard let v = w.contentView, let rep = v.bitmapImageRepForCachingDisplay(in: v.bounds) else { return }
            v.cacheDisplay(in: v.bounds, to: rep)
            if let png = rep.representation(using: .png, properties: [:]) {
                try? png.write(to: URL(fileURLWithPath: dir + "/" + name + ".png"))
            }
        }

        for (scheme, ap) in [("light", NSAppearance.Name.aqua), ("dark", .darkAqua)] {
            w.appearance = NSAppearance(named: ap)
            wc.root.setSection(.inspector)
            wc.root.list.reload()
            wc.root.sidebar.refresh()
            wc.root.list.select(id: 39)
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
            wc.root.detail.selectTab(2)
            snap("mac-inspector-\(scheme)")
            wc.root.list.select(id: 9)
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
            wc.root.detail.selectTab(0)
            snap("mac-pinned-\(scheme)")
            wc.root.detail.selectTab(3)
            wc.root.list.select(id: 39)
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
            wc.root.detail.selectTab(3)
            snap("mac-timing-\(scheme)")
            wc.root.setSection(.settings)
            snap("mac-settings-\(scheme)")
        }
    }
}
