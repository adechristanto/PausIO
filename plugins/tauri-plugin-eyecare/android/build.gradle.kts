plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.pausio.app.eyecare"
    compileSdk = 36
    defaultConfig { minSdk = 24 }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
        // minSdk is 24 but the reminder planner parses and formats ISO-8601
        // instants with java.time, which is API 26. Desugaring keeps those
        // APIs available on 24/25 instead of forcing a minSdk bump that would
        // drop devices, or a second date parser that could disagree with the
        // one the engine uses.
        isCoreLibraryDesugaringEnabled = true
    }
    kotlinOptions { jvmTarget = "1.8" }
}

dependencies {
    implementation(project(":tauri-android"))
    implementation("com.google.android.gms:play-services-wearable:20.0.1")
    // NotificationCompat and the permission check used by standalone phone
    // reminders; both must behave consistently back to the minSdk of 24.
    implementation("androidx.core:core-ktx:1.13.1")
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.5")
}
