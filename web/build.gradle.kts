import org.gradle.kotlin.dsl.support.kotlinCompilerOptions

plugins {
    alias(libs.plugins.kotlinMultiplatform)
    alias(libs.plugins.spotless)
}

group = "bop"

version = "1.0-SNAPSHOT"

repositories {
    mavenCentral()
    gradlePluginPortal()
    mavenLocal()
}

kotlin {
    sourceSets {
        jsMain.languageSettings {
            optIn("kotlin.RequiresOptIn")
            compilerOptions.freeCompilerArgs.add("-Xcontext-parameters")
        }
        jsMain.dependencies {
            implementation("org.jetbrains.kotlin-wrappers:kotlin-react:2025.5.6-19.1.0")
            implementation("org.jetbrains.kotlin-wrappers:kotlin-react-dom:2025.5.6-19.1.0")

            implementation("org.jetbrains.kotlin-wrappers:kotlin-react-router-dom:2025.1.6-6.28.0")
            implementation("org.jetbrains.kotlin-wrappers:kotlin-remix-run-router:2025.1.6-1.21.0")

            implementation("org.jetbrains.kotlin-wrappers:kotlin-tanstack-table-core:2025.5.6-8.21.3")

            //
            // implementation("org.jetbrains.kotlin-wrappers:kotlin-typescript-js:2025.5.6-5.7.2")

            implementation(npm("@dnd-kit/core", "^6.3.1"))
            implementation(npm("@dnd-kit/modifiers", "^9.0.0"))
            implementation(npm("@dnd-kit/sortable", "^10.0.0"))
            implementation(npm("@dnd-kit/utilities", "^3.2.2"))
            implementation(npm("@radix-ui/react-accessible-icon", "^1.1.1"))
            implementation(npm("@radix-ui/react-accordion", "^1.2.2"))
            implementation(npm("@radix-ui/react-alert-dialog", "^1.1.5"))
            implementation(npm("@radix-ui/react-aspect-ratio", "^1.1.1"))
            implementation(npm("@radix-ui/react-avatar", "^1.1.2"))
            implementation(npm("@radix-ui/react-checkbox", "^1.1.3"))
            implementation(npm("@radix-ui/react-collapsible", "^1.1.2"))
            implementation(npm("@radix-ui/react-context-menu", "^2.2.5"))
            implementation(npm("@radix-ui/react-dialog", "^1.1.5"))
            implementation(npm("@radix-ui/react-dropdown-menu", "^2.1.5"))
            implementation(npm("@radix-ui/react-hover-card", "^1.1.5"))
            implementation(npm("@radix-ui/react-icons", "^1.3.2"))
            implementation(npm("@radix-ui/react-label", "^2.1.1"))
            implementation(npm("@radix-ui/react-menubar", "^1.1.5"))
            implementation(npm("@radix-ui/react-navigation-menu", "^1.2.4"))
            implementation(npm("@radix-ui/react-popover", "^1.1.5"))
            implementation(npm("@radix-ui/react-portal", "^1.1.3"))
            implementation(npm("@radix-ui/react-progress", "^1.1.1"))
            implementation(npm("@radix-ui/react-radio-group", "^1.2.2"))
            implementation(npm("@radix-ui/react-scroll-area", "^1.2.3"))
            implementation(npm("@radix-ui/react-select", "^2.1.5"))
            implementation(npm("@radix-ui/react-separator", "^1.1.1"))
            implementation(npm("@radix-ui/react-slider", "^1.2.2"))
            implementation(npm("@radix-ui/react-slot", "^1.1.1"))
            implementation(npm("@radix-ui/react-switch", "^1.1.2"))
            implementation(npm("@radix-ui/react-tabs", "^1.1.2"))
            implementation(npm("@radix-ui/react-toast", "^1.2.5"))
            implementation(npm("@radix-ui/react-toggle", "^1.1.1"))
            implementation(npm("@radix-ui/react-toggle-group", "^1.1.1"))
            implementation(npm("@radix-ui/react-tooltip", "^1.1.7"))
            implementation(npm("@radix-ui/themes", "^3.2.1"))

            implementation(npm("@tabler/icons-react", "^3.31.0"))

            implementation(npm("class-variance-authority", "^0.7.1"))
            implementation(npm("clsx", "^2.1.1"))
            implementation(npm("cmdk", "^1.1.1"))
            implementation(npm("date-fns", "^4.1.0"))
            implementation(npm("embla-carousel-react", "^8.6.0"))
            implementation(npm("input-otp", "^1.4.2"))
            implementation(npm("lucide-react", "^0.474.0"))
            implementation(npm("react-day-picker", "^9.5.0"))
            implementation(npm("react-resizable-panels", "^3.0.2"))
            implementation(npm("recharts", "^2.15.1"))
            implementation(npm("sonner", "^2.0.3"))
            implementation(npm("tailwindcss", "^4.0.7"))
            implementation(npm("tailwind-merge", "^3.3.0"))
            implementation(npm("vaul", "^1.1.2"))
            implementation(npm("zod", "^3.24.1"))
        }
        jsTest.dependencies { implementation(kotlin("test")) }
    }

    js {
        useEsModules()
//        useCommonJs()

        browser {
            commonWebpackConfig {
                outputFileName = "bop-bundle.js"
                showProgress = true
                cssSupport { enabled = true }
            }

            testTask {
                enabled = true
                useKarma {
                    useIe()
                    useSafari()
                    useFirefox()
                    useChrome()
                    useChromeCanary()
                    useChromeHeadless()
                    useOpera()
                }
            }
        }
        binaries.executable()
    }
}

spotless {
    kotlin {
        target("src/**/*.kt", "build/generated/**/*.kt", "**/*.kts")
        //        ktlint(libs.ktlint.get().version)
        trimTrailingWhitespace()
        endWithNewline()
//    ktfmt(libs.ktfmt.get().version).googleStyle()
//    ktfmt(libs.ktfmt.get().version).kotlinlangStyle().configure {
//      it.setBlockIndent(3)
//      it.setContinuationIndent(3)
//    }
    }
}

tasks["spotlessKotlin"].dependsOn("compileKotlinJs")

tasks["spotlessKotlin"].dependsOn("compileTestKotlinJs")

tasks["spotlessKotlinCheck"].enabled = false
