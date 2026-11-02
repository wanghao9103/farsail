plugins { id("com.android.application"); id("org.jetbrains.kotlin.android") }
android {
    namespace = "app.farsail.android"
    compileSdk = 35
    defaultConfig {
        applicationId = "app.farsail.android"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }
    compileOptions { sourceCompatibility = JavaVersion.VERSION_17; targetCompatibility = JavaVersion.VERSION_17 }
    kotlinOptions { jvmTarget = "17" }
    buildTypes { release { isMinifyEnabled = false } }
}
dependencies {
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
