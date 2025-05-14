plugins {
    java
    `java-library`
    application
    alias(libs.plugins.kotlinJvm)
    alias(libs.plugins.jmh)
    alias(libs.plugins.graalNative)
    alias(libs.plugins.shadow)
    alias(libs.plugins.spotless)
}

group = "bop"
version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
    gradlePluginPortal()
    mavenLocal()
}

dependencies {
    compileOnly(libs.google.java.format)
    compileOnly(libs.palantir.java.format)

    implementation(kotlin("compiler-embeddable"))
//    implementation("org.jetbrains.kotlin:kotlin-compiler-embeddable:2.2.0-Beta2")
}

tasks {
    javadoc {
        options.encoding = "UTF-8"
    }
    compileJava {
        options.encoding = "UTF-8"
    }
    compileTestJava {
        options.encoding = "UTF-8"
    }
    test {
        useJUnitPlatform()
        testLogging {
            showStandardStreams = true
        }
    }
}

spotless {
    java {
        target("src/**/*.java", "build/generated/**/*.java")
        importOrder()
        palantirJavaFormat(libs.palantir.java.format.get().version)
            .formatJavadoc(true)
            .style("GOOGLE")
        formatAnnotations()
    }

    kotlin {
        target("src/**/*.kt", "build/generated/**/*.kt")
        ktlint(libs.ktlint.get().version)
        trimTrailingWhitespace()
        endWithNewline()
        ktfmt(libs.ktfmt.get().version).googleStyle()
    }
}

tasks["spotlessJava"].dependsOn("compileJava")
tasks["spotlessJava"].dependsOn("compileTestJava")
tasks["spotlessKotlin"].dependsOn("compileJava")
tasks["spotlessKotlin"].dependsOn("compileTestJava")
tasks["spotlessJavaCheck"].dependsOn("compileJava")
tasks["spotlessJavaCheck"].dependsOn("compileTestJava")
tasks["spotlessJavaCheck"].enabled = false
tasks["spotlessKotlinCheck"].enabled = false