package {{ package }}

import android.app.Activity
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import org.json.JSONArray
import org.json.JSONObject
import java.util.Locale

/**
 * Text to speech, with the device's voices. Methods: `speak {text, lang, rate, pitch}` (answers when
 * it starts; the event `done {id}` follows), `stop`, `voices` (the languages available).
 */
class TtsPlugin(activity: Activity) : Plugin("tts") {
    private var engine: TextToSpeech? = null
    private var ready = false
    private val waiting = mutableListOf<() -> Unit>()
    private var counter = 0

    private val context = activity.applicationContext

    private fun withEngine(then: () -> Unit) {
        if (ready) return then()
        waiting.add(then)
        if (engine == null) {
            engine = TextToSpeech(context) { status ->
                ready = status == TextToSpeech.SUCCESS
                val queued = waiting.toList()
                waiting.clear()
                if (ready) queued.forEach { it() }
            }.also { tts ->
                tts.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
                    override fun onStart(utteranceId: String) = emit("start", JSONObject().put("id", utteranceId))
                    override fun onDone(utteranceId: String) = emit("done", JSONObject().put("id", utteranceId))
                    @Deprecated("Deprecated in Java")
                    override fun onError(utteranceId: String) = emit("error", JSONObject().put("id", utteranceId))
                })
            }
        }
    }

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "speak" -> withEngine {
                val tts = engine!!
                tts.language = Locale.forLanguageTag(args.optString("lang", Locale.getDefault().toLanguageTag()))
                tts.setSpeechRate(args.optDouble("rate", 1.0).toFloat())
                tts.setPitch(args.optDouble("pitch", 1.0).toFloat())
                val id = "u${++counter}"
                val result = tts.speak(args.getString("text"), TextToSpeech.QUEUE_ADD, null, id)
                if (result == TextToSpeech.SUCCESS) call.resolve(JSONObject().put("id", id)) else call.reject("failed", "the engine refused the text")
            }
            "stop" -> { engine?.stop(); call.resolve() }
            "voices" -> withEngine {
                val list = JSONArray()
                engine!!.availableLanguages?.map { it.toLanguageTag() }?.sorted()?.forEach { list.put(it) }
                call.resolve(list)
            }
            else -> call.reject("unknown_method", "tts has no method `$method`")
        }
    }

    override fun disconnected() {
        engine?.stop()
    }
}
