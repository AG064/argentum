// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

package com.argentum.bridge;

import android.app.Activity;
import android.content.Intent;

public final class ArgentumPlatformBridge {
    private final Activity activity;

    public ArgentumPlatformBridge(Activity activity) {
        this.activity = activity;
    }

    public byte[] secretGet(String key) {
        return ArgentumSecureStore.get(activity, key);
    }

    public void secretSet(String key, byte[] value) {
        ArgentumSecureStore.set(activity, key, value);
    }

    public void secretDelete(String key) {
        ArgentumSecureStore.delete(activity, key);
    }

    public void requestWorkspaceTree() {
        ArgentumStorageBridge.requestTree(activity);
    }

    public String persistedWorkspaceUri() {
        return ArgentumStorageBridge.persistedUri(activity);
    }

    public void clearWorkspaceUri() {
        ArgentumStorageBridge.clearPersistedUri(activity);
    }

    public boolean onActivityResult(int requestCode, int resultCode, Intent data) {
        return ArgentumStorageBridge.onActivityResult(activity, requestCode, resultCode, data);
    }
}
