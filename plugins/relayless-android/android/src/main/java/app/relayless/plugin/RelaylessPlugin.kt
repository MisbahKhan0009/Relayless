package app.relayless.plugin

import android.Manifest
import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.os.PowerManager
import android.provider.Settings
import android.webkit.MimeTypeMap
import androidx.core.app.ActivityCompat
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class ServiceArgs {
    var title: String = "Relayless"
    var text: String = ""
}

@InvokeArg
class PathArgs {
    var path: String = ""
}

@TauriPlugin
class RelaylessPlugin(private val activity: Activity) : Plugin(activity) {

    private fun granted(value: Boolean) = JSObject().apply { put("granted", value) }

    private fun hasAllFiles(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Environment.isExternalStorageManager()
        } else {
            ContextCompat.checkSelfPermission(activity, Manifest.permission.WRITE_EXTERNAL_STORAGE) ==
                PackageManager.PERMISSION_GRANTED
        }

    @Command
    fun hasAllFilesAccess(invoke: Invoke) {
        invoke.resolve(granted(hasAllFiles()))
    }

    /** Opens the system screen where the user allows "All files access". */
    @Command
    fun requestAllFilesAccess(invoke: Invoke) {
        if (hasAllFiles()) {
            invoke.resolve(granted(true))
            return
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            try {
                val i = Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION)
                i.data = Uri.parse("package:" + activity.packageName)
                activity.startActivity(i)
            } catch (e: ActivityNotFoundException) {
                activity.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
            }
        } else {
            ActivityCompat.requestPermissions(
                activity,
                arrayOf(Manifest.permission.READ_EXTERNAL_STORAGE, Manifest.permission.WRITE_EXTERNAL_STORAGE),
                4801
            )
        }
        invoke.resolve(granted(false))
    }

    @Command
    fun deviceInfo(invoke: Invoke) {
        val ret = JSObject()
        val manufacturer = Build.MANUFACTURER ?: ""
        val model = Build.MODEL ?: "Android"
        val nice = if (model.lowercase().startsWith(manufacturer.lowercase())) model
        else "${manufacturer.replaceFirstChar { it.uppercase() }} $model"
        ret.put("model", nice.trim())
        ret.put("manufacturer", manufacturer)
        ret.put("sdk", Build.VERSION.SDK_INT)
        invoke.resolve(ret)
    }

    @Command
    fun requestNotificationPermission(invoke: Invoke) {
        if (Build.VERSION.SDK_INT >= 33) {
            val ok = ContextCompat.checkSelfPermission(activity, Manifest.permission.POST_NOTIFICATIONS) ==
                PackageManager.PERMISSION_GRANTED
            if (!ok) {
                ActivityCompat.requestPermissions(activity, arrayOf(Manifest.permission.POST_NOTIFICATIONS), 4802)
            }
            invoke.resolve(granted(ok))
        } else {
            invoke.resolve(granted(true))
        }
    }

    private fun unrestricted(): Boolean {
        val pm = activity.getSystemService(Context.POWER_SERVICE) as PowerManager
        return pm.isIgnoringBatteryOptimizations(activity.packageName)
    }

    @Command
    fun batteryStatus(invoke: Invoke) {
        invoke.resolve(JSObject().apply { put("unrestricted", unrestricted()) })
    }

    @Command
    fun requestIgnoreBatteryOptimizations(invoke: Invoke) {
        // Opens the battery optimization list (no special permission needed);
        // the user picks Relayless and chooses "Don't optimize" / "Unrestricted".
        if (!unrestricted()) {
            try {
                activity.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
            } catch (e: ActivityNotFoundException) {
                val i = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
                i.data = Uri.parse("package:" + activity.packageName)
                activity.startActivity(i)
            }
        }
        invoke.resolve(JSObject().apply { put("unrestricted", unrestricted()) })
    }

    /** Opens a finished download in whatever app handles its type. */
    @Command
    fun openFile(invoke: Invoke) {
        val args = invoke.parseArgs(PathArgs::class.java)
        try {
            val file = File(args.path)
            val uri = FileProvider.getUriForFile(activity, activity.packageName + ".fileprovider", file)
            val ext = file.extension.lowercase()
            val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "*/*"
            val i = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mime)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            activity.startActivity(Intent.createChooser(i, "Open with"))
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("Couldn't open the file: ${e.message}")
        }
    }

    /** Starts the foreground service, or updates its notification text. */
    @Command
    fun setService(invoke: Invoke) {
        val args = invoke.parseArgs(ServiceArgs::class.java)
        if (TransferService.running) {
            // Already in the foreground: just refresh the notification. Restarting the
            // service from the background is not allowed on Android 12+.
            val nm = activity.getSystemService(Context.NOTIFICATION_SERVICE) as android.app.NotificationManager
            nm.notify(TransferService.NOTIFICATION_ID, TransferService.build(activity, args.title, args.text))
            invoke.resolve(JSObject())
            return
        }
        try {
            val i = Intent(activity, TransferService::class.java).apply {
                action = TransferService.ACTION_UPDATE
                putExtra(TransferService.EXTRA_TITLE, args.title)
                putExtra(TransferService.EXTRA_TEXT, args.text)
            }
            ContextCompat.startForegroundService(activity.applicationContext, i)
            invoke.resolve(JSObject())
        } catch (e: Exception) {
            invoke.reject("Couldn't start background service: ${e.message}")
        }
    }

    @Command
    fun stopService(invoke: Invoke) {
        activity.applicationContext.stopService(Intent(activity, TransferService::class.java))
        invoke.resolve(JSObject())
    }
}
