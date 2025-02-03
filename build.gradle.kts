plugins {
    java
    `java-library`
    application
    id("me.champeau.jmh") version "0.7.2"
    id("org.graalvm.buildtools.native") version "0.10.4"
    id("com.gradleup.shadow") version "9.0.0-beta5"
    id("com.diffplug.spotless") version "7.0.2"
}

group = "bop"
version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
}

java {
    sourceCompatibility = JavaVersion.VERSION_23
    targetCompatibility = JavaVersion.VERSION_23
    toolchain {
        languageVersion = JavaLanguageVersion.of(23)
    }
}

application {
//    mainClass = "bop.Main"
    mainClass = "bop.kernel.VThreadTests"
}

dependencies {
    compileOnly("com.google.googlejavaformat:google-java-format:1.25.2")
    compileOnly("com.palantir.javaformat:palantir-java-format:2.50.0")

    implementation("org.agrona:agrona:2.0.1")
    implementation("org.openjdk.jol:jol-core:0.17")
//    implementation("com.conversantmedia:disruptor:1.2.21")
//    implementation("org.ow2.asm:asm:9.7.1")
//    implementation("org.ow2.asm:asm-util:9.7.1")
//    implementation("net.bytebuddy:byte-buddy-dep:1.15.11")
//    implementation("net.bytebuddy:byte-buddy-agent:1.15.11")
    implementation("org.jctools:jctools-core:4.0.5")
    implementation("com.google.dagger:dagger:2.55")
    implementation("net.openhft:zero-allocation-hashing:0.27ea0")
    implementation("org.jspecify:jspecify:1.0.0")



    compileOnly("org.projectlombok:lombok:1.18.36")
    annotationProcessor("com.google.dagger:dagger-compiler:2.55")
    annotationProcessor("org.projectlombok:lombok:1.18.36")
    annotationProcessor("com.google.auto.service:auto-service:1.1.1")

    testAnnotationProcessor("com.google.dagger:dagger-compiler:2.55")
    testAnnotationProcessor("org.projectlombok:lombok:1.18.36")
    testAnnotationProcessor("com.google.auto.service:auto-service:1.1.1")

    testCompileOnly("org.projectlombok:lombok:1.18.36")
    testImplementation("org.junit.jupiter:junit-jupiter-api:5.10.2")
    testRuntimeOnly("org.junit.jupiter:junit-jupiter-engine")
    testImplementation(platform("org.junit:junit-bom:5.10.2"))
    testImplementation("org.junit.jupiter:junit-jupiter")
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
        palantirJavaFormat("2.50.0")
            .formatJavadoc(true)
            .style("GOOGLE")
        formatAnnotations()
    }
}

tasks["spotlessJava"].dependsOn("compileJava")
tasks["spotlessJava"].dependsOn("compileTestJava")
tasks["spotlessJavaCheck"].dependsOn("compileJava")
tasks["spotlessJavaCheck"].dependsOn("compileTestJava")
tasks["spotlessJavaCheck"].enabled = false

tasks.test {
    useJUnitPlatform()
}

allprojects {
    tasks.withType<JavaCompile> {
        options.compilerArgs.add("--enable-preview")
        options.compilerArgs.add("--add-exports=java.base/java.nio=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/java.lang=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/java.lang.classfile=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.access=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.misc=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.ref=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.util=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.util.random=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.vm=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/jdk.internal.vm.annotation=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/sun.nio.ch=ALL-UNNAMED")
        options.compilerArgs.add("--add-exports=java.base/java.util.zip=ALL-UNNAMED")
    }

    tasks.withType<JavaExec> {
        jvmArgs(
            "--enable-preview",
            "--enable-native-access=ALL-UNNAMED",
            "--add-opens=java.base/java.nio=ALL-UNNAMED",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
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
    }

    tasks.withType<Test> {
        jvmArgs(
            "--enable-preview",
            "--enable-native-access=ALL-UNNAMED",
            "--add-opens=java.base/java.nio=ALL-UNNAMED",
            "--add-opens=java.base/java.lang=ALL-UNNAMED",
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
    }
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
        buildArgs("-O3")
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