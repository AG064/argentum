package com.argentum.bridge;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.net.Uri;

final class ArgentumStorageBridge {
    private static final int REQUEST_WORKSPACE_TREE = 4107;
    private static final String PREFERENCES = "argentum.storage-tree.v1";
    private static final String URI_KEY = "workspace_uri";
    private static final int REQUIRED_FLAGS =
            Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION;
    static final int RELEASE_NONE = 0;
    static final int RELEASE_SELECTED = 1;
    static final int RELEASE_PREVIOUS = 2;

    private ArgentumStorageBridge() {}

    interface GrantFacade {
        String storedUri();
        boolean storeUri(String uri);
        boolean removeStoredUri();
        void take(Uri uri, int flags) throws SecurityException;
        void release(Uri uri, int flags) throws SecurityException;
    }

    static void requestTree(Activity activity) {
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        intent.addFlags(REQUIRED_FLAGS | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
        activity.startActivityForResult(intent, REQUEST_WORKSPACE_TREE);
    }

    static boolean onActivityResult(Activity activity, int requestCode, int resultCode, Intent data) {
        if (requestCode != REQUEST_WORKSPACE_TREE) {
            return false;
        }
        if (resultCode != Activity.RESULT_OK || data == null || data.getData() == null) {
            return true;
        }
        int granted = data.getFlags() & REQUIRED_FLAGS;
        if (granted != REQUIRED_FLAGS) {
            return true;
        }
        Uri uri = data.getData();
        persistSelection(new AndroidGrantFacade(activity), uri);
        return true;
    }

    static String persistedUri(Context context) {
        return preferences(context).getString(URI_KEY, null);
    }

    static void clearPersistedUri(Context context) {
        clearSelection(new AndroidGrantFacade(context));
    }

    static boolean persistSelection(GrantFacade facade, Uri selected) {
        String next = selected.toString();
        String previous;
        try {
            previous = facade.storedUri();
        } catch (RuntimeException exception) {
            return false;
        }
        boolean sameRecordedUri = previous != null && previous.equals(next);
        try {
            facade.take(selected, REQUIRED_FLAGS);
        } catch (SecurityException ignored) {
            return false;
        }
        boolean committed;
        try {
            committed = facade.storeUri(next);
        } catch (RuntimeException exception) {
            committed = false;
        }
        int releaseDecision = grantReleaseDecision(
                sameRecordedUri,
                previous != null,
                committed);
        if (releaseDecision == RELEASE_SELECTED) {
            releaseToleratingRevocation(facade, selected);
        } else if (releaseDecision == RELEASE_PREVIOUS) {
            releaseToleratingRevocation(facade, Uri.parse(previous));
        }
        if (!committed) {
            return false;
        }
        return true;
    }

    static int grantReleaseDecision(
            boolean sameRecordedUri,
            boolean hasPrevious,
            boolean committed) {
        if (!committed) {
            return sameRecordedUri ? RELEASE_NONE : RELEASE_SELECTED;
        }
        return hasPrevious && !sameRecordedUri ? RELEASE_PREVIOUS : RELEASE_NONE;
    }

    static boolean clearSelection(GrantFacade facade) {
        String stored;
        try {
            stored = facade.storedUri();
        } catch (RuntimeException exception) {
            return false;
        }
        if (stored == null) {
            return true;
        }
        boolean removed;
        try {
            removed = facade.removeStoredUri();
        } catch (RuntimeException exception) {
            return false;
        }
        if (!removed) {
            return false;
        }
        releaseToleratingRevocation(facade, Uri.parse(stored));
        return true;
    }

    private static void releaseToleratingRevocation(GrantFacade facade, Uri uri) {
        try {
            facade.release(uri, REQUIRED_FLAGS);
        } catch (SecurityException ignored) {
            // The grant may already have been revoked by the operating system.
        }
    }

    private static SharedPreferences preferences(Context context) {
        return context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
    }

    private static final class AndroidGrantFacade implements GrantFacade {
        private final Context context;

        private AndroidGrantFacade(Context context) {
            this.context = context;
        }

        @Override
        public String storedUri() {
            return persistedUri(context);
        }

        @Override
        public boolean storeUri(String uri) {
            return preferences(context).edit().putString(URI_KEY, uri).commit();
        }

        @Override
        public boolean removeStoredUri() {
            return preferences(context).edit().remove(URI_KEY).commit();
        }

        @Override
        public void take(Uri uri, int flags) throws SecurityException {
            context.getContentResolver().takePersistableUriPermission(uri, flags);
        }

        @Override
        public void release(Uri uri, int flags) throws SecurityException {
            context.getContentResolver().releasePersistableUriPermission(uri, flags);
        }
    }
}
