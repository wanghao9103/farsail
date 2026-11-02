import groovy.json.JsonSlurper

plugins { id("com.android.application"); id("org.jetbrains.kotlin.android") }
val mobileVersion = JsonSlurper().parse(rootProject.file("version.json")) as Map<*, *>
val installedKeystore = System.getenv("ANDROID_KEYSTORE_PATH")
android {
    namespace = "app.farsail.android"
    compileSdk = 35
    defaultConfig {
        applicationId = "app.farsail.android"
        minSdk = 26
        targetSdk = 35
        versionCode = providers.gradleProperty("farsailVersionCode").orElse(mobileVersion["versionCode"].toString()).get().toInt()
        versionName = providers.gradleProperty("farsailVersionName").orElse(mobileVersion["version"].toString()).get()
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
    signingConfigs {
        if (installedKeystore != null) create("installedCertificate") {
            storeFile = file(installedKeystore)
            storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS")
            keyPassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
        }
    }
    buildTypes { release {
        isMinifyEnabled = false
        if (installedKeystore != null) signingConfig = signingConfigs.getByName("installedCertificate")
    } }
}
dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.uiautomator:uiautomator:2.3.0")
}
// Build native libraries first: scripts/build-android.sh. Do not produce an APK
// that silently lacks its transport/credential implementation.
tasks.named("preBuild") {
    doFirst {
        listOf("arm64-v8a", "x86_64").forEach { abi ->
            check(file("src/main/jniLibs/$abi/libfarsail_android.so").exists()) {
                "Missing $abi native library. Run scripts/build-android.sh from the repository root."
            }
        }
    }
}

tasks.matching { it.name == "assembleRelease" || it.name == "packageRelease" }.configureEach {
    doFirst { check(installedKeystore != null) { "Release builds require the original installed APK signing certificate." } }
}
