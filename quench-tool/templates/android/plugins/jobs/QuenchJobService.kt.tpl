package {{ package }}

import android.app.job.JobParameters
import android.app.job.JobService
import android.content.Context
import android.util.Log
import dev.quench.Native
import org.json.JSONObject

/**
 * Runs one scheduled task by calling the native operation `job.<task>` with the app's files
 * directory. It does its work off the main thread, records what happened and tells the scheduler.
 */
class QuenchJobService : JobService() {
    override fun onStartJob(params: JobParameters): Boolean {
        val task = params.extras.getString("task") ?: return false
        Thread {
            val prefs = getSharedPreferences(Jobs.PREFS, Context.MODE_PRIVATE)
            val started = System.currentTimeMillis()
            val result = try {
                val input = JSONObject().put("dir", filesDir.absolutePath).put("task", task).toString()
                String(Native.invoke(applicationContext, "job.$task", input.toByteArray()))
            } catch (e: Exception) {
                "error: ${e.message}"
            }
            prefs.edit()
                .putString("$task.result", result)
                .putLong("$task.at", started)
                .putInt("$task.runs", prefs.getInt("$task.runs", 0) + 1)
                .apply()
            Log.i("quench-job", "$task: $result")
            jobFinished(params, result.startsWith("error:"))
        }.start()
        return true
    }

    /** The scheduler stopped the job (constraints no longer hold): try again later. */
    override fun onStopJob(params: JobParameters): Boolean = true
}
