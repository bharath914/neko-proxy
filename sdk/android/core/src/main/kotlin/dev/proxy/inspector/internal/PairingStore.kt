package dev.proxy.inspector.internal

import java.io.File
import java.util.UUID

/**
 * What survives a restart: this install's random id, and the token and certificate fingerprint of
 * the desktop it paired with. Nothing about captured traffic is ever stored.
 */
class PairingStore(private val dir: File) {
    private val file = File(dir, "inspector-pairing.txt")

    data class Pairing(
        val token: ByteArray, val fingerprint: ByteArray,
        val lastHost: String = "", val lastPort: Int = 0, val desktopName: String = "",
    )

    @Synchronized
    fun installId(): String {
        val f = File(dir, "inspector-install-id")
        runCatching { f.readText().trim() }.getOrNull()?.takeIf { it.length in 8..64 }?.let { return it }
        val id = UUID.randomUUID().toString()
        dir.mkdirs()
        writeAtomic(f, id)
        return id
    }

    @Synchronized
    fun load(): Pairing? {
        val kv = runCatching { file.readLines() }.getOrNull() ?: return null
        val m = kv.mapNotNull { l -> l.indexOf('=').takeIf { it > 0 }?.let { l.substring(0, it) to l.substring(it + 1) } }.toMap()
        return try {
            Pairing(
                m.getValue("token").hexToBytes(), m.getValue("fp").hexToBytes(),
                m["host"].orEmpty(), m["port"]?.toIntOrNull() ?: 0, m["name"].orEmpty(),
            ).takeIf { it.token.size == 32 && it.fingerprint.size == 32 }
        } catch (e: Exception) { null }
    }

    @Synchronized
    fun save(p: Pairing) {
        dir.mkdirs()
        writeAtomic(file, "token=${p.token.toHex()}\nfp=${p.fingerprint.toHex()}\nhost=${p.lastHost}\nport=${p.lastPort}\nname=${p.desktopName}\n")
    }

    @Synchronized
    fun clear() { file.delete() }

    private fun writeAtomic(target: File, text: String) {
        val tmp = File(dir, target.name + ".tmp")
        tmp.writeText(text)
        // Owner only, where the file system has such a notion.
        tmp.setReadable(false, false); tmp.setReadable(true, true)
        tmp.setWritable(false, false); tmp.setWritable(true, true)
        if (!tmp.renameTo(target)) { target.delete(); tmp.renameTo(target) }
    }
}
