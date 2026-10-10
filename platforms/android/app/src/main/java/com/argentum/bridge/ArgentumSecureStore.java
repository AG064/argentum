package com.argentum.bridge;

import android.content.Context;
import android.content.SharedPreferences;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;

import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.KeyStore;
import java.util.Arrays;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

final class ArgentumSecureStore {
    static final int MAX_SECRET_BYTES = 16 * 1024;
    static final byte PAYLOAD_VERSION = 1;
    static final String KEY_ALIAS = "argentum.provider-credentials.v1";

    private static final String FAILURE = "Android secure storage operation failed";
    private static final String PREFERENCES = "argentum.secure-store.v1";
    private static final int GCM_TAG_BITS = 128;
    private static final int GCM_IV_BYTES = 12;
    private static final int MAX_CIPHERTEXT_BYTES = MAX_SECRET_BYTES + GCM_TAG_BITS / 8;
    private static final int MAX_PAYLOAD_BYTES = 1 + 1 + 4 + GCM_IV_BYTES + MAX_CIPHERTEXT_BYTES;
    private static final int MAX_ENCODED_BYTES = ((MAX_PAYLOAD_BYTES + 2) / 3) * 4;

    private ArgentumSecureStore() {}

    static byte[] get(Context context, String key) {
        validateKey(key);
        byte[] encoded = null;
        byte[] payload = null;
        byte[] plaintext = null;
        try {
            String stored = preferences(context).getString(key, null);
            if (stored == null) {
                return null;
            }
            if (stored.length() == 0 || stored.length() > MAX_ENCODED_BYTES) {
                throw failure();
            }
            encoded = stored.getBytes(StandardCharsets.US_ASCII);
            payload = Base64.decode(encoded, Base64.NO_WRAP);
            if (payload.length == 0 || payload.length > MAX_PAYLOAD_BYTES) {
                throw failure();
            }
            plaintext = decrypt(key, payload);
            validatePlaintext(plaintext);
            byte[] result = plaintext;
            plaintext = null;
            return result;
        } catch (Exception exception) {
            throw failure();
        } finally {
            clear(encoded);
            clear(payload);
            clear(plaintext);
        }
    }

    static void set(Context context, String key, byte[] value) {
        byte[] payload = null;
        byte[] encoded = null;
        try {
            validateKey(key);
            validatePlaintext(value);
            payload = encrypt(key, value);
            encoded = Base64.encode(payload, Base64.NO_WRAP);
            String stored = new String(encoded, StandardCharsets.US_ASCII);
            if (!preferences(context).edit().putString(key, stored).commit()) {
                throw failure();
            }
        } catch (Exception exception) {
            throw failure();
        } finally {
            clear(value);
            clear(payload);
            clear(encoded);
        }
    }

    static void delete(Context context, String key) {
        validateKey(key);
        try {
            if (!preferences(context).edit().remove(key).commit()) {
                throw failure();
            }
        } catch (Exception exception) {
            throw failure();
        }
    }

    private static void validateKey(String key) {
        if (!("provider/openai".equals(key)
                || "provider/minimax".equals(key)
                || "provider/deepseek".equals(key))) {
            throw failure();
        }
    }

    private static void validatePlaintext(byte[] value) {
        if (value == null || value.length == 0 || value.length > MAX_SECRET_BYTES) {
            throw failure();
        }
        int offset = 0;
        while (offset < value.length) {
            int first = Byte.toUnsignedInt(value[offset]);
            int codePoint;
            int width;
            if (first <= 0x7f) {
                codePoint = first;
                width = 1;
            } else if (first >= 0xc2 && first <= 0xdf) {
                width = 2;
                requireContinuation(value, offset, width);
                codePoint = ((first & 0x1f) << 6)
                        | (Byte.toUnsignedInt(value[offset + 1]) & 0x3f);
            } else if (first >= 0xe0 && first <= 0xef) {
                width = 3;
                requireContinuation(value, offset, width);
                int second = Byte.toUnsignedInt(value[offset + 1]);
                if ((first == 0xe0 && second < 0xa0) || (first == 0xed && second >= 0xa0)) {
                    throw failure();
                }
                codePoint = ((first & 0x0f) << 12)
                        | ((second & 0x3f) << 6)
                        | (Byte.toUnsignedInt(value[offset + 2]) & 0x3f);
            } else if (first >= 0xf0 && first <= 0xf4) {
                width = 4;
                requireContinuation(value, offset, width);
                int second = Byte.toUnsignedInt(value[offset + 1]);
                if ((first == 0xf0 && second < 0x90) || (first == 0xf4 && second >= 0x90)) {
                    throw failure();
                }
                codePoint = ((first & 0x07) << 18)
                        | ((second & 0x3f) << 12)
                        | ((Byte.toUnsignedInt(value[offset + 2]) & 0x3f) << 6)
                        | (Byte.toUnsignedInt(value[offset + 3]) & 0x3f);
            } else {
                throw failure();
            }
            if (Character.getType(codePoint) == Character.CONTROL) {
                throw failure();
            }
            offset += width;
        }
    }

