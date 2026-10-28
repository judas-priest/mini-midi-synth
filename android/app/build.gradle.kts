plugins {
    id("com.android.application")
}

// Release signing is configured through ~/.gradle/gradle.properties so that no
// keystore path, password or alias ever lands in the repository:
//
//   mmsStoreFile=/home/you/keys/mini-midi-synth.jks
//   mmsStorePassword=...
//   mmsKeyAlias=mini-midi-synth
//   mmsKeyPassword=...
//   mmsVersionCode=2
//
// Without those properties the release build falls back to the debug key and
// says so loudly, so a local assembleRelease still works for testing.
val hasReleaseKey = project.hasProperty("mmsStoreFile")

android {
    namespace = "com.minimidisynth"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.minimidisynth"
        minSdk = 29
        targetSdk = 35
        versionCode = (project.findProperty("mmsVersionCode") as String?)?.toInt() ?: 1
        versionName = "0.1.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                storeFile = file(project.property("mmsStoreFile") as String)
                storePassword = project.property("mmsStorePassword") as String
                keyAlias = project.property("mmsKeyAlias") as String
                keyPassword = project.property("mmsKeyPassword") as String
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (hasReleaseKey) {
                signingConfigs.getByName("release")
            } else {
                logger.warn(
                    "No mmsStoreFile property found — signing release with the debug key. " +
                    "This APK cannot be distributed."
                )
                signingConfigs.getByName("debug")
            }
        }
    }
}
