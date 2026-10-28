package {{ package }}

import android.app.Activity
import android.content.res.Configuration
import android.os.Bundle

/**
 * An activity whose whole screen is drawn by the app's Rust program ([QuenchSurfaceView]), with no WebView. It is
 * exported so it can be started with `adb shell am start -n {{ package }}/.SurfaceActivity`; make it the app's
 * launcher by moving the `LAUNCHER` intent filter of the manifest from `.MainActivity` to it.
 */
class SurfaceActivity : Activity() {
    private lateinit var surface: QuenchSurfaceView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        surface = QuenchSurfaceView(this)
        surface.exitHandler = { finish() }
        setContentView(surface)
        surface.setDark(isDark())
        surface.start()
    }

    private fun isDark(): Boolean =
        (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        surface.setDark(isDark())
    }

    override fun onResume() {
        super.onResume()
        surface.setVisible(true)
    }

    override fun onPause() {
        surface.setVisible(false)
        super.onPause()
    }

    // The program decides what Back does (close a sheet, go back a screen) and asks to leave when it has nothing left.
    @Deprecated("the platform's back handling")
    override fun onBackPressed() {
        surface.back()
    }

    override fun onDestroy() {
        surface.stop()
        super.onDestroy()
    }
}
