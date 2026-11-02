package app.farsail.android

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** No backup, no plaintext fallback. JNI calls these methods from native workers. */
class CredentialStore(context: Context) {
    private val directory = File(context.noBackupFilesDir, "credentials").apply { mkdirs() }
    private val alias = "farsail.credentials.v1"
    private fun file(name: String): AtomicFile {
        require(name.matches(Regex("[A-Za-z0-9-]+")))
        return AtomicFile(File(directory, "$name.bin"))
    }
    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(alias, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        }.generateKey()
    }
    @Synchronized fun read(name: String): ByteArray? {
        val f = file(name)
        if (!f.baseFile.exists() && !File(f.baseFile.path + ".bak").exists()) return null
        val bytes = f.readFully()
        require(bytes.size >= 29 && bytes[0] == 1.toByte())
        return Cipher.getInstance("AES/GCM/NoPadding").run {
            init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(1, 13)))
            updateAAD(name.toByteArray(Charsets.UTF_8))
            doFinal(bytes.copyOfRange(13, bytes.size))
        }
    }
    @Synchronized fun write(name: String, value: ByteArray) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply {
            init(Cipher.ENCRYPT_MODE, key()); updateAAD(name.toByteArray(Charsets.UTF_8))
        }
        val bytes = byteArrayOf(1) + cipher.iv + cipher.doFinal(value)
        val f = file(name)
        val output = f.startWrite()
        try { output.write(bytes); f.finishWrite(output) } catch (e: Exception) { f.failWrite(output); throw e }
    }
    @Synchronized fun delete(name: String) { file(name).delete() }
}
