// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

package com.argentum.uitest;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.graphics.Bitmap;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.KeyEvent;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Predicate;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import com.argentum.ArgentumActivity;

/**
 * Emulator UI tests for the debug APK. Keys reach the app through Android's
 * real input path. The app's debug-only test bridge reports what is on screen
 * in files/ui-test/state.json and runs a few button callbacks from
 * files/ui-test/commands, so no test taps at screen coordinates.
 *
 * Run with: am instrument -w -r -e fake_endpoint http://127.0.0.1:PORT/v1/
 *   com.argentum.test/com.argentum.uitest.ArgentumUiInstrumentation
 */
public final class ArgentumUiInstrumentation extends Instrumentation {
    private static final long START_TIMEOUT_MS = 120_000;
    private static final long STEP_TIMEOUT_MS = 20_000;
    private static final long REPLY_TIMEOUT_MS = 120_000;
    private static final String MESSAGE = "Hello from the Argentum emulator test";
    private static final Pattern ENDPOINT = Pattern.compile("^http://127\\.0\\.0\\.1:[0-9]{2,5}/v1/$");

    private Bundle arguments;
    private File bridge;
    private File shots;
    private final Map<String, String> results = new LinkedHashMap<>();

    @Override
    public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        this.arguments = new Bundle(arguments);
        start();
    }

    @Override
    public void onStart() {
        Bundle report = new Bundle();
        int code = Activity.RESULT_CANCELED;
        Activity activity = null;
        try {
            String endpoint = arguments.getString("fake_endpoint", "");
            if (!ENDPOINT.matcher(endpoint).matches()) {
                throw new TestFailure("setup", "fake_endpoint must be http://127.0.0.1:PORT/v1/");
            }
            prepareBridge();
            activity = launch();
            openTest();
            firstRun(endpoint);
            settingsTest();
            backTest(activity);
            messageTest();
            code = results.containsValue("fail") ? Activity.RESULT_CANCELED : Activity.RESULT_OK;
        } catch (TestFailure failure) {
            results.put(failure.test, "fail");
            report.putString("ARGENTUM_UI_failure", failure.test + ": " + failure.getMessage());
        } catch (Throwable error) {
            results.put("instrumentation", "fail");
            report.putString("ARGENTUM_UI_failure", "instrumentation: " + error);
        }
        for (Map.Entry<String, String> entry : results.entrySet()) {
            report.putString("ARGENTUM_UI_" + entry.getKey(), entry.getValue());
        }
        report.putString("ARGENTUM_UI_state", readState());
        finish(code, report);
    }

    private void prepareBridge() throws IOException {
        File files = getTargetContext().getFilesDir();
        bridge = new File(files, "ui-test");
        if (!bridge.isDirectory() && !bridge.mkdirs()) {
            throw new IOException("cannot create " + bridge);
        }
        new File(bridge, "state.json").delete();
        new File(bridge, "commands").delete();
        Files.write(new File(bridge, "enabled").toPath(), new byte[0]);
        File external = getTargetContext().getExternalFilesDir(null);
        shots = new File(external, "ui-test-shots");
        if (!shots.isDirectory() && !shots.mkdirs()) {
            throw new IOException("cannot create " + shots);
        }
    }

    private Activity launch() {
        Intent intent = new Intent(Intent.ACTION_MAIN);
        intent.addCategory(Intent.CATEGORY_LAUNCHER);
        intent.setClassName(getTargetContext(), ArgentumActivity.class.getName());
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TASK);
        return startActivitySync(intent);
    }

    /** The app opens and draws its interface. */
    private void openTest() throws Exception {
        waitFor("open", START_TIMEOUT_MS, state -> state.contains("\"first_run_open\""));
        SystemClock.sleep(1500);
        screenshot("01-open");
        results.put("open", "pass");
    }

    /** First run: the server page's Continue, then Start. */
    private void firstRun(String endpoint) throws Exception {
        String state = readState();
        if (!state.contains("\"first_run_open\":true")) {
            results.put("first_run", "skipped");
            return;
        }
        command("first_run_server\t" + endpoint + "\tfixture\t32768");
        waitFor("first_run", STEP_TIMEOUT_MS, s -> !s.contains("\"first_run_stage\":\"provider\"")
                || s.contains("\"first_run_error\":\"") && !s.contains("\"first_run_error\":\"\""));
        if (!readState().contains("\"first_run_error\":\"\"")) {
            throw new TestFailure("first_run", "the server was refused: " + readState());
        }
        screenshot("02-first-run");
        command("first_run_finish");
        waitFor("first_run", STEP_TIMEOUT_MS, s -> s.contains("\"first_run_open\":false"));
        results.put("first_run", "pass");
    }

    /** Ctrl+, opens Settings, through the hardware keyboard path. */
    private void settingsTest() throws Exception {
        sendChord(KeyEvent.KEYCODE_COMMA, KeyEvent.META_CTRL_ON | KeyEvent.META_CTRL_LEFT_ON);
        waitFor("settings", STEP_TIMEOUT_MS, s -> s.contains("\"settings_open\":true"));
        SystemClock.sleep(800);
        screenshot("03-settings");
        results.put("settings", "pass");
    }

    /** Back closes Settings, then the drawer, and the app stays open. */
    private void backTest(Activity activity) throws Exception {
        sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);
        waitFor("back", STEP_TIMEOUT_MS, s -> s.contains("\"settings_open\":false"));
        sendChord(KeyEvent.KEYCODE_B, KeyEvent.META_CTRL_ON | KeyEvent.META_CTRL_LEFT_ON);
        waitFor("back", STEP_TIMEOUT_MS, s -> s.contains("\"drawer_open\":true"));
        screenshot("04-drawer");
        sendKeyDownUpSync(KeyEvent.KEYCODE_BACK);
        waitFor("back", STEP_TIMEOUT_MS, s -> s.contains("\"drawer_open\":false")
                && s.contains("\"back_layer_open\":false"));
        SystemClock.sleep(1000);
        if (activity.isFinishing() || activity.isDestroyed()) {
            throw new TestFailure("back", "Back left the app while a panel was open");
        }
        screenshot("05-after-back");
        results.put("back", "pass");
    }

    /** A message reaches the local fake server and its reply is shown. */
    private void messageTest() throws Exception {
        int before = messageCount(readState());
        command("submit\t" + MESSAGE);
        waitFor("message", REPLY_TIMEOUT_MS, s -> messageCount(s) >= before + 2
                && s.contains("\"running\":false")
                && s.contains("Fixture review from the local test provider"));
        SystemClock.sleep(800);
        screenshot("06-reply");
        results.put("message", "pass");
    }

    private void sendChord(int keyCode, int meta) {
        long now = SystemClock.uptimeMillis();
        sendKeySync(new KeyEvent(now, now, KeyEvent.ACTION_DOWN, keyCode, 0, meta));
        sendKeySync(new KeyEvent(now, SystemClock.uptimeMillis(), KeyEvent.ACTION_UP, keyCode, 0, meta));
    }

    private void command(String line) throws IOException {
        File commands = new File(bridge, "commands");
        List<String> lines = new ArrayList<>();
        if (commands.isFile()) {
            lines.addAll(Files.readAllLines(commands.toPath(), StandardCharsets.UTF_8));
        }
        lines.add(line);
        File partial = new File(bridge, "commands.partial");
        Files.write(partial.toPath(), (String.join("\n", lines) + "\n").getBytes(StandardCharsets.UTF_8));
        if (!partial.renameTo(commands)) {
            throw new IOException("cannot write the command");
        }
    }

    private String readState() {
        try {
            return new String(Files.readAllBytes(new File(bridge, "state.json").toPath()),
                    StandardCharsets.UTF_8);
        } catch (IOException | RuntimeException missing) {
            return "";
        }
    }

    private void waitFor(String test, long timeoutMs, Predicate<String> condition)
            throws TestFailure {
        long deadline = SystemClock.uptimeMillis() + timeoutMs;
        while (SystemClock.uptimeMillis() < deadline) {
            if (condition.test(readState())) {
                return;
            }
            SystemClock.sleep(200);
        }
        screenshot("failed-" + test);
        throw new TestFailure(test, "timed out; state: " + readState());
    }

    private static int messageCount(String state) {
        Matcher matcher = Pattern.compile("\"message_count\":([0-9]+)").matcher(state);
        return matcher.find() ? Integer.parseInt(matcher.group(1)) : -1;
    }

    private void screenshot(String name) {
        try {
            Bitmap bitmap = getUiAutomation().takeScreenshot();
            if (bitmap == null) {
                return;
            }
            try (FileOutputStream out = new FileOutputStream(new File(shots, name + ".png"))) {
                bitmap.compress(Bitmap.CompressFormat.PNG, 100, out);
            }
        } catch (IOException | RuntimeException ignored) {
            // A missing screenshot does not change a result.
        }
    }

    private static final class TestFailure extends Exception {
        final String test;

        TestFailure(String test, String message) {
            super(message);
            this.test = test;
        }
    }
}
