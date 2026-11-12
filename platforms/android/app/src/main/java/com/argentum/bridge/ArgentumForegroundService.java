package com.argentum.bridge;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.IBinder;

import java.util.concurrent.atomic.AtomicLong;

public final class ArgentumForegroundService extends Service {
    private static final int PROTOCOL_VERSION = 1;
    private static final int NOTIFICATION_ID = 4108;
    private static final String CHANNEL_ID = "argentum-host";
    private static final String ACTION_START =
            "com.argentum.bridge.action.START_LOCAL_INFERENCE";
    private static final String ACTION_STOP =
            "com.argentum.bridge.action.STOP_LOCAL_INFERENCE";
    private static final String EXTRA_GENERATION = "lifecycle_generation";

    private final AtomicLong activeGeneration = new AtomicLong();

    public static boolean requestStart(Context context, long generation) {
        if (generation <= 0L) {
            return false;
        }
        Intent intent = new Intent(context, ArgentumForegroundService.class)
                .setAction(ACTION_START)
                .putExtra(EXTRA_GENERATION, generation);
        try {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent);
            } else {
                context.startService(intent);
            }
            return true;
        } catch (RuntimeException exception) {
            return false;
        }
    }

    public static boolean requestStop(Context context, long generation) {
        if (generation <= 0L) {
            return false;
        }
        Intent intent = new Intent(context, ArgentumForegroundService.class)
                .setAction(ACTION_STOP)
                .putExtra(EXTRA_GENERATION, generation);
        try {
            context.startService(intent);
            return true;
        } catch (RuntimeException exception) {
            return false;
        }
    }

    private final IArgentumHostService.Stub binder = new IArgentumHostService.Stub() {
        @Override
        public int protocolVersion() {
            return PROTOCOL_VERSION;
        }

        @Override
        public String status() {
            return activeGeneration.get() == 0L ? "unavailable" : "active";
        }

        @Override
        public long activeGeneration() {
            return activeGeneration.get();
        }

        @Override
        public boolean stopGeneration(long generation) {
            return stopExactGeneration(generation, 0);
        }
    };

    @Override
    public void onCreate() {
        super.onCreate();
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID,
                getString(com.argentum.R.string.host_channel_name),
                NotificationManager.IMPORTANCE_LOW);
        getSystemService(NotificationManager.class).createNotificationChannel(channel);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent == null) {
            return START_NOT_STICKY;
        }
        long generation = intent.getLongExtra(EXTRA_GENERATION, 0L);
        String action = intent.getAction();
        if (ACTION_STOP.equals(action)) {
            stopExactGeneration(generation, startId);
            return START_NOT_STICKY;
        }
        if (!ACTION_START.equals(action) || generation <= 0L) {
            if (activeGeneration.get() == 0L) {
                stopSelf(startId);
            }
            return START_NOT_STICKY;
        }
        long current = activeGeneration.get();
        if (current != 0L && current != generation) {
            return START_NOT_STICKY;
        }
        activeGeneration.set(generation);
        Notification notification = new Notification.Builder(this, CHANNEL_ID)
                .setContentTitle(getString(com.argentum.R.string.app_name))
                .setContentText(getString(com.argentum.R.string.inference_active))
                .setSmallIcon(com.argentum.R.drawable.ic_argentum)
                .build();
        startForeground(NOTIFICATION_ID, notification);
        return START_NOT_STICKY;
    }

    @Override
    public void onDestroy() {
        activeGeneration.set(0L);
        super.onDestroy();
    }

    @Override
    public IBinder onBind(Intent intent) {
        return binder;
    }

    private boolean stopExactGeneration(long generation, int startId) {
        if (generation <= 0L
                || !activeGeneration.compareAndSet(generation, 0L)) {
            return false;
        }
        stopForeground(STOP_FOREGROUND_REMOVE);
        if (startId == 0) {
            stopSelf();
        } else {
            stopSelf(startId);
        }
        return true;
    }
}
