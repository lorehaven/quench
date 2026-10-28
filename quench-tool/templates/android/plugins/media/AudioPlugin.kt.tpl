package {{ package }}

import android.app.Activity
import android.media.AudioAttributes
import android.media.MediaPlayer
import android.net.Uri
import org.json.JSONObject

/**
 * Audio playback of a file or stream, one track at a time. Methods: `play {uri}` (a content, file or
 * http(s) URI), `pause`, `resume`, `stop`, `seek {ms}`, `state` (`{playing, position_ms, duration_ms}`).
 * Events: `ended`, `error {message}`.
 */
class AudioPlugin(private val activity: Activity) : Plugin("audio") {
    private var player: MediaPlayer? = null

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "play" -> play(Uri.parse(args.getString("uri")), call)
            "pause" -> { player?.pause(); call.resolve() }
            "resume" -> { player?.start(); call.resolve() }
            "stop" -> { release(); call.resolve() }
            "seek" -> { player?.seekTo(args.getInt("ms")); call.resolve() }
            "state" -> call.resolve(
                JSONObject()
                    .put("playing", player?.isPlaying == true)
                    .put("position_ms", player?.currentPosition ?: 0)
                    .put("duration_ms", player?.duration ?: 0)
            )
            else -> call.reject("unknown_method", "audio has no method `$method`")
        }
    }

    private fun play(uri: Uri, call: Call) {
        release()
        val p = MediaPlayer()
        player = p
        p.setAudioAttributes(AudioAttributes.Builder().setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).setUsage(AudioAttributes.USAGE_MEDIA).build())
        p.setOnCompletionListener { emit("ended") }
        p.setOnErrorListener { _, what, extra ->
            emit("error", JSONObject().put("message", "media error $what/$extra"))
            true
        }
        try {
            p.setDataSource(activity, uri)
            p.setOnPreparedListener {
                it.start()
                call.resolve(JSONObject().put("duration_ms", it.duration))
            }
            p.prepareAsync()
        } catch (e: Exception) {
            release()
            call.reject("failed", e.message ?: "cannot play")
        }
    }

    private fun release() {
        player?.release()
        player = null
    }

    override fun disconnected() = release()
}
