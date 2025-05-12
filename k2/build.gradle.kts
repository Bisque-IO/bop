plugins {
    java
    `java-library`
    application
    kotlin("jvm") version "2.2.0-Beta2"
    id("me.champeau.jmh") version "0.7.2"
    id("org.graalvm.buildtools.native") version "0.10.6"
    id("com.gradleup.shadow") version "9.0.0-beta13"
    id("com.diffplug.spotless") version "7.0.3"
}

group = "bop"
version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
    gradlePluginPortal()
    mavenLocal()
}

dependencies {
    compileOnly("com.google.googlejavaformat:google-java-format:1.26.0")
    compileOnly("com.palantir.javaformat:palantir-java-format:2.62.0")

//    implementation(kotlin("compiler-embeddable"))
    implementation("org.jetbrains.kotlin:kotlin-compiler-embeddable:2.2.0-Beta2")
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
//        googleJavaFormat("1.25.2").reflowLongStrings()
        palantirJavaFormat("2.62.0")
            .formatJavadoc(true)
            .style("GOOGLE")
        formatAnnotations()
    }

    kotlin {
        target("src/**/*.kt", "build/generated/**/*.kt")
        ktlint("1.5.0")
        trimTrailingWhitespace()
        endWithNewline()
        ktfmt("0.54").googleStyle()
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