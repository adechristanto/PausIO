plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose") version "2.1.21"
}

android {
    namespace = "com.pausio.app.wear"
    compileSdk = 36
    defaultConfig {
        applicationId = "com.pausio.app"
        minSdk = 30
        targetSdk = 36
        // Wear APKs are uploaded independently. Keep their code range separate
        // from the phone artifact while allowing CI to supply a monotonic code.
        versionCode = providers.gradleProperty("pausioWearVersionCode").orNull?.toIntOrNull() ?: 36_001_000
        versionName = providers.gradleProperty("pausioVersionName").orNull
            ?: System.getenv("PAUSIO_VERSION")
            ?: "0.1.0"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions { jvmTarget = "1.8" }
    buildFeatures { compose = true; buildConfig = true }
}
dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.wear.compose:compose-foundation:1.6.2")
    implementation("androidx.wear.compose:compose-material3:1.6.2")
    implementation("androidx.wear:wear:1.3.0")
    implementation("com.google.android.gms:play-services-wearable:20.0.1")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
}
