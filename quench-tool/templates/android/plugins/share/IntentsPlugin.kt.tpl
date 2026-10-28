package {{ package }}

import android.content.Intent
import org.json.JSONArray
import org.json.JSONObject

/**
 * What other apps send to this one: a share, an opened file. The intents the manifest accepts are
 * its `<intent-filter>`s (see the activity); each one that arrives is kept until the page takes it
 * (`take`, which a page calls at start) and is pushed as an `intent` event while a page is connected.
 * An intent is `{action, type, uri, text}`.
 */
class IntentsPlugin : Plugin("intents") {
    private val pending = mutableListOf<JSONObject>()

    override fun onIntent(intent: Intent) {
        val item = JSONObject()
            .put("action", intent.action ?: "")
            .put("type", intent.type ?: "")
            .put("uri", (intent.data ?: intent.getParcelableExtra<android.net.Uri>(Intent.EXTRA_STREAM))?.toString() ?: "")
            .put("text", intent.getStringExtra(Intent.EXTRA_TEXT) ?: "")
        synchronized(pending) { pending.add(item) }
        emit("intent", item)
        // The intent that started the app is for the first reader only.
        intent.action = Intent.ACTION_MAIN
    }

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "take" -> synchronized(pending) {
                call.resolve(JSONArray(pending.toList()))
                pending.clear()
            }
            else -> call.reject("unknown_method", "intents has no method `$method`")
        }
    }
}
