plugins {
    id("com.android.application")
}

android {
    namespace = "com.minimidisynth"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.minimidisynth"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}
