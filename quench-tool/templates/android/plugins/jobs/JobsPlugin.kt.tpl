package {{ package }}

import android.app.Activity
import android.app.job.JobInfo
import android.app.job.JobScheduler
import android.content.ComponentName
import android.content.Context
import android.os.PersistableBundle
import org.json.JSONArray
import org.json.JSONObject

/**
 * Background work. A job is a task name the native code knows (`job.<task>` in `native/`), run by
 * the system's job scheduler, in this app's process, even when the page and the activity are gone.
 * Constraints (network, charging, battery) are the scheduler's.
 *
 * Methods: `schedule {task, interval_minutes, network, charging, battery_not_low}`, `cancel {task}`,
 * `run_now {task}`, `pending`, `last_run {task}`.
 */
class JobsPlugin(private val activity: Activity) : Plugin("jobs") {
    private val scheduler get() = activity.getSystemService(JobScheduler::class.java)

    override fun call(method: String, args: JSONObject, call: Call) {
        when (method) {
            "schedule" -> {
                val task = args.getString("task")
                val minutes = args.optLong("interval_minutes", 15).coerceAtLeast(15)
                val info = builder(task, args).setPeriodic(minutes * 60_000L).setPersisted(true).build()
                call.resolve(JSONObject().put("scheduled", scheduler.schedule(info) == JobScheduler.RESULT_SUCCESS))
            }
            "run_now" -> {
                val task = args.getString("task")
                val info = builder(task, args, Jobs.oneOffId(task)).setOverrideDeadline(0).build()
                call.resolve(JSONObject().put("scheduled", scheduler.schedule(info) == JobScheduler.RESULT_SUCCESS))
            }
            "cancel" -> {
                val task = args.getString("task")
                scheduler.cancel(Jobs.id(task))
                scheduler.cancel(Jobs.oneOffId(task))
                call.resolve()
            }
            "pending" -> {
                val list = JSONArray()
                for (job in scheduler.allPendingJobs) {
                    val task = job.extras.getString("task") ?: continue
                    list.put(JSONObject().put("task", task).put("periodic", job.isPeriodic).put("interval_minutes", job.intervalMillis / 60_000L))
                }
                call.resolve(list)
            }
            "last_run" -> {
                val task = args.getString("task")
                val prefs = activity.getSharedPreferences(Jobs.PREFS, Context.MODE_PRIVATE)
                call.resolve(
                    JSONObject()
                        .put("runs", prefs.getInt("$task.runs", 0))
                        .put("at", prefs.getLong("$task.at", 0))
                        .put("result", prefs.getString("$task.result", ""))
                )
            }
            else -> call.reject("unknown_method", "jobs has no method `$method`")
        }
    }

    private fun builder(task: String, args: JSONObject, id: Int = Jobs.id(task)): JobInfo.Builder {
        val extras = PersistableBundle().apply { putString("task", task) }
        return JobInfo.Builder(id, ComponentName(activity, QuenchJobService::class.java))
            .setExtras(extras)
            .setRequiredNetworkType(
                when (args.optString("network", "any")) {
                    "none" -> JobInfo.NETWORK_TYPE_NONE
                    "connected" -> JobInfo.NETWORK_TYPE_ANY
                    "unmetered" -> JobInfo.NETWORK_TYPE_UNMETERED
                    else -> JobInfo.NETWORK_TYPE_NONE
                }
            )
            .setRequiresCharging(args.optBoolean("charging", false))
            .setRequiresBatteryNotLow(args.optBoolean("battery_not_low", false))
    }
}

object Jobs {
    const val PREFS = "quench-jobs"

    /** The scheduler's id for a task: stable, so scheduling it again replaces the earlier job. */
    fun id(task: String): Int = (task.hashCode() and 0x3fffffff)

    fun oneOffId(task: String): Int = id(task) or 0x40000000
}
