package {{ package }}

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.security.GeneralSecurityException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Small secrets (tokens, keys) encrypted with an AES-256-GCM key that lives in the Android
 * Keystore: the key never leaves it (hardware-backed where the device has the hardware), and what
 * is written to the app's preferences is ciphertext. Each value is bound to its key name, so a
 * value cannot be moved to another name. Usable from anywhere in the app, a background job
 * included: one process, one set of preferences, one key.
 */
class SecureStore(context: Context) {
    private val prefs = context.getSharedPreferences("quench-secure", Context.MODE_PRIVATE)

    fun set(key: String, value: String) {
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.ENCRYPT_MODE, secret())
        cipher.updateAAD(key.toByteArray())
        val sealed = cipher.iv + cipher.doFinal(value.toByteArray())
        prefs.edit().putString(PREFIX + key, Base64.encodeToString(sealed, Base64.NO_WRAP)).apply()
    }

    /** The value, or null when there is none. Throws if it cannot be decrypted (damaged, or the key is gone). */
    fun get(key: String): String? {
        val sealed = Base64.decode(prefs.getString(PREFIX + key, null) ?: return null, Base64.NO_WRAP)
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.DECRYPT_MODE, secret(), GCMParameterSpec(128, sealed, 0, IV_BYTES))
        cipher.updateAAD(key.toByteArray())
        return String(cipher.doFinal(sealed, IV_BYTES, sealed.size - IV_BYTES))
    }

    fun remove(key: String) = prefs.edit().remove(PREFIX + key).apply()

    fun keys(): List<String> = prefs.all.keys.filter { it.startsWith(PREFIX) }.map { it.removePrefix(PREFIX) }.sorted()

    fun clear() = prefs.edit().clear().apply()

    private fun secret(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return generator.generateKey()
    }

    private companion object {
        const val ALIAS = "quench-secure-storage"
        const val PREFIX = "v."
        const val TRANSFORM = "AES/GCM/NoPadding"
        const val IV_BYTES = 12
    }
}

/** Methods: `set {key, value}`, `get {key}` (null when absent), `remove {key}`, `keys`, `clear`. */
class SecureStoragePlugin(activity: Activity) : Plugin("secure_storage") {
    private val store = SecureStore(activity.applicationContext)

    override fun call(method: String, args: JSONObject, call: Call) {
        try {
            when (method) {
                "set" -> { store.set(args.getString("key"), args.getString("value")); call.resolve() }
                "get" -> call.resolve(store.get(args.getString("key")))
                "remove" -> { store.remove(args.getString("key")); call.resolve() }
                "keys" -> call.resolve(JSONArray(store.keys()))
                "clear" -> { store.clear(); call.resolve() }
                else -> call.reject("unknown_method", "secure_storage has no method `$method`")
            }
        } catch (e: GeneralSecurityException) {
            call.reject("corrupt", "the value cannot be decrypted: ${e.message}")
        }
    }
}
