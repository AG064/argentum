package com.argentum.bridge;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;

import java.io.File;
import java.io.FileNotFoundException;
import java.util.regex.Pattern;

/**
 * Serves the latest session export to the app the owner picks in the share
 * sheet. It is not exported: another app can read a file only through the
 * one-time read grant attached to the share intent. It serves only files named
 * like a session export from the app's own cache folder, read only.
 */
public final class ArgentumExportProvider extends ContentProvider {
    static final String MIME_TYPE = "application/json";
    private static final String FOLDER = "session-exports";
    private static final Pattern NAME = Pattern.compile("argentum-session-[0-9a-f]{8}\\.json");

    static String authority(Context context) {
        return context.getPackageName() + ".exports";
    }

    static boolean isExportName(String name) {
        return name != null && NAME.matcher(name).matches();
    }

    /** The folder exports are written to. Created if missing. */
    static File folder(Context context) {
        File folder = new File(context.getCacheDir(), FOLDER);
        if (!folder.isDirectory()) {
            folder.mkdirs();
        }
        return folder;
    }

    static Uri uriFor(Context context, String name) {
        return new Uri.Builder()
                .scheme("content")
                .authority(authority(context))
                .appendPath(name)
                .build();
    }

    private File fileFor(Uri uri) throws FileNotFoundException {
        Context context = getContext();
        if (context == null || uri == null || !authority(context).equals(uri.getAuthority())
                || uri.getPathSegments().size() != 1) {
            throw new FileNotFoundException();
        }
        String name = uri.getLastPathSegment();
        if (!isExportName(name)) {
            throw new FileNotFoundException();
        }
        File file = new File(folder(context), name);
        if (!file.isFile()) {
            throw new FileNotFoundException();
        }
        return file;
    }

    @Override
    public boolean onCreate() {
        return true;
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) {
            throw new SecurityException("Session exports are read only.");
        }
        return ParcelFileDescriptor.open(fileFor(uri), ParcelFileDescriptor.MODE_READ_ONLY);
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection,
            String[] selectionArgs, String sortOrder) {
        File file;
        try {
            file = fileFor(uri);
        } catch (FileNotFoundException exception) {
            return null;
        }
        String[] columns = projection != null
                ? projection
                : new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE};
        MatrixCursor cursor = new MatrixCursor(columns, 1);
        Object[] row = new Object[columns.length];
        for (int index = 0; index < columns.length; index++) {
            if (OpenableColumns.DISPLAY_NAME.equals(columns[index])) {
                row[index] = file.getName();
            } else if (OpenableColumns.SIZE.equals(columns[index])) {
                row[index] = file.length();
            }
        }
        cursor.addRow(row);
        return cursor;
    }

    @Override
    public String getType(Uri uri) {
        return MIME_TYPE;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int delete(Uri uri, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] selectionArgs) {
        throw new UnsupportedOperationException();
    }
}
