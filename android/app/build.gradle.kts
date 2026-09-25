import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
    id("app.cash.paparazzi")
}

// One version for every KuDownloader build: the workspace Cargo.toml.
val workspaceVersion: String = run {
    val toml = rootProject.file("../Cargo.toml").readText()
    Regex("""(?m)^version\s*=\s*"([^"]+)"""").find(toml)?.groupValues?.get(1) ?: "0.0.0"
}
val versionNumber: Int = workspaceVersion.split('.', '-').take(3).map { it.toIntOrNull() ?: 0 }
    .let { (a, b, c) -> a * 10000 + b * 100 + c }

// Release signing: android/keystore.properties or the KU_ANDROID_* environment
// (CI secrets). Without either, release builds use the debug key.
val signing: Properties = Properties().apply {
    val f = rootProject.file("keystore.properties")
    if (f.isFile) f.inputStream().use { load(it) }
    System.getenv("KU_ANDROID_KEYSTORE")?.let { setProperty("storeFile", it) }
    System.getenv("KU_ANDROID_KEYSTORE_PASSWORD")?.let { setProperty("storePassword", it) }
    System.getenv("KU_ANDROID_KEY_ALIAS")?.let { setProperty("keyAlias", it) }
    System.getenv("KU_ANDROID_KEY_PASSWORD")?.let { setProperty("keyPassword", it) }
}
val hasReleaseKey = signing.getProperty("storeFile")?.let { file(it).isFile } == true

android {
    namespace = "digital.kuduy.kudownloader"
    compileSdk = 35

    defaultConfig {
        applicationId = "digital.kuduy.kudownloader"
        minSdk = 24
        targetSdk = 35
        versionCode = versionNumber * 10
        versionName = workspaceVersion
        vectorDrawables { useSupportLibrary = true }
    }

    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64")
            isUniversalApk = true
        }
    }

    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                storeFile = file(signing.getProperty("storeFile"))
                storePassword = signing.getProperty("storePassword")
                keyAlias = signing.getProperty("keyAlias")
                keyPassword = signing.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = if (hasReleaseKey) signingConfigs.getByName("release") else signingConfigs.getByName("debug")
        }
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
        isCoreLibraryDesugaringEnabled = true
    }
    kotlinOptions { jvmTarget = "17" }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        // yt-dlp's Python, FFmpeg and aria2 ship as executables in the library
        // folder; they must be extracted to disk to run.
        jniLibs { useLegacyPackaging = true }
        resources { excludes += setOf("/META-INF/{AL2.0,LGPL2.1}", "/META-INF/DEPENDENCIES", "/META-INF/*.kotlin_module") }
    }

    lint {
        // Reported in CI (lint-results-release.html); never blocks a build.
        abortOnError = false
        checkReleaseBuilds = false
        // Translations live in assets/i18n, not strings.xml.
        disable += setOf("MissingTranslation")
    }

    androidResources {
        // Interface text comes from assets/i18n (shared with the desktop app).
        generateLocaleConfig = false
    }
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2024.12.01")
    implementation(composeBom)
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.animation:animation")
    debugImplementation("androidx.compose.ui:ui-tooling")

    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.3")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-process:2.8.7")
    implementation("androidx.lifecycle:lifecycle-service:2.8.7")
    implementation("androidx.webkit:webkit:1.12.1")
    implementation("androidx.documentfile:documentfile:1.1.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("io.coil-kt:coil-compose:2.7.0")
    testImplementation("junit:junit:4.13.2")

    // yt-dlp (with its Python), FFmpeg and aria2 built for Android.
    val ytdl = "0.17.2"
    implementation("io.github.junkfood02.youtubedl-android:library:$ytdl")
    implementation("io.github.junkfood02.youtubedl-android:ffmpeg:$ytdl")
    implementation("io.github.junkfood02.youtubedl-android:aria2c:$ytdl")
}
