package com.argentum.bridge;

final class ArgentumInferenceTestHook {
    static {
        System.loadLibrary("main");
    }

    private ArgentumInferenceTestHook() {}

    static native String runNative(
            String filesDir,
            String deviceClass,
            String apkSha256,
            String modelSha256,
            String phase);
}
