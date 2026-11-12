import org.jetbrains.intellij.platform.gradle.IntelliJPlatformType

plugins {
    id("org.jetbrains.kotlin.jvm") version "2.0.21"
    id("org.jetbrains.intellij.platform") version "2.2.1"
}

group = "dev.satex"
version = "0.2.1"

repositories {
    mavenCentral()
    intellijPlatform { defaultRepositories() }
}

dependencies {
    intellijPlatform {
        create(IntelliJPlatformType.IntellijIdeaCommunity, "2024.2.4")
        plugins("com.redhat.devtools.lsp4ij:0.21.0")
        bundledPlugins("org.jetbrains.plugins.yaml")
        // JSON schema support lives in the platform since 2024.2.
        instrumentationTools()
        pluginVerifier()
    }
}

kotlin { jvmToolchain(21) }

// The one schema of satex.yaml, shared with the VS Code extension.
tasks.processResources {
    from(rootProject.projectDir.resolve("../../satex.schema.json")) { into("schemas") }
}

intellijPlatform {
    pluginConfiguration {
        ideaVersion {
            sinceBuild = "242"
            untilBuild = provider { null }
        }
    }
    buildSearchableOptions = false
}
