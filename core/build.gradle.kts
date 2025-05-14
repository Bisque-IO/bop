import java.util.Locale

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

java {
    sourceCompatibility = JavaVersion.VERSION_24
    targetCompatibility = JavaVersion.VERSION_24
    toolchain {
        languageVersion = JavaLanguageVersion.of(24)
    }
}

application {
//    mainClass = "bop.Main"
    mainClass = "bop.cluster.Server"

    applicationDefaultJvmArgs = listOf(
        "--add-exports", "java.base/jdk.internal.misc=ALL-UNNAMED"
    )
}

dependencies {
    compileOnly(libs.google.java.format)
    compileOnly(libs.palantir.java.format)

//    api("org.agrona:agrona:2.1.0")
//    api("org.openjdk.jol:jol-core:0.17")
//    implementation("com.conversantmedia:disruptor:1.2.21")
//    implementation("org.ow2.asm:asm:9.7.1")
//    implementation("org.ow2.asm:asm-util:9.7.1")
//    implementation("net.bytebuddy:byte-buddy-dep:1.15.11")
//    implementation("net.bytebuddy:byte-buddy-agent:1.15.11")

    api(libs.affinity)
    api(libs.jspecify)

    api(libs.aeron.client) {
        exclude("org.agrona")
    }
    api(libs.aeron.driver) {
        exclude("org.agrona")
    }
    api(libs.aeron.archive) {
        exclude("org.agrona")
    }
    api(libs.aeron.annotations) {
        exclude("org.agrona")
    }
    api(libs.aeron.cluster) {
        exclude("org.agrona")
    }

    compileOnly(libs.lombok)
    annotationProcessor(libs.lombok)
    testCompileOnly(libs.lombok)
    testAnnotationProcessor(libs.lombok)

    annotationProcessor(libs.dagger.compiler)
    testAnnotationProcessor(libs.dagger.compiler)

    annotationProcessor(libs.auto.service)
    testAnnotationProcessor(libs.auto.service)

    testImplementation(libs.junit.jupiter.api)
    testImplementation(platform(libs.junit.bom))
    testRuntimeOnly(libs.junit.jupiter.engine)
    testImplementation(libs.junit.jupiter)

    testImplementation(kotlin("test"))

    api("org.jctools:jctools-core:4.0.5")
//    api("com.google.dagger:dagger:2.56.1")
    api("net.openhft:zero-allocation-hashing:0.27ea0")
    api("com.dynatrace.hash4j:hash4j:0.22.0")
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

    tasks.test {
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

enum class OsName { WINDOWS, MAC, LINUX, UNKNOWN }
enum class OsArch { X86_32, X86_64, ARM64, UNKNOWN }
data class OsType(val name: OsName, val arch: OsArch)

val currentOsType = run {
    val gradleOs = org.gradle.internal.os.OperatingSystem.current()
    val osName = when {
        gradleOs.isMacOsX -> OsName.MAC
        gradleOs.isWindows -> OsName.WINDOWS
        gradleOs.isLinux -> OsName.LINUX
        else -> OsName.UNKNOWN
    }

    val osArch = when (providers.systemProperty("sun.arch.data.model").get()) {
        "32" -> OsArch.X86_32
        "64" -> when (providers.systemProperty("os.arch").get().lowercase(Locale.getDefault())) {
            "aarch64" -> OsArch.ARM64
            else -> OsArch.X86_64
        }
        else -> OsArch.UNKNOWN
    }

    OsType(osName, osArch)
}

//jmh {
////    jvmArgs.add("-XX:+UseG1GC")
////    jvmArgs.add("-XX:+UseParallelGC")
////    jvmArgs.add("-XX:+UseZGC")
////    jvmArgs.add("-XX:+ZGenerational")
//    jvmArgs.add("-XX:+UseSerialGC")
////    jvmArgs.add("-XX:+UseShenandoahGC")
//    jvmArgs.add("-Xmn128m")
//    jvmArgs.add("-Xms256m")
//    jvmArgs.add("-Xmx256m")
//    jvmArgs.add("-XX:+PrintGC")
////    jvmArgs.add("-Xlog:gc")
//
//    jvmArgs.add("--enable-preview")
//    jvmArgs.add("--enable-native-access=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/java.nio=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/java.lang=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/java.lang.reflect=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/jdk.internal.access=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/jdk.internal.misc=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/jdk.internal.util.random=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/jdk.internal.vm=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/jdk.internal.vm.annotations=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/java.util.zip=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=java.base/sun.nio.ch=ALL-UNNAMED")
//    jvmArgs.add("--add-opens=jdk.compiler/com.sun.tools.javac.file=ALL-UNNAMED")
//    jvmArgs.add("--add-exports=jdk.compiler/com.sun.tools.javac.file=ALL-UNNAMED")
//}

graalvmNative {
    binaries.all {
        resources.autodetect()
        asCompileOptions().debug.set(false)
        asCompileOptions().sharedLibrary.set(false)
        asCompileOptions().useFatJar.set(true)
        asCompileOptions().richOutput.set(true)
        asCompileOptions().pgoInstrument.set(false)
        asCompileOptions().verbose.set(true)


        buildArgs.add("-H:+UnlockExperimentalVMOptions")
        val reflectConfigPath = buildscript.sourceFile?.toPath()?.parent?.resolve("reflectconfig.json")?.toAbsolutePath()
        buildArgs.add("-H:ReflectionConfigurationFiles=$reflectConfigPath")
        buildArgs.add("-H:+UnlockExperimentalVMOptions")
        buildArgs.add("-H:+ForeignAPISupport")
//        buildArgs.add("-H:+BuildReport")
//        buildArgs.add("--emit build-report")
//        buildArgs.add("build-report")
//        jvmArgs.add("-XX:+UseG1GC")
//        jvmArgs.add("-XX:+UseSerialGC")
//        jvmArgs.add("-XX:+UseZGC")
//        jvmArgs.add("-XX:+PrintGC")

        jvmArgs(
            "--enable-preview",
            "--enable-native-access=ALL-UNNAMED",
            "--add-opens=java.base/java.nio=ALL-UNNAMED",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
//            "--add-opens=java.base/java.lang.classfile=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.misc=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.ref=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util.random=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm.annotation=ALL-UNNAMED",
            "--add-opens=java.base/sun.nio.ch=ALL-UNNAMED",
            "--add-opens=java.base/java.util.zip=ALL-UNNAMED",
        )

        buildArgs(
            "--enable-preview",
            "--enable-native-access=ALL-UNNAMED",
            "--add-opens=java.base/java.nio=ALL-UNNAMED",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
//            "--add-opens=java.base/java.lang.classfile=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.access=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.misc=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.ref=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.util.random=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm=ALL-UNNAMED",
            "--add-opens=java.base/jdk.internal.vm.annotation=ALL-UNNAMED",
            "--add-opens=java.base/sun.nio.ch=ALL-UNNAMED",
            "--add-opens=java.base/java.util.zip=ALL-UNNAMED",
        )

//        buildArgs("--initialize-at-build-time=io.movemedical.server.essentials.runtime.AppGraph")
//        buildArgs("--initialize-at-build-time=move.*")
        buildArgs("-O1")
//        buildArgs("-march=native")
//        buildArgs("--pgo")
//        buildArgs("--initialize-at-build-time=move.App")
//        buildArgs("--initialize-at-build-time=move.AppJson")
//        buildArgs("--initialize-at-build-time=move.AppJson\$ImageSerializer")
//        buildArgs("--initialize-at-build-time=move.AppJson\$ImageBinarySerializer")
//        buildArgs("--initialize-at-build-time=org.slf4j.simple.SimpleLogger")
//        buildArgs("--initialize-at-build-time=org.slf4j.helpers.NOP_FallbackServiceProvider")
//        buildArgs("--initialize-at-build-time=org.slf4j.helpers.SubstituteServiceProvider")
//        buildArgs("--initialize-at-build-time=org.slf4j.helpers.SubstituteServiceFactory")
//        buildArgs("--initialize-at-build-time=org.slf4j.simple.SimpleServiceProvider")
//        buildArgs("--initialize-at-build-time=org.slf4j.helpers.NOPLoggerFactory")
//        buildArgs("--initialize-at-build-time=org.slf4j.simple.SimpleLoggerConfiguration")
//        buildArgs("--initialize-at-build-time=org.slf4j.simple.SimpleLoggerFactory")
//        buildArgs("--initialize-at-build-time=org.slf4j.simple.OutputChoice")
//        buildArgs("--initialize-at-build-time=org.slf4j.helpers.SubstituteLoggerFactory")
//        buildArgs("--initialize-at-build-time=org.slf4j.simple.OutputChoice\$OutputChoiceType")
////        buildArgs("--initialize-at-build-time=io.fury.resolver.ClassInfo")
//        buildArgs("--initialize-at-build-time=com.alibaba.fastjson2.JSON")
//        buildArgs("--initialize-at-build-time=com.alibaba.fastjson2.reader.ObjectReader")
//        buildArgs("--initialize-at-build-time=com.alibaba.fastjson2.reader.ObjectWriter")
//        buildArgs("--initialize-at-build-time=com.google.common.reflect.Types\$JavaVersion\$2")
//        buildArgs("--initialize-at-build-time=com.google.common.reflect.Types\$ClassOwnership\$2")
//        buildArgs("--initialize-at-build-time=com.google.common.collect.ImmutableMapEntry\$NonTerminalImmutableMapEntry")
//        buildArgs.add("--static")
//        buildArgs.add("--gc=epsilon")
    }
    toolchainDetection = true
}