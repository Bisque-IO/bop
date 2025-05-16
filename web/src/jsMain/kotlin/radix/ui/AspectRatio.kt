@file:JsModule("@radix-ui/react-aspect-ratio")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyAspectRatioExample = FC {
    AspectRatio {
        ratio = 16.0 / 9.0
        className = ClassName("bg-gray-200")

        img {
            src = "https://via.placeholder.com/800x450"
            alt = "Aspect Ratio Example"
            className = ClassName("object-cover w-full h-full")
        }
    }
}

*/

// ------------------------------
// AspectRatio
// ------------------------------
external interface AspectRatioProps : DefaultProps {
    var ratio: Double? // e.g. 16.0 / 9.0
    var asChild: Boolean?
}

@JsName("Root")
external val AspectRatio: ComponentType<AspectRatioProps>
