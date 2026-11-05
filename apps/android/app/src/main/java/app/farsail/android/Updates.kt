package app.farsail.android

import android.app.Activity
import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

data class UpdateState(val phase: String = "idle", val message: String = "", val version: String = "", val progress: Int = 0)

/** Updates never share the account/heartbeat executor or server credentials. */
class Updates(private val app: FarSailApp) {
    private val io = Executors.newSingleThreadExecutor()
    private val active = AtomicBoolean(false)
    private val cacheChecked = AtomicBoolean(false)
    private val prefs = app.getSharedPreferences("updates", 0)
    private val directory = File(app.cacheDir, "updates").apply { mkdirs() }
    private val apk = File(directory, "verified-update.apk")
    private var release: JSONObject? = null
    var state = UpdateState()
        private set
    val currentVersion: String get() = installed().versionName ?: ""
    val currentCode: Long get() = code(installed())
    val autoCheck get() = prefs.getBoolean("autoCheck", true)
    val autoDownload get() = prefs.getBoolean("autoDownload", true)
    val wifiOnly get() = prefs.getBoolean("wifiOnly", true)
    private fun installed() = app.packageManager.getPackageInfo(app.packageName, 0)
    @Suppress("DEPRECATION") private fun code(p: PackageInfo) = if (Build.VERSION.SDK_INT >= 28) p.longVersionCode else p.versionCode.toLong()
    fun preferences(check: Boolean, download: Boolean, wifi: Boolean = wifiOnly) {
        prefs.edit().putBoolean("autoCheck", check).putBoolean("autoDownload", download).putBoolean("wifiOnly", wifi).apply()
        maybeCheck()
    }
    private fun automaticDownloadAllowed(): Boolean {
        if (!autoDownload) return false
        if (!wifiOnly) return true
        val network = app.getSystemService(ConnectivityManager::class.java)
        val capabilities = network.getNetworkCapabilities(network.activeNetwork) ?: return false
        return capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) && !network.isActiveNetworkMetered
    }
    private fun show(next: UpdateState) { app.main.post { state = next; app.observer?.invoke() } }
    fun maybeCheck() {
        if (!cacheChecked.getAndSet(true) && apk.exists() && active.compareAndSet(false, true)) {
            io.execute {
                try {
                    val cached = app.unwrap(NativeBridge.verifyUpdate(File(directory, "release.json").readText(), currentCode)) as JSONObject
                    if (cached.getBoolean("available") && validPackage(apk, cached)) {
                        release = cached
                        show(UpdateState("ready", "更新已下载，可以安装。", cached.getString("version")))
                    }
                } catch (_: Exception) { /* A corrupt/obsolete cache cannot enable installation. */ }
                finally { active.set(false); app.main.post { maybeCheck() } }
            }
            return
        }
        if (state.phase in listOf("checking", "downloading", "ready")) return
        val now = System.currentTimeMillis()
        if (autoCheck && now - prefs.getLong("checkedAt", 0) > 6 * 60 * 60 * 1000 && now - prefs.getLong("attemptedAt", 0) > 15 * 60 * 1000) check()
        else if (state.phase == "available" && automaticDownloadAllowed()) download()
    }
    fun check() {
        if (!active.compareAndSet(false, true)) return
        prefs.edit().putLong("attemptedAt", System.currentTimeMillis()).apply()
        show(UpdateState("checking", "正在检查更新…"))
        io.execute {
            var shouldDownload = false
            try {
                val raw = readManifest()
                val result = app.unwrap(NativeBridge.verifyUpdate(raw, currentCode)) as JSONObject
                prefs.edit().putLong("checkedAt", System.currentTimeMillis()).apply()
                if (result.getBoolean("available")) {
                    release = result
                    File(directory, "release.json").writeText(raw)
                    if (apk.exists() && validPackage(apk, result)) show(UpdateState("ready", "更新已下载，可以安装。", result.getString("version")))
                    else { apk.delete(); show(UpdateState("available", result.optString("notes", "发现新版本"), result.getString("version"))); shouldDownload = automaticDownloadAllowed() }
                } else { release = null; apk.delete(); show(UpdateState("upToDate", "当前已是最新版本。")) }
            } catch (e: NotPublished) { prefs.edit().putLong("checkedAt", System.currentTimeMillis()).apply(); show(UpdateState("upToDate", "当前暂无已发布的安卓更新。")) }
            catch (_: Exception) { show(UpdateState("error", "无法检查更新，请检查网络后重试。")) }
            finally { active.set(false) }
            if (shouldDownload) download()
        }
    }
    fun download() {
        val next = release ?: return
        if (!active.compareAndSet(false, true)) return
        show(UpdateState("downloading", "正在下载更新…", next.getString("version")))
        io.execute {
            val temporary = File(directory, "download.part")
            try {
                val connection = connection(URL(next.getString("url")))
                try {
                    check(connection.responseCode == 200)
                    val size = next.getLong("size")
                    if (connection.contentLengthLong >= 0) check(connection.contentLengthLong == size)
                    var received = 0L; var shown = -1
                    connection.inputStream.use { input -> temporary.outputStream().use { output ->
                        val bytes = ByteArray(65536)
                        while (true) {
                            val count = input.read(bytes); if (count < 0) break
                            received += count; check(received <= size)
                            output.write(bytes, 0, count)
                            val percent = (received * 100 / size).toInt()
                            if (percent != shown) { shown = percent; show(UpdateState("downloading", "正在下载更新…", next.getString("version"), percent)) }
                        }
                    } }
                    check(received == size && validPackage(temporary, next))
                    check(!apk.exists() || apk.delete()); check(temporary.renameTo(apk))
                    show(UpdateState("ready", "更新已通过签名和完整性检查。", next.getString("version")))
                } finally { connection.disconnect() }
            } catch (_: Exception) { temporary.delete(); show(UpdateState("error", "更新下载或校验失败，请重新检查。")) }
            finally { active.set(false) }
        }
    }
    fun install(activity: Activity) {
        if (CaptureService.running) { show(state.copy(message = "请先停止屏幕共享，再安装更新。")); return }
        if (!app.packageManager.canRequestPackageInstalls()) {
            activity.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${app.packageName}")))
            show(state.copy(message = "允许此应用安装更新后，返回点击“安装更新”。")); return
        }
        if (!active.compareAndSet(false, true)) return
        io.execute {
            try {
                val manifest = File(directory, "release.json").readText()
                val next = app.unwrap(NativeBridge.verifyUpdate(manifest, currentCode)) as JSONObject
                check(next.getBoolean("available") && validPackage(apk, next))
                app.main.post {
                    if (!activity.isFinishing && !activity.isDestroyed && activity.hasWindowFocus() && !CaptureService.running) {
                        val uri = FileProvider.getUriForFile(app, "${app.packageName}.updates", apk)
                        activity.startActivity(Intent(Intent.ACTION_VIEW).setDataAndType(uri, "application/vnd.android.package-archive").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION))
                    }
                }
            } catch (_: Exception) { show(UpdateState("error", "更新校验未通过，请重新下载。")) }
            finally { active.set(false) }
        }
    }
    private fun validPackage(file: File, next: JSONObject): Boolean {
        if (file.length() != next.getLong("size")) return false
        val digest = MessageDigest.getInstance("SHA-256")
        file.inputStream().use { input -> val bytes = ByteArray(65536); while (true) { val n = input.read(bytes); if (n < 0) break; digest.update(bytes, 0, n) } }
        if (digest.digest().joinToString("") { "%02x".format(it) } != next.getString("sha256")) return false
        val flags = if (Build.VERSION.SDK_INT >= 28) PackageManager.GET_SIGNING_CERTIFICATES else @Suppress("DEPRECATION") PackageManager.GET_SIGNATURES
        val archive = app.packageManager.getPackageArchiveInfo(file.absolutePath, flags) ?: return false
        if (archive.packageName != app.packageName || code(archive) != next.getLong("versionCode") || archive.versionName != next.getString("version") || code(archive) <= currentCode) return false
        val current = app.packageManager.getPackageInfo(app.packageName, flags)
        return certificates(archive) == certificates(current) && certificates(current).isNotEmpty()
    }
    @Suppress("DEPRECATION") private fun certificates(p: PackageInfo): Set<String> {
        val certs = if (Build.VERSION.SDK_INT >= 28) p.signingInfo?.apkContentsSigners else p.signatures
        return certs.orEmpty().map { MessageDigest.getInstance("SHA-256").digest(it.toByteArray()).joinToString("") { b -> "%02x".format(b) } }.toSet()
    }
    private class NotPublished : Exception()
    private fun readManifest(): String {
        val c = connection(URL("https://github.com/wanghao9103/farsail/releases/download/android-updates/latest.json"))
        try {
            if (c.responseCode == 404) throw NotPublished()
            check(c.responseCode == 200)
            val bytes = c.inputStream.use { input ->
                val out = java.io.ByteArrayOutputStream(); val buffer = ByteArray(4096)
                while (true) { val n = input.read(buffer); if (n < 0) break; check(out.size() + n <= 32768); out.write(buffer, 0, n) }; out.toByteArray()
            }
            check(bytes.size <= 32768)
            return bytes.toString(Charsets.UTF_8)
        } finally { c.disconnect() }
    }
    private fun connection(start: URL): HttpURLConnection {
        var url = start
        repeat(6) {
            check(url.protocol == "https" && url.userInfo == null && url.port in listOf(-1, 443) && url.host in setOf("github.com", "release-assets.githubusercontent.com", "objects.githubusercontent.com"))
            val c = (url.openConnection() as HttpURLConnection).apply { connectTimeout = 15000; readTimeout = 30000; instanceFollowRedirects = false }
            if (c.responseCode in 300..399) { val location = c.getHeaderField("Location") ?: error("redirect"); c.disconnect(); url = URL(url, location) }
            else return c
        }
        error("redirect limit")
    }
}
