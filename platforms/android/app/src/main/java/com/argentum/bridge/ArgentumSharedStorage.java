// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

package com.argentum.bridge;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Environment;
import android.provider.DocumentsContract;
import android.provider.Settings;

/**
 * Optional workspace on primary shared storage. Argentum works in its own
 * private folder unless the owner turns on All files access and picks a
 * folder. This class only reports the permission, opens Android's settings
 * screen for it, and returns which tree the owner picked. Rust maps the tree
 * to a path and checks it before anything uses it.
 */
public final class ArgentumSharedStorage {
    static final int REQUEST_FOLDER_PATH = 4108;

    public static final int ACCESS_UNSUPPORTED = 0;
    public static final int ACCESS_OFF = 1;
    public static final int ACCESS_ON = 2;

    private static final Object LOCK = new Object();
    private static String pickedFolder;

    private ArgentumSharedStorage() {}

    /** Whether All files access is on. Android 10 and older have no such setting. */
    public static int allFilesAccess() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return ACCESS_UNSUPPORTED;
        }
        return Environment.isExternalStorageManager() ? ACCESS_ON : ACCESS_OFF;
    }

    /** Opens Android's All files access screen for Argentum. */
    public static boolean openAllFilesSettings(Activity activity) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return false;
        }
        try {
            activity.startActivity(new Intent(
                    Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:" + activity.getPackageName())));
            return true;
        } catch (RuntimeException exception) {
            try {
                activity.startActivity(
                        new Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION));
                return true;
            } catch (RuntimeException ignored) {
                return false;
            }
        }
    }

    /** The real path of primary shared storage, such as /storage/emulated/0. */
    public static String primaryRoot() {
        java.io.File root = Environment.getExternalStorageDirectory();
        return root == null ? null : root.getAbsolutePath();
    }

    /**
     * Opens the system folder picker. No grant is kept: with All files access
     * the folder is opened by path, and without it nothing is opened.
     */
    public static boolean requestFolder(Activity activity) {
        synchronized (LOCK) {
            pickedFolder = null;
        }
        try {
            activity.startActivityForResult(
                    new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), REQUEST_FOLDER_PATH);
            return true;
        } catch (RuntimeException exception) {
            return false;
        }
    }

    /** Handles only this class's own picker request. Other results return false. */
    public static boolean onActivityResult(int requestCode, int resultCode, Intent data) {
        if (requestCode != REQUEST_FOLDER_PATH) {
            return false;
        }
        String result = "cancel";
        if (resultCode == Activity.RESULT_OK && data != null && data.getData() != null) {
            Uri tree = data.getData();
            try {
                String documentId = DocumentsContract.getTreeDocumentId(tree);
                String authority = tree.getAuthority();
                if (authority != null && documentId != null
                        && authority.indexOf('\n') < 0 && documentId.indexOf('\n') < 0) {
                    result = "tree\n" + authority + "\n" + documentId;
                } else {
                    result = "invalid";
                }
            } catch (RuntimeException exception) {
                result = "invalid";
            }
        }
        synchronized (LOCK) {
            pickedFolder = result;
        }
        return true;
    }

    /**
     * Returns and clears the last picker result: null while none is waiting,
     * "cancel", "invalid", or "tree\n" + authority + "\n" + document ID.
     */
    public static String takePickedFolder() {
        synchronized (LOCK) {
            String result = pickedFolder;
            pickedFolder = null;
            return result;
        }
    }

    /**
     * Starts Argentum again in a new process. Called after the native host has
     * shut down, so the database is closed. The interface toolkit can only
     * start once per process, which is why the process ends here.
     */
    public static void restart(Activity activity) {
        Intent launch = activity.getPackageManager()
                .getLaunchIntentForPackage(activity.getPackageName());
        if (launch != null) {
            launch.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
            activity.startActivity(launch);
        }
        Runtime.getRuntime().exit(0);
    }
}
