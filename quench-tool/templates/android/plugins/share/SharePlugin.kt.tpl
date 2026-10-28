package {{ package }}

import android.app.Activity
import android.content.Intent
import android.net.Uri
import org.json.JSONObject

/**
 * Sending things to other apps and opening them in other apps. Methods: `text {text, title}`,
 * `file {uri, mime, title}` (a `content://` URI, such as one the file picker gave), `open {uri, mime}`.
 */
class SharePlugin(private val activity: Activity) : Plugin("share") {
    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "text" -> {
                val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, args.getString("text"))
                activity.startActivity(Intent.createChooser(send, args.optString("title")))
                call.resolve()
            }
            "file" -> {
                val send = Intent(Intent.ACTION_SEND).setType(args.optString("mime", "*/*"))
                    .putExtra(Intent.EXTRA_STREAM, Uri.parse(args.getString("uri"))).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                activity.startActivity(Intent.createChooser(send, args.optString("title")))
                call.resolve()
            }
            "open" -> {
                val view = Intent(Intent.ACTION_VIEW).setDataAndType(Uri.parse(args.getString("uri")), args.optString("mime", "*/*"))
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                try {
                    activity.startActivity(view)
                    call.resolve()
                } catch (_: android.content.ActivityNotFoundException) {
                    call.reject("not_found", "no app can open this")
                }
            }
            else -> call.reject("unknown_method", "share has no method `$method`")
        }
    }
}
