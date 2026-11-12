package com.argentum.bridge;

import android.app.Activity;
import android.app.Instrumentation;
import android.os.Bundle;
import android.os.Process;

import java.io.BufferedReader;
import java.io.FileReader;
import java.util.HashSet;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public final class ArgentumInferenceInstrumentation extends Instrumentation {
    private static final Pattern EVIDENCE =
            Pattern.compile("^ARGENTUM_M3 ([a-z][a-z0-9_]*)=(.*)$");
    private static final Pattern SAFE_VALUE = Pattern.compile("^[A-Za-z0-9_.+,: -]{1,256}$");
    private Bundle arguments;

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = new Bundle(arguments);
        start();
    }

    @Override
    public void onStart() {
        Bundle result = new Bundle();
        int code = Activity.RESULT_CANCELED;
        try {
            String suite = requiredArgument("argentum_suite");
            String deviceClass = requiredArgument("argentum_device_class");
            String apkSha256 = requiredHash("argentum_apk_sha256");
            String modelSha256 = requiredHash("argentum_model_sha256");
            String phase = requiredArgument("argentum_phase");
            if (!"milestone3_inference".equals(suite)) {
                throw new EvidenceFailure("suite");
            }
            if (!"emulator".equals(deviceClass) && !"physical-arm64".equals(deviceClass)) {
                throw new EvidenceFailure("device-class");
            }
            if (!"primary".equals(phase) && !"restart".equals(phase)) {
                throw new EvidenceFailure("phase");
            }
            String nativeEvidence = ArgentumInferenceTestHook.runNative(
                    getTargetContext().getFilesDir().getAbsolutePath(),
                    deviceClass,
                    apkSha256,
                    modelSha256,
                    phase);
            collectEvidence(result, nativeEvidence);
            result.putString("ARGENTUM_M3_process_identity", processIdentity());
            if (!"pass".equals(result.getString("ARGENTUM_M3_status"))) {
                throw new EvidenceFailure("native-result");
            }
            code = Activity.RESULT_OK;
        } catch (EvidenceFailure failure) {
            if (!result.containsKey("ARGENTUM_M3_protocol")) {
                result.putString("ARGENTUM_M3_protocol", "1");
            }
            result.putString("ARGENTUM_M3_status", "fail");
            if (!result.containsKey("ARGENTUM_M3_failure_stage")) {
                result.putString("ARGENTUM_M3_failure_stage", failure.stage);
            }
        } catch (Throwable ignored) {
            result.putString("ARGENTUM_M3_protocol", "1");
            result.putString("ARGENTUM_M3_status", "fail");
            result.putString("ARGENTUM_M3_failure_stage", "instrumentation");
        }
        finish(code, result);
    }

    private String requiredArgument(String name) throws EvidenceFailure {
        String value = arguments.getString(name);
        if (value == null || !SAFE_VALUE.matcher(value).matches()) {
            throw new EvidenceFailure("argument");
        }
        return value;
    }

    private String requiredHash(String name) throws EvidenceFailure {
        String value = requiredArgument(name);
        if (!value.matches("(?i)^[0-9a-f]{64}$")) {
            throw new EvidenceFailure("artifact-hash");
        }
        return value.toUpperCase(java.util.Locale.ROOT);
    }

    private static void collectEvidence(Bundle result, String text) throws EvidenceFailure {
        if (text == null || text.length() > 8192) {
            throw new EvidenceFailure("native-output");
        }
        Set<String> keys = new HashSet<>();
        for (String line : text.split("\\n", -1)) {
            Matcher matcher = EVIDENCE.matcher(line);
            if (!matcher.matches() || !SAFE_VALUE.matcher(matcher.group(2)).matches()) {
                throw new EvidenceFailure("native-output");
            }
            String key = matcher.group(1);
            if (!keys.add(key)) {
                throw new EvidenceFailure("duplicate-key");
            }
            result.putString("ARGENTUM_M3_" + key, matcher.group(2));
        }
    }

    private static String processIdentity() throws EvidenceFailure {
        try (BufferedReader reader = new BufferedReader(new FileReader("/proc/self/stat"))) {
            String stat = reader.readLine();
            int commandEnd = stat.lastIndexOf(')');
            String[] fieldsAfterCommand = stat.substring(commandEnd + 2).split(" ");
            String startTicks = fieldsAfterCommand[19];
            return Process.myPid() + ":" + startTicks;
        } catch (Exception ignored) {
            throw new EvidenceFailure("process-identity");
        }
    }

    private static final class EvidenceFailure extends Exception {
        private final String stage;

        private EvidenceFailure(String stage) {
            this.stage = stage;
        }
    }
}
