#if DEBUG
import Foundation

/// Keeps bodies the desktop has not asked for yet, newest last, within a byte budget. Never written to disk.
final class BodyRing {
    private var map: [String: Data] = [:]
    private var order: [String] = []
    private var used = 0
    private let budget: Int
    private let lock = NSLock()

    init(budget: Int = 5 << 20) { self.budget = budget }

    @discardableResult
    func put(_ callId: String, _ part: Int, _ body: Data) -> Bool {
        lock.lock(); defer { lock.unlock() }
        if body.count > budget { return false }
        let key = "\(callId)/\(part)"
        if let old = map.removeValue(forKey: key) { used -= old.count; order.removeAll { $0 == key } }
        while used + body.count > budget, let first = order.first {
            order.removeFirst()
            if let d = map.removeValue(forKey: first) { used -= d.count }
        }
        map[key] = body; order.append(key); used += body.count
        return true
    }

    func get(_ callId: String, _ part: Int) -> Data? { lock.lock(); defer { lock.unlock() }; return map["\(callId)/\(part)"] }
    func clear() { lock.lock(); map = [:]; order = []; used = 0; lock.unlock() }
    var size: Int { lock.lock(); defer { lock.unlock() }; return used }
}

/// A bounded queue that never blocks the producer: when full, the oldest entry is dropped and counted.
final class DropOldestQueue<T> {
    private var items: [T] = []
    private let capacity: Int
    private let cond = NSCondition()
    private(set) var dropped = 0

    init(capacity: Int) { self.capacity = capacity }

    func offer(_ item: T) {
        cond.lock()
        while items.count >= capacity { items.removeFirst(); dropped += 1 }
        items.append(item)
        cond.signal()
        cond.unlock()
    }

    func poll(timeout: TimeInterval) -> T? {
        cond.lock(); defer { cond.unlock() }
        if items.isEmpty { _ = cond.wait(until: Date(timeIntervalSinceNow: timeout)) }
        return items.isEmpty ? nil : items.removeFirst()
    }

    var count: Int { cond.lock(); defer { cond.unlock() }; return items.count }
    func clear() { cond.lock(); items = []; cond.unlock() }
}

/// A bounded copy of a body as the app reads or writes it.
final class Tee {
    private let cap: Int
    private(set) var data = Data()
    private(set) var total: Int64 = 0
    private(set) var truncated = false
    init(cap: Int) { self.cap = cap }

    func write(_ d: Data) {
        total += Int64(d.count)
        let room = cap - data.count
        if d.count <= room { data.append(d) } else { if room > 0 { data.append(d.prefix(room)) }; truncated = true }
    }
}
#endif