    private static void requireContinuation(byte[] value, int offset, int width) {
        if (offset > value.length - width) {
            throw failure();
        }
        for (int index = 1; index < width; index += 1) {
            int next = Byte.toUnsignedInt(value[offset + index]);
            if ((next & 0xc0) != 0x80) {
                throw failure();
            }
        }
    }

    private static SharedPreferences preferences(Context context) {
        return context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE);
    }

    private static SecretKey loadOrCreateKey() throws Exception {
        KeyStore keyStore = KeyStore.getInstance("AndroidKeyStore");
        keyStore.load(null);
        SecretKey existing = (SecretKey) keyStore.getKey(KEY_ALIAS, null);
        if (existing != null) {
            return existing;
        }
        KeyGenerator generator = KeyGenerator.getInstance(
                KeyProperties.KEY_ALGORITHM_AES,
                "AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build());
        return generator.generateKey();
    }

    private static byte[] encrypt(String key, byte[] plaintext) throws Exception {
        if (plaintext.length > MAX_SECRET_BYTES) {
            throw failure();
        }
        byte[] aad = key.getBytes(StandardCharsets.UTF_8);
        byte[] ciphertext = null;
        byte[] iv = null;
        try {
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.ENCRYPT_MODE, loadOrCreateKey());
            cipher.updateAAD(aad);
            ciphertext = cipher.doFinal(plaintext);
            iv = cipher.getIV();
            return encodePayload(iv, ciphertext);
        } finally {
            clear(aad);
            clear(ciphertext);
            clear(iv);
        }
    }

    private static byte[] decrypt(String key, byte[] payload) throws Exception {
        DecodedPayload decoded = decodePayload(payload);
        byte[] aad = key.getBytes(StandardCharsets.UTF_8);
        try {
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(
                    Cipher.DECRYPT_MODE,
                    loadOrCreateKey(),
                    new GCMParameterSpec(GCM_TAG_BITS, decoded.iv));
            cipher.updateAAD(aad);
            byte[] plaintext = cipher.doFinal(decoded.ciphertext);
            if (plaintext.length > MAX_SECRET_BYTES) {
                clear(plaintext);
                throw failure();
            }
            return plaintext;
        } finally {
            clear(aad);
            decoded.clear();
        }
    }

    private static byte[] encodePayload(byte[] iv, byte[] ciphertext) {
        if (iv == null || iv.length != GCM_IV_BYTES
                || ciphertext == null || ciphertext.length > MAX_CIPHERTEXT_BYTES) {
            throw failure();
        }
        int total = 1 + 1 + 4 + iv.length + ciphertext.length;
        if (total > MAX_PAYLOAD_BYTES) {
            throw failure();
        }
        return ByteBuffer.allocate(total)
                .put(PAYLOAD_VERSION)
                .put((byte) iv.length)
                .putInt(ciphertext.length)
                .put(iv)
                .put(ciphertext)
                .array();
    }

    private static DecodedPayload decodePayload(byte[] payload) {
        if (payload == null || payload.length < 1 + 1 + 4 + GCM_IV_BYTES + GCM_TAG_BITS / 8
                || payload.length > MAX_PAYLOAD_BYTES) {
            throw failure();
        }
        ByteBuffer buffer = ByteBuffer.wrap(payload);
        int version = Byte.toUnsignedInt(buffer.get());
        int ivLength = Byte.toUnsignedInt(buffer.get());
        int ciphertextLength = buffer.getInt();
        if (version != PAYLOAD_VERSION
                || ivLength != GCM_IV_BYTES
                || ciphertextLength < GCM_TAG_BITS / 8
                || ciphertextLength > MAX_CIPHERTEXT_BYTES
                || buffer.remaining() != ivLength + ciphertextLength) {
            throw failure();
        }
        byte[] iv = new byte[ivLength];
        byte[] ciphertext = new byte[ciphertextLength];
        buffer.get(iv);
        buffer.get(ciphertext);
        return new DecodedPayload(iv, ciphertext);
    }

    private static IllegalStateException failure() {
        return new IllegalStateException(FAILURE);
    }

    private static void clear(byte[] value) {
        if (value != null) {
            Arrays.fill(value, (byte) 0);
        }
    }

    private static final class DecodedPayload {
        private final byte[] iv;
        private final byte[] ciphertext;

        private DecodedPayload(byte[] iv, byte[] ciphertext) {
            this.iv = iv;
            this.ciphertext = ciphertext;
        }

        private void clear() {
            ArgentumSecureStore.clear(iv);
            ArgentumSecureStore.clear(ciphertext);
        }
    }
}
