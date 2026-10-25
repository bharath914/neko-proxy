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

    static func rule(_ id: String, _ name: String, on: Bool, _ build: (inout Rule) -> Void) -> Rule {
        var r = Rule()
        r.id = id; r.name = name; r.enabled = on
        build(&r)
        return r
    }

    static func rulesFixture() -> Neko_Api_V1_RulesResponse {
        var out = Neko_Api_V1_RulesResponse()
        out.rules = [
            rule("r10", "Checkout is down", on: true) {
                $0.matcher = .with { $0.method = "POST"; $0.host = "api.example.com"; $0.path = "/v1/checkout" }
                $0.mock = .with { $0.status = 503; $0.delayMs = 300 }
            },
            rule("r11", "Hit staging", on: false) {
                $0.matcher = .with { $0.host = "api.example.com"; $0.path = "/*" }
                $0.mapRemote = .with { $0.scheme = "https"; $0.host = "staging.example.com" }
            },
            rule("r12", "Pause on login", on: true) {
                $0.matcher = .with { $0.host = "api.example.com"; $0.path = "/v1/login" }
                $0.breakpoint = .with { $0.phase = .request }
            },
        ]
        return out
    }

    static func pausedFixture() -> [PausedFlow] {
        var p = PausedFlow()
        p.flowID = 40; p.ruleID = "r12"; p.phase = .phaseRequest; p.method = "POST"; p.path = "/v1/login"; p.timeoutS = 60
        p.summary = .with { $0.scheme = "https"; $0.host = "api.example.com"; $0.path = "/v1/login"; $0.method = "POST" }
        p.headers = textToHeaders("content-type: application/json\naccept: application/json\nuser-agent: Mozilla/5.0")
        p.body = Data("{\"email\":\"ann@example.com\",\"password\":\"correct horse\"}".utf8)
        return [p]
    }

    static func approvalFixture() -> [Approval] {
        [Approval.with {
            $0.id = "a1"; $0.tool = "create_mock"; $0.title = "Mock 503 for POST api.example.com/v1/checkout"
            $0.detail = "{\n  \"host\": \"api.example.com\",\n  \"method\": \"POST\",\n  \"path\": \"/v1/checkout\",\n  \"status\": 503\n}"
        }]
    }

    static func devicesFixture() -> [Device] {
        func d(_ id: String, _ name: String, _ model: String, _ os: String, _ app: String, _ appID: String, _ ver: String, _ sdk: String,
               _ conn: String, _ st: Neko_Api_V1_DeviceStatus, _ seen: Int64, _ flows: UInt64) -> Device {
            Device.with {
                $0.deviceID = id; $0.deviceName = name; $0.deviceModel = model; $0.osVersion = os; $0.appName = app; $0.appID = appID
                $0.appVersion = ver; $0.sdkName = sdk; $0.sdkVersion = "0.1.0"; $0.connection = conn; $0.status = st
                $0.lastSeenMs = Int64(Date().timeIntervalSince1970 * 1000) - seen; $0.flows = flows
            }
        }
        return [
            d("p8", "Pixel 8", "Google Pixel 8", "Android 15", "Demo Shop", "com.demo.shop", "1.4.2", "android", "wifi", .deviceConnected, 0, 1284),
            d("p7", "Pixel 7 API 35", "sdk_gphone64_x86_64", "Android 15", "Demo Shop", "com.demo.shop", "1.4.3-debug", "android", "emulator", .deviceConnected, 0, 312),
            d("ip", "Asha's iPhone", "iPhone 15 Pro", "iOS 19.2", "Demo Shop", "com.demo.shop", "1.4.2", "ios", "wifi", .deviceIdle, 3_600_000, 96),
            d("s23", "Galaxy S23", "SM-S911B", "Android 14", "Partner Hub", "com.demo.partner", "0.9.0", "android", "usb", .deviceDenied, 259_200_000, 0),
        ]
    }

    @MainActor static func composerFixture(_ model: ComposerModel) {
        func kv(_ n: String, _ v: String, _ d: String = "", off: Bool = false) -> KeyValue {
            .with { $0.name = n; $0.value = v; $0.description_p = d; $0.disabled = off }
        }
        let draft = ComposedRequest.with {
            $0.id = "items"; $0.name = "Search catalog"; $0.collection = "Shop API"; $0.method = "POST"
            $0.url = "{{base_url}}/v1/items/search"
            $0.params = [kv("page", "2", "Page number"), kv("per_page", "20"), kv("debug", "1", off: true)]
            $0.headers = [kv("accept", "application/json"), kv("x-client", "Neko Composer", "Identifies this tool")]
            $0.bodyKind = .json; $0.body = "{\n  \"query\": \"charger\",\n  \"in_stock\": true\n}"
            $0.auth = .inherit
            $0.tests = [
                .with { $0.kind = .status; $0.op = .equals; $0.expected = "200" },
                .with { $0.kind = .json; $0.target = "data.items[0].id"; $0.op = .exists },
                .with { $0.kind = .time; $0.op = .lessThan; $0.expected = "200" },
            ]
        }
        var saved = draft; saved.body = "{}"
        let me = ComposedRequest.with { $0.id = "me"; $0.name = "Current user"; $0.collection = "Shop API"; $0.method = "GET"; $0.url = "{{base_url}}/v1/me"; $0.auth = .inherit }
        let orders = ComposedRequest.with { $0.id = "orders"; $0.name = "List orders"; $0.collection = "Shop API/Orders"; $0.method = "GET"; $0.url = "{{base_url}}/v1/orders/:status"; $0.auth = .inherit; $0.pathVars = [kv("status", "open")] }
        let create = ComposedRequest.with { $0.id = "create"; $0.name = "Create order"; $0.collection = "Shop API/Orders"; $0.method = "POST"; $0.url = "{{base_url}}/v1/orders"; $0.bodyKind = .formData; $0.form = [kv("sku", "USB-C-65W"), kv("qty", "2"), .with { $0.name = "receipt"; $0.value = "~/receipt.pdf"; $0.file = true }] }
        let refund = ComposedRequest.with { $0.id = "refund"; $0.name = "Refund"; $0.collection = "Shop API/Orders/Admin"; $0.method = "DELETE"; $0.url = "{{base_url}}/v1/orders/:id/refund" }
        let login = ComposedRequest.with { $0.id = "login"; $0.name = "Sign in"; $0.collection = "Authentication"; $0.method = "POST"; $0.url = "{{base_url}}/oauth/token"; $0.bodyKind = .form; $0.form = [kv("grant_type", "password")] }
        let state = ComposerState.with {
            $0.collections = ["Shop API", "Shop API/Orders", "Shop API/Orders/Admin", "Authentication"]
            $0.requests = [saved, me, orders, create, refund, login]
            $0.environments = [
                .with { $0.name = "Staging"; $0.vars = [kv("base_url", "https://api.example.com"), .with { $0.name = "token"; $0.value = "s3cr3t"; $0.secret = true }] },
                .with { $0.name = "Production"; $0.vars = [kv("base_url", "https://api.shop.example")] },
            ]
            $0.activeEnvironment = "Staging"
            $0.globals = [kv("locale", "en-US")]
            $0.configs = [.with { $0.name = "Shop API"; $0.auth = .with { $0.kind = .bearer; $0.token = "{{token}}" }; $0.headers = [kv("x-app", "neko")] }]
            $0.history = [
                .with { $0.request = draft; $0.flowID = 2; $0.method = "POST"; $0.url = "https://api.example.com/v1/items/search"; $0.status = 200; $0.durationMs = 250 },
                .with { $0.request = me; $0.flowID = 3; $0.method = "GET"; $0.url = "https://api.example.com/v1/me"; $0.status = 401; $0.durationMs = 80 },
            ]
        }
        model.fixtureResolved = .with {
            $0.method = "POST"; $0.url = "https://api.example.com/v1/items/search?page=2&per_page=20"; $0.curl = "curl https://api.example.com/v1/items/search"
            $0.headers = [.with { $0.name = "accept"; $0.value = "application/json" }, .with { $0.name = "Authorization"; $0.value = "Bearer s3cr3t" }, .with { $0.name = "Content-Type"; $0.value = "application/json" }]
            $0.generated = ["Authorization", "Content-Type"]
        }
        model.loadFixture(state, draft: draft, response: detail(2), sent: .with {
            $0.flowID = 2; $0.status = 200; $0.durationMs = 250; $0.size = 1840
            $0.tests = [
                .with { $0.name = "Status equals 200"; $0.passed = true },
                .with { $0.name = "JSON data.items[0].id exists"; $0.passed = true },
                .with { $0.name = "Response time less than 200 ms"; $0.passed = false; $0.message = "Got 250 ms" },
            ]
            $0.cookies = [.with { $0.name = "session"; $0.value = "9f2c1ab4"; $0.domain = "api.example.com"; $0.path = "/"; $0.secure = true; $0.httpOnly = true; $0.sameSite = "Lax" }]
        }, display: "{{base_url}}/v1/items/search?page=2&per_page=20")
        model.addFixtureTab(me)
        model.addFixtureTab(login)
    }

    @MainActor static func run(into dir: String) {
        try? FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        Theme.registerFonts()
        let model = FlowModel()
        model.loadFixture(flows: flows(), hosts: hosts(),
                          devices: ["Pixel 8", "Pixel 7 API 35", "Asha's iPhone"].map { n in .with { $0.deviceID = n; $0.deviceName = n } },
                          pending: 1) { detail($0) }
        model.admin.loadFixture(
            rules: rulesFixture(), paused: pausedFixture(), approvals: approvalFixture(),
            sdk: .with {
                $0.available = true; $0.enabled = true; $0.autoAcceptLocal = true; $0.lan = true
                $0.listening = ["192.168.0.109:8897"]; $0.fingerprint = "d7d65b46c5037a"
                $0.adbState = "ready"; $0.adbLabel = "adb ready, 1 device(s)"; $0.discoveryState = "advertising"
                $0.checks = [.with { $0.id = "listener"; $0.level = "ok"; $0.title = "Listening on 192.168.0.109:8897"; $0.detail = "Phones on this network can find this computer." }]
            },
            mcp: .with {
                $0.enabled = true; $0.url = "http://127.0.0.1:8898/mcp"; $0.token = "0123456789abcdef"
                $0.command = "claude mcp add --transport http neko http://127.0.0.1:8898/mcp --header \"Authorization: Bearer 0123456789abcdef\""
                $0.writes = .mcpAsk
            })
        model.setDevices(devicesFixture(), pending: [.with {
            $0.id = 1; $0.deviceName = "Galaxy Tab A9"; $0.deviceModel = "SM-X110"; $0.appName = "com.demo.shop"
            $0.sdkName = "android"; $0.sdkVersion = "0.1.0"; $0.connection = "wifi"; $0.osVersion = "Android 14"; $0.code = "482913"
            $0.expiresMs = Int64(Date().timeIntervalSince1970 * 1000) + 47_000
        }])
        let wc = MainWindowController(model: model)
        let w = wc.window!
        w.setContentSize(NSSize(width: 1320, height: 820))
        w.orderBack(nil)

        func snap(_ name: String) {
            RunLoop.main.run(until: Date().addingTimeInterval(0.4))
            w.contentView?.setFrameSize(NSSize(width: 1440, height: 900))
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
            wc.root.selectTab(2)
            snap("mac-inspector-\(scheme)")
            wc.root.list.select(id: 9)
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
            wc.root.selectTab(0)
            snap("mac-pinned-\(scheme)")
            wc.root.selectTab(3)
            wc.root.list.select(id: 39)
            RunLoop.main.run(until: Date().addingTimeInterval(0.3))
            wc.root.selectTab(3)
            snap("mac-timing-\(scheme)")
            composerFixture(model.composer)
            wc.root.setSection(.composer)
            snap("mac-composer-\(scheme)")
            // Every section of the editor and the response, and the sidebar tabs.
            let cm = model.composer
            for i in 0..<5 {
                cm.tab.section = i
                cm.onTab?()
                snap("mac-composer-edit\(i)-\(scheme)")
            }
            cm.tab.section = 0
            for i in 0..<5 {
                cm.tab.responseSection = i
                cm.onTab?()
                snap("mac-composer-res\(i)-\(scheme)")
            }
            cm.tab.responseSection = 0
            cm.onTab?()
            wc.root.composerView.showSidebarTab(1)
            snap("mac-composer-envs-\(scheme)")
            wc.root.composerView.showSidebarTab(2)
            snap("mac-composer-history-\(scheme)")
            wc.root.composerView.showSidebarTab(0)
            model.composer.fixtureResolved = nil
            model.composer.loadFixture(.init(), draft: ComposerModel.blank(), response: nil, sent: nil)
            snap("mac-composer-empty-\(scheme)")
            wc.root.setSection(.settings)
            snap("mac-settings-\(scheme)")
            wc.root.setSection(.breakpoints)
            wc.root.approval.update(approvalFixture())
            snap("mac-breakpoints-\(scheme)")
            wc.root.setSection(.rules)
            snap("mac-rules-\(scheme)")
            wc.root.rules.startNew(.mock)
            snap("mac-rules-editor-\(scheme)")
            wc.root.rules.cancelEditing()
            wc.root.approval.update([])
            wc.root.setSection(.devices)
            snap("mac-devices-\(scheme)")
            wc.root.setSection(.inspector)
            wc.root.showPalette()
            snap("mac-palette-\(scheme)")
            wc.root.closePalette()
        }
    }
}
