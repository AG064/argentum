plugins {
    id("com.android.application")
}

val rustDebugJniDir = layout.buildDirectory.dir("generated/rustJniLibs/debug")
val rustEvidenceJniDir = layout.buildDirectory.dir("generated/rustJniLibs/evidence")
val rustReleaseJniDir = layout.buildDirectory.dir("generated/rustJniLibs/release")
val windowsHostToolchain = rootProject.projectDir.resolve("cmake/windows-host-toolchain.cmake")
val canonicalBrandIcon = rootProject.projectDir.resolve("../../assets/brand/argentum-512.png")
val mainBrandResDir = layout.buildDirectory.dir("generated/brandRes/main")
val androidTestBrandResDir = layout.buildDirectory.dir("generated/brandRes/androidTest")

val syncMainBrandIcon by tasks.registering(Sync::class) {
    from(canonicalBrandIcon)
    into(mainBrandResDir.map { it.dir("drawable") })
    rename { "ic_argentum.png" }
}

val syncAndroidTestBrandIcon by tasks.registering(Sync::class) {
    from(canonicalBrandIcon)
    into(androidTestBrandResDir.map { it.dir("drawable") })
    rename { "ic_argentum.png" }
}

val buildRustDebug by tasks.registering(Exec::class) {
    workingDir(rootProject.projectDir.resolve("../.."))
    environment("CARGO_BUILD_JOBS", "1")
    val androidHome = providers.environmentVariable("ANDROID_HOME").get()
    environment("ANDROID_JAR", file("$androidHome/platforms/android-36/android.jar"))
    if (System.getProperty("os.name").startsWith("Windows")) {
        environment("GGML_VULKAN_SHADERS_GEN_TOOLCHAIN", windowsHostToolchain.absolutePath)
    }
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-t", "x86_64",
        "-P", "28",
        "-o", rustDebugJniDir.get().asFile.absolutePath,
        "build", "-p", "argentum-android", "--lib", "--locked"
    )
}

val buildRustEvidence by tasks.registering(Exec::class) {
    workingDir(rootProject.projectDir.resolve("../.."))
    environment("CARGO_BUILD_JOBS", "1")
    val androidHome = providers.environmentVariable("ANDROID_HOME").get()
    environment("ANDROID_JAR", file("$androidHome/platforms/android-36/android.jar"))
    if (System.getProperty("os.name").startsWith("Windows")) {
        environment("GGML_VULKAN_SHADERS_GEN_TOOLCHAIN", windowsHostToolchain.absolutePath)
    }
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-t", "x86_64",
        "-P", "28",
        "-o", rustEvidenceJniDir.get().asFile.absolutePath,
        "build", "-p", "argentum-android", "--lib", "--locked",
        "--features", "android-inference-test-hook"
    )
}

val buildRustRelease by tasks.registering(Exec::class) {
    workingDir(rootProject.projectDir.resolve("../.."))
    environment("CARGO_BUILD_JOBS", "1")
    val androidHome = providers.environmentVariable("ANDROID_HOME").get()
    environment("ANDROID_JAR", file("$androidHome/platforms/android-36/android.jar"))
    if (System.getProperty("os.name").startsWith("Windows")) {
        environment("GGML_VULKAN_SHADERS_GEN_TOOLCHAIN", windowsHostToolchain.absolutePath)
    }
    commandLine(
        "cargo", "ndk", "-t", "arm64-v8a", "-t", "x86_64",
        "-P", "28",
        "-o", rustReleaseJniDir.get().asFile.absolutePath,
        "build", "-p", "argentum-android", "--lib", "--release", "--locked"
    )
}

android {
    namespace = "com.argentum"
    compileSdk = 36
    ndkVersion = "27.0.12077973"

    defaultConfig {
        applicationId = "com.argentum"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        testApplicationId = "com.argentum.test"
        testInstrumentationRunner = "com.argentum.bridge.ArgentumInferenceInstrumentation"
    }

    buildTypes {
        create("evidence") {
            initWith(getByName("debug"))
            isDebuggable = true
            matchingFallbacks += listOf("debug")
        }
    }
    testBuildType = "evidence"

    sourceSets.getByName("debug").jniLibs.directories.add(rustDebugJniDir.get().asFile.absolutePath)
    sourceSets.getByName("evidence").jniLibs.directories.add(rustEvidenceJniDir.get().asFile.absolutePath)
    sourceSets.getByName("release").jniLibs.directories.add(rustReleaseJniDir.get().asFile.absolutePath)
    sourceSets.getByName("main").res.directories.add(mainBrandResDir.get().asFile.absolutePath)
    sourceSets.getByName("androidTest").res.directories.add(androidTestBrandResDir.get().asFile.absolutePath)

    buildFeatures {
        aidl = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    java {
        toolchain {
            languageVersion = JavaLanguageVersion.of(17)
        }
    }
}

tasks.matching {
    it.name == "preDebugBuild"
        || it.name == "mergeDebugJniLibFolders"
        || it.name == "mergeDebugNativeLibs"
}.configureEach {
    dependsOn(buildRustDebug)
}

tasks.matching {
    it.name == "preReleaseBuild"
        || it.name == "mergeReleaseJniLibFolders"
        || it.name == "mergeReleaseNativeLibs"
}.configureEach {
    dependsOn(buildRustRelease)
}

tasks.matching {
    it.name == "preEvidenceBuild"
        || it.name == "mergeEvidenceJniLibFolders"
        || it.name == "mergeEvidenceNativeLibs"
}.configureEach {
    dependsOn(buildRustEvidence)
}

tasks.matching {
    it.name == "preBuild" || it.name.contains("Resources")
}.configureEach {
    dependsOn(syncMainBrandIcon, syncAndroidTestBrandIcon)
}
