package {{ package }}

import android.app.Activity
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.widget.RemoteViews
import org.json.JSONObject

/**
 * A home-screen widget showing a title and a line of text the app sets, and opening the app when
 * tapped. Methods: `set {title, body}` (updates every placed widget), `pin` (asks the launcher to
 * add one, where it can), `count` (how many are placed).
 */
class WidgetPlugin(private val activity: Activity) : Plugin("widget") {
    private val context = activity.applicationContext

    override fun call(method: String, args: JSONObject, call: Call) {
        val manager = AppWidgetManager.getInstance(context)
        val component = ComponentName(context, QuenchWidgetProvider::class.java)
        when (method) {
            "set" -> {
                context.getSharedPreferences(QuenchWidgetProvider.PREFS, Context.MODE_PRIVATE).edit()
                    .putString("title", args.optString("title")).putString("body", args.optString("body")).apply()
                QuenchWidgetProvider.render(context, manager, manager.getAppWidgetIds(component))
                call.resolve()
            }
            "pin" -> call.resolve(JSONObject().put("requested", manager.isRequestPinAppWidgetSupported && manager.requestPinAppWidget(component, null, null)))
            "count" -> call.resolve(JSONObject().put("count", manager.getAppWidgetIds(component).size))
            else -> call.reject("unknown_method", "widget has no method `$method`")
        }
    }
}

class QuenchWidgetProvider : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, ids: IntArray) = render(context, manager, ids)

    companion object {
        const val PREFS = "quench-widget"

        fun render(context: Context, manager: AppWidgetManager, ids: IntArray) {
            val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val open = PendingIntent.getActivity(
                context, 0, context.packageManager.getLaunchIntentForPackage(context.packageName),
                PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
            )
            for (id in ids) {
                val views = RemoteViews(context.packageName, R.layout.quench_widget)
                views.setTextViewText(R.id.quench_widget_title, prefs.getString("title", "") ?: "")
                views.setTextViewText(R.id.quench_widget_body, prefs.getString("body", "") ?: "")
                views.setOnClickPendingIntent(R.id.quench_widget_root, open)
                manager.updateAppWidget(id, views)
            }
        }
    }
}
