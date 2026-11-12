// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

package com.argentum.bridge;

import android.app.Activity;
import android.content.ClipData;
import android.content.Intent;
import android.net.Uri;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/**
 * Writes a session export to the app's cache and opens the share sheet for it.
 * Only the latest export is kept. Nothing leaves the device until the owner
 * picks a destination in the share sheet.
 */
public final class ArgentumSessionShare {
    private ArgentumSessionShare() {}

    public static boolean share(Activity activity, String name, String json) {
        if (activity == null || json == null || !ArgentumExportProvider.isExportName(name)) {
            return false;
        }
        File folder = ArgentumExportProvider.folder(activity);
        File[] previous = folder.listFiles();
        if (previous != null) {
            for (File file : previous) {
                file.delete();
            }
        }
        File file = new File(folder, name);
        try (FileOutputStream output = new FileOutputStream(file)) {
            output.write(json.getBytes(StandardCharsets.UTF_8));
        } catch (IOException exception) {
            file.delete();
            return false;
        }
        Uri uri = ArgentumExportProvider.uriFor(activity, name);
        Intent send = new Intent(Intent.ACTION_SEND);
        send.setType(ArgentumExportProvider.MIME_TYPE);
        send.putExtra(Intent.EXTRA_STREAM, uri);
        send.putExtra(Intent.EXTRA_SUBJECT, "Argentum session export");
        send.putExtra(Intent.EXTRA_TITLE, name);
        send.setClipData(ClipData.newRawUri(name, uri));
        send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        Intent chooser = Intent.createChooser(send, "Export session");
        chooser.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
        try {
            activity.startActivity(chooser);
            return true;
        } catch (RuntimeException exception) {
            return false;
        }
    }
}
