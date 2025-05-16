@file:JsModule("@radix-ui/react-separator")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MySeparatorExample = FC {
    div {
        +"Above the line"
        Separator {
            orientation = "horizontal"
            className = ClassName("my-4 h-px bg-gray-300")
        }
        +"Below the line"
    }
}

*/

// ------------------------------
// Separator
// ------------------------------
external interface SeparatorProps : DefaultProps {
    var orientation: String? // "horizontal" | "vertical"
    var decorative: Boolean?
    var asChild: Boolean?
}

@JsName("Root")
external val Separator: ComponentType<SeparatorProps>
