@file:JsModule("@radix-ui/react-progress") @file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyProgressBar = FC {
    Progress {
        value = 60.0
        max = 100.0
        className = ClassName("relative w-full h-4 bg-gray-200 rounded overflow-hidden")

        ProgressIndicator {
            className = ClassName("absolute top-0 left-0 h-full bg-blue-500")
            style = unsafeJso { width = "60%" }
        }
    }
}

*/

// ------------------------------
// Progress Root
external interface ProgressProps : DefaultProps {
    var value: Double?
    var max: Double?
    var asChild: Boolean?
}

@JsName("Root")
external val Progress: ComponentType<ProgressProps>

// ------------------------------
// Progress Indicator
external interface ProgressIndicatorProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Indicator")
external val ProgressIndicator: ComponentType<ProgressIndicatorProps>
