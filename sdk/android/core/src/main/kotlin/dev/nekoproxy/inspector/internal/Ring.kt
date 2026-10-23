package dev.nekoproxy.inspector.internal

/**
 * Keeps bodies the desktop has not asked for yet, newest last, within a byte budget. The oldest
 * go first. Nothing here is ever written to disk.
 */
class BodyRing(private val budget: Int = 5 shl 20) {
    private val map = LinkedHashMap<String, ByteArray>()
    private var used = 0

    @Synchronized
    fun put(callId: String, part: Int, body: ByteArray): Boolean {
        if (body.size > budget) return false
        val key = key(callId, part)
        map.remove(key)?.let { used -= it.size }
        while (used + body.size > budget && map.isNotEmpty()) {
            val it = map.entries.iterator()
            val e = it.next()
            used -= e.value.size
            it.remove()
        }
        map[key] = body
        used += body.size
        return true
    }

    @Synchronized
    fun get(callId: String, part: Int): ByteArray? = map[key(callId, part)]

    @Synchronized
    fun clear() { map.clear(); used = 0 }

    @Synchronized
    fun size(): Int = used

    private fun key(callId: String, part: Int) = "$callId/$part"
}

/**
 * A bounded queue that never blocks the producer: when full, the oldest entry is dropped and
 * counted. The consumer is the single sender thread.
 */
class DropOldestQueue<T : Any>(private val capacity: Int) {
    private val deque = java.util.concurrent.LinkedBlockingDeque<T>()
    private val dropped = java.util.concurrent.atomic.AtomicLong()

    fun offer(item: T) {
        while (deque.size >= capacity) {
            if (deque.pollFirst() != null) dropped.incrementAndGet() else break
        }
        deque.offerLast(item)
    }

    fun poll(timeoutMs: Long): T? = deque.poll(timeoutMs, java.util.concurrent.TimeUnit.MILLISECONDS)

    fun clear() = deque.clear()

    fun size() = deque.size

    fun dropped(): Long = dropped.get()
}
