plugins {
    java
    `java-library`

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

java {
    sourceCompatibility = JavaVersion.VERSION_24
    targetCompatibility = JavaVersion.VERSION_24
    toolchain {
        languageVersion = JavaLanguageVersion.of(24)
    }
}

buildscript {
    repositories {
        mavenCentral()
        gradlePluginPortal()
    }
    dependencies {
        classpath("com.gradleup.shadow:shadow-gradle-plugin:9.0.0-beta13")
    }
}

dependencies {

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

allprojects {
    tasks.withType<JavaCompile> {
        options.compilerArgs.add("--add-exports=java.base/java.nio=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/java.lang=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/java.lang.classfile=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.access=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.misc=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.foreign=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.ref=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.util=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.util.random=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.vm=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.vm.annotation=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/sun.nio.ch=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/java.util.zip=ALL-UNNAMED")
    }

    tasks.withType<JavaExec> {
        jvmArgs("--add-exports", "java.base/jdk.internal.misc=ALL-UNNAMED")
    }

    tasks.withType<JavaExec> {
        jvmArgs(
            "--enable-preview",
            "--enable-native-access=ALL-UNNAMED",
            "--add-opens=java.base/java.nio=ALL-UNNAMED",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.foreign=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.misc=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.ref=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util.random=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm.annotation=ALL-UNNAMED",
            "--add-opens=java.base/sun.nio.ch=ALL-UNNAMED",
            "--add-opens=java.base/java.util.zip=ALL-UNNAMED",
        )
    }

    tasks.withType<Test> {
        jvmArgs(
            "--enable-preview",
            "--enable-native-access=ALL-UNNAMED",
            "--add-opens=java.base/java.nio=ALL-UNNAMED",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.foreign=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.misc=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.ref=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util.random=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm.annotation=ALL-UNNAMED",
            "--add-opens=java.base/sun.nio.ch=ALL-UNNAMED",
            "--add-opens=java.base/java.util.zip=ALL-UNNAMED",
        )
    }
}
