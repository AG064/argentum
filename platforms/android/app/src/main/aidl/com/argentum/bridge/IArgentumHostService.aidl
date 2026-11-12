// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

package com.argentum.bridge;

interface IArgentumHostService {
    int protocolVersion();
    String status();
    long activeGeneration();
    boolean stopGeneration(long generation);
}
