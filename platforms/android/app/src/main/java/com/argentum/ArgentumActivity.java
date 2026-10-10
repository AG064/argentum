package com.argentum;

import android.app.NativeActivity;
import android.content.Intent;
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

    public void argentumClearWorkspaceUri() {
        platformBridge.clearWorkspaceUri();
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        if (!platformBridge.onActivityResult(requestCode, resultCode, data)) {
            super.onActivityResult(requestCode, resultCode, data);
        }
    }
}
