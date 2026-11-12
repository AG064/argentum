// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

package com.argentum;

import android.app.NativeActivity;
import android.content.Intent;
import android.net.Uri;
import android.graphics.Insets;
import android.os.Build;
import android.os.Bundle;
import android.view.WindowInsets;

import com.argentum.bridge.ArgentumPlatformBridge;
import com.argentum.bridge.ArgentumForegroundService;

import java.util.concurrent.atomic.AtomicLong;

public final class ArgentumActivity extends NativeActivity {
    static {
        System.loadLibrary("main");
    }

    private static final int LIFECYCLE_PROTOCOL_VERSION = 2;
    private static final int STATE_CREATED = 1;
    private static final int STATE_STARTED = 2;
    private static final int STATE_RESUMED = 3;
    private static final int STATE_PAUSED = 4;
    private static final int STATE_STOPPED = 5;
    private static final int STATE_DESTROYED = 6;
    private static final AtomicLong NEXT_ACTIVITY_GENERATION = new AtomicLong();

    private final long activityGeneration = NEXT_ACTIVITY_GENERATION.incrementAndGet();
    private final AtomicLong lifecycleSequence = new AtomicLong();
    private ArgentumPlatformBridge platformBridge;

    private native void nativeOnLifecycle(
            int state, long activityGeneration, long sequence, int protocolVersion);
    private native boolean nativeOnBackRequested(
            long activityGeneration, long sequence, int protocolVersion);

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        platformBridge = new ArgentumPlatformBridge(this);
        super.onCreate(savedInstanceState);
        publishLifecycle(STATE_CREATED);
    }

    @Override
    protected void onStart() {
        super.onStart();
        publishLifecycle(STATE_STARTED);
    }

    @Override
    protected void onResume() {
        super.onResume();
        publishLifecycle(STATE_RESUMED);
    }

    @Override
    protected void onPause() {
        publishLifecycle(STATE_PAUSED);
        super.onPause();
    }

    @Override
    protected void onStop() {
        publishLifecycle(STATE_STOPPED);
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        publishLifecycle(STATE_DESTROYED);
        super.onDestroy();
    }

    @Override
    public void onBackPressed() {
        if (!nativeOnBackRequested(
                activityGeneration, nextSequence(), LIFECYCLE_PROTOCOL_VERSION)) {
            super.onBackPressed();
        }
    }

    private void publishLifecycle(int state) {
        nativeOnLifecycle(
                state, activityGeneration, nextSequence(), LIFECYCLE_PROTOCOL_VERSION);
    }

    private long nextSequence() {
        return lifecycleSequence.incrementAndGet();
    }

    public long argentumActivityGeneration() {
        return activityGeneration;
    }

    public int[] argentumWindowInsets() {
        WindowInsets windowInsets = getWindow().getDecorView().getRootWindowInsets();
        if (windowInsets == null) {
            return null;
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            Insets systemBars = windowInsets.getInsetsIgnoringVisibility(
                    WindowInsets.Type.systemBars());
            Insets ime = windowInsets.getInsets(WindowInsets.Type.ime());
            return new int[] {
                Math.max(0, systemBars.top),
                Math.max(0, systemBars.bottom),
                Math.max(0, ime.bottom - systemBars.bottom)
            };
        }
        int systemBarBottom = Math.max(0, windowInsets.getStableInsetBottom());
        return new int[] {
            Math.max(0, windowInsets.getStableInsetTop()),
            systemBarBottom,
            Math.max(0, windowInsets.getSystemWindowInsetBottom() - systemBarBottom)
        };
    }

    public boolean argentumStartInferenceForeground(long generation) {
        return ArgentumForegroundService.requestStart(this, generation);
    }

    public boolean argentumStopInferenceForeground(long generation) {
        return ArgentumForegroundService.requestStop(this, generation);
    }

    public byte[] argentumSecretGet(String key) {
        return platformBridge.secretGet(key);
    }

    public void argentumSecretSet(String key, byte[] value) {
        platformBridge.secretSet(key, value);
    }

    public void argentumSecretDelete(String key) {
        platformBridge.secretDelete(key);
    }

    public void argentumRequestWorkspaceTree() {
        platformBridge.requestWorkspaceTree();
    }

    public String argentumPersistedWorkspaceUri() {
        return platformBridge.persistedWorkspaceUri();
    }

    /** Puts text on the clipboard. Android shows its own confirmation. */
    public boolean argentumCopyText(String text) {
        android.content.ClipboardManager clipboard =
                (android.content.ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
        if (clipboard == null || text == null) {
            return false;
        }
        clipboard.setPrimaryClip(android.content.ClipData.newPlainText("Argentum", text));
        return true;
    }

    /** Opens the Argentum issue page. Any other address is refused. */
    public boolean argentumOpenIssuePage(String url) {
        if (url == null || !url.startsWith("https://github.com/AG064/argentum/issues/new")) {
            return false;
        }
        try {
            startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            return true;
        } catch (RuntimeException exception) {
            return false;
        }
    }

    /**
     * Writes a session export to the app cache and opens the share sheet.
     * The owner picks where it goes; nothing is sent before that.
     */
    public boolean argentumShareSessionExport(String name, String json) {
        return com.argentum.bridge.ArgentumSessionShare.share(this, name, json);
    }

    /** 0: no All files access setting on this Android, 1: off, 2: on. */
    public int argentumAllFilesAccess() {
        return com.argentum.bridge.ArgentumSharedStorage.allFilesAccess();
    }

    public boolean argentumOpenAllFilesSettings() {
        return com.argentum.bridge.ArgentumSharedStorage.openAllFilesSettings(this);
    }

    public String argentumPrimaryStorageRoot() {
        return com.argentum.bridge.ArgentumSharedStorage.primaryRoot();
    }

    public boolean argentumRequestFolderPath() {
        return com.argentum.bridge.ArgentumSharedStorage.requestFolder(this);
    }

    public String argentumTakePickedFolder() {
        return com.argentum.bridge.ArgentumSharedStorage.takePickedFolder();
    }

    public void argentumRestart() {
        com.argentum.bridge.ArgentumSharedStorage.restart(this);
    }

    public void argentumClearWorkspaceUri() {
        platformBridge.clearWorkspaceUri();
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        if (com.argentum.bridge.ArgentumSharedStorage.onActivityResult(
                requestCode, resultCode, data)) {
            return;
        }
        if (!platformBridge.onActivityResult(requestCode, resultCode, data)) {
            super.onActivityResult(requestCode, resultCode, data);
        }
    }
}
