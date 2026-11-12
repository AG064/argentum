package com.argentum.bridge;

interface IArgentumHostService {
    int protocolVersion();
    String status();
    long activeGeneration();
    boolean stopGeneration(long generation);
}
