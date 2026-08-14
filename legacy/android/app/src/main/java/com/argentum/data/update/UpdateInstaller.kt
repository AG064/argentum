package com.argentum.data.update

import android.app.DownloadManager
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.os.Environment
import androidx.core.content.FileProvider
import kotlinx.coroutines.suspendCancellableCoroutine
import java.io.File
import kotlin.coroutines.resume

/**
 * Downloads an APK using the system [DownloadManager] and returns a content URI
 * the system package installer can consume via [Intent.ACTION_VIEW].
 *
 * Why [DownloadManager] and not OkHttp? The system service handles
 * - background downloads (survives the activity being killed)
 * - large file support
 * - the system "Download complete" notification
 * - resume across app restarts
 *
 * Flow:
 *   1. enqueue(url)            -> returns downloadId
 *   2. await download completion (BroadcastReceiver + ContentResolver poll)
 *   3. resolve the local file via FileProvider -> content:// URI
 *   4. caller fires ACTION_INSTALL_PACKAGE
 */
class UpdateInstaller(private val context: Context) {

    /**
     * Download the APK and return a content:// URI ready for the installer.
     *
     * @throws UpdateInstallException on download failure or unresolved file.
     */
    suspend fun downloadAndResolve(url: String, suggestedName: String): Uri {
        val dm = context.getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager

        val request = DownloadManager.Request(Uri.parse(url))
            .setTitle("Argentum update")
            .setDescription("Downloading $suggestedName")
            .setMimeType("application/vnd.android.package-archive")
            .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
            .setDestinationInExternalFilesDir(
                context,
                Environment.DIRECTORY_DOWNLOADS,
                suggestedName,
            )
            .setAllowedOverMetered(true)
            .setAllowedOverRoaming(false)

        val downloadId = dm.enqueue(request)
        val targetFile = File(
            context.getExternalFilesDir(Environment.DIRECTORY_DOWNLOADS),
            suggestedName,
        )

        return waitForDownload(dm, downloadId, targetFile)
    }

    private suspend fun waitForDownload(
        dm: DownloadManager,
        downloadId: Long,
        targetFile: File,
    ): Uri = suspendCancellableCoroutine { cont ->
        val receiver = object : BroadcastReceiver() {
            override fun onReceive(ctx: Context?, intent: Intent?) {
                val id = intent?.getLongExtra(DownloadManager.EXTRA_DOWNLOAD_ID, -1L)
                if (id == downloadId) {
                    try {
                        ctx?.unregisterReceiver(this)
                    } catch (_: IllegalArgumentException) {
                        // already unregistered
                    }
                    if (cont.isActive) {
                        val status = queryStatus(dm, downloadId)
                        if (status == DownloadManager.STATUS_SUCCESSFUL && targetFile.exists()) {
                            cont.resume(fileProviderUriFor(targetFile))
                        } else {
                            cont.cancel(
                                UpdateInstallException(
                                    "Download did not complete (status=$status)"
                                )
                            )
                        }
                    }
                }
            }
        }

        val filter = IntentFilter(DownloadManager.ACTION_DOWNLOAD_COMPLETE)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            context.registerReceiver(receiver, filter, Context.RECEIVER_EXPORTED)
        } else {
            @Suppress("UnspecifiedRegisterReceiverFlag")
            context.registerReceiver(receiver, filter)
        }

        cont.invokeOnCancellation {
            try {
                context.unregisterReceiver(receiver)
            } catch (_: IllegalArgumentException) {
                // ignore
            }
            dm.remove(downloadId)
        }
    }

    private fun queryStatus(dm: DownloadManager, downloadId: Long): Int {
        val query = DownloadManager.Query().setFilterById(downloadId)
        dm.query(query).use { cursor: Cursor? ->
            if (cursor != null && cursor.moveToFirst()) {
                val statusIdx = cursor.getColumnIndex(DownloadManager.COLUMN_STATUS)
                if (statusIdx >= 0) return cursor.getInt(statusIdx)
            }
        }
        return DownloadManager.STATUS_FAILED
    }

    private fun fileProviderUriFor(file: File): Uri {
        // The authority must match the one declared in AndroidManifest.xml.
        // Package name is com.argentum; the .fileProvider suffix is the convention.
        return FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            file,
        )
    }
}

class UpdateInstallException(message: String) : Exception(message)
