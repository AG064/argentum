import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.argentum"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.argentum"
        minSdk = 26
        targetSdk = 34
        versionCode = 9
        versionName = "0.0.9"

        vectorDrawables {
            useSupportLibrary = true
        }

        // BuildConfig fields used by the in-app updater. We embed the current
        // version and the canonical releases URL so the updater doesn't need
        // a separate config file.
        buildConfigField("String", "GITHUB_RELEASES_URL", "\"https://api.github.com/repos/AG064/argentum/releases/latest\"")
        buildConfigField("String", "GITHUB_RELEASES_PAGE", "\"https://github.com/AG064/argentum/releases/latest\"")
    }

    signingConfigs {
        create("release") {
            // Published APKs use the same persistent release keystore.
            // See docs/ANDROID_BUILD.md for the signing flow and how to provide
            // a custom keystore via repository secrets.
            val keystorePropsFile = file("keystore.properties")
            if (keystorePropsFile.exists()) {
                val props = Properties().apply {
                    load(keystorePropsFile.inputStream())
                }
                storeFile = file(props.getProperty("storeFile", "keystore/release.keystore"))
                storePassword = props.getProperty("storePassword")
                keyAlias = props.getProperty("keyAlias")
                keyPassword = props.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            // The release signing config is only useful when a keystore is
            // present. Otherwise fall back to the debug keystore for local,
            // non-publishable test builds only.
            signingConfig = if (file("keystore.properties").exists()) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.8"
    }

    packaging {
        resources {
            excludes += "/META-INF/{AL2.0,LGPL2.1}"
        }
    }
}

dependencies {
    // Core Android
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.7.0")
    implementation("androidx.activity:activity-compose:1.8.2")

    // Compose BOM
    implementation(platform("androidx.compose:compose-bom:2024.01.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")

    // Navigation
    implementation("androidx.navigation:navigation-compose:2.7.6")

    // ViewModel
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.7.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.7.0")

    // Coroutines
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")

    // DataStore Preferences
    implementation("androidx.datastore:datastore-preferences:1.0.0")

    // OkHttp (no Retrofit as per requirements)
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    // Kotlinx Serialization
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.6.2")

    // Debug
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")

    // Testing
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.7.3")
    testImplementation("io.mockk:mockk:1.13.8")
    testRuntimeOnly("org.junit.vintage:junit-vintage-engine:5.10.1")
}
