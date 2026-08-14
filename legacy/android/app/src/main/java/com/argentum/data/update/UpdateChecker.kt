package com.argentum.data.update

import com.argentum.BuildConfig
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Result of a check-for-updates call.
 *
 * - [UpdateAvailable] is shown when the remote version is strictly newer than
 *   the running [currentVersion].
 * - [UpToDate] is shown when the remote version is equal to or older than the
 *   running version (we don't downgrade).
 * - [Error] is shown for any network or parse failure; the user can retry.
 */
sealed interface UpdateResult {
    data class UpdateAvailable(
        val version: String,
        val releaseNotes: String,
        val apkUrl: String,
        val apkSize: Long,
        val releasePageUrl: String,
    ) : UpdateResult

    data class UpToDate(val currentVersion: String) : UpdateResult
    data class Error(val message: String) : UpdateResult
}

/**
 * Lightweight GitHub Releases update checker.
 *
 * Talks to the public REST endpoint at
 *   `https://api.github.com/repos/AG064/argentum/releases/latest`
 *
 * No auth required. The endpoint is rate-limited to 60 requests/hour per IP,
 * which is plenty for an in-app "Check for updates" button.
 */
class UpdateChecker(
    private val currentVersion: String = BuildConfig.VERSION_NAME,
    private val releasesApiUrl: String = BuildConfig.GITHUB_RELEASES_URL,
    private val releasesPageUrl: String = BuildConfig.GITHUB_RELEASES_PAGE,
    private val client: OkHttpClient = defaultClient,
) {

    suspend fun check(): UpdateResult = withContext(Dispatchers.IO) {
        try {
            val request = Request.Builder()
                .url(releasesApiUrl)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .build()

            client.newCall(request).execute().use { response ->
                if (!response.isSuccessful) {
                    return@withContext UpdateResult.Error(
                        "GitHub returned HTTP ${response.code}"
                    )
                }

                val body = response.body?.string()
                    ?: return@withContext UpdateResult.Error("Empty response from GitHub")

                val json = JSONObject(body)
                val tag = json.optString("tag_name", "")
                val remoteVersion = tag.removePrefix("v").ifEmpty {
                    return@withContext UpdateResult.Error("Release tag missing")
                }

                if (!isRemoteNewer(remoteVersion, currentVersion)) {
                    return@withContext UpdateResult.UpToDate(currentVersion)
                }

                // Find the Android APK asset
                val assets = json.optJSONArray("assets")
                var apkUrl: String? = null
                var apkSize = 0L
                if (assets != null) {
                    for (i in 0 until assets.length()) {
                        val a = assets.getJSONObject(i)
                        val name = a.optString("name", "")
                        if (name.endsWith("-android.apk", ignoreCase = true) ||
                            name.endsWith("-android-v3.apk", ignoreCase = true)
                        ) {
                            apkUrl = a.optString("browser_download_url", null)
                            apkSize = a.optLong("size", 0L)
                            break
                        }
                    }
                }

                if (apkUrl == null) {
                    return@withContext UpdateResult.Error(
                        "No Android APK in the latest release"
                    )
                }

                val notes = json.optString("body", "")
                val htmlUrl = json.optString("html_url", releasesPageUrl)

                UpdateResult.UpdateAvailable(
                    version = remoteVersion,
                    releaseNotes = notes,
                    apkUrl = apkUrl,
                    apkSize = apkSize,
                    releasePageUrl = htmlUrl,
                )
            }
        } catch (e: Exception) {
            UpdateResult.Error(e.message ?: "Network error")
        }
    }

    private companion object {
        val defaultClient: OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(15, TimeUnit.SECONDS)
            .readTimeout(30, TimeUnit.SECONDS)
            .build()
    }
}

/**
 * Strict semver comparison: returns true if [remote] > [current].
 *
 * - Ignores leading `v`.
 * - Compares major, minor, patch numerically.
 * - Treats missing pre-release as higher than present (1.0.0 > 1.0.0-rc.1).
 * - Throws nothing — unparseable input falls back to a string compare.
 */
internal fun isRemoteNewer(remote: String, current: String): Boolean {
    val r = parseSemver(remote.trim().removePrefix("v"))
    val c = parseSemver(current.trim().removePrefix("v"))
    if (r == null || c == null) {
        // Fallback: lexical compare (still better than nothing).
        return remote != current && remote > current
    }
    if (r.major != c.major) return r.major > c.major
    if (r.minor != c.minor) return r.minor > c.minor
    if (r.patch != c.patch) return r.patch > c.patch

    if (r.prerelease == c.prerelease) {
        return false
    }
    // Equal major.minor.patch — a release is newer than a pre-release.
    if (r.prerelease == null) return true
    if (c.prerelease == null) return false

    return r.prerelease > c.prerelease
}

private data class Semver(
    val major: Int,
    val minor: Int,
    val patch: Int,
    val prerelease: String?,
)

private fun parseSemver(version: String): Semver? {
    val main = version.split("-", limit = 2)
    val parts = main[0].split(".")
    if (parts.size != 3) return null
    val major = parts[0].toIntOrNull()?.takeIf { it >= 0 } ?: return null
    val minor = parts[1].toIntOrNull()?.takeIf { it >= 0 } ?: return null
    val patch = parts[2].toIntOrNull()?.takeIf { it >= 0 } ?: return null
    return Semver(major, minor, patch, main.getOrNull(1)?.ifBlank { null })
}
