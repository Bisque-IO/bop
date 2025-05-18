@file:JsModule("@radix-ui/react-scroll-area")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

val MyScrollArea = FC {
    ScrollArea {
        className = "w-64 h-40 border"

        ScrollAreaViewport {
            div {
                repeat(50) {
                    div { +"Item #$it" }
                }
            }
        }

        ScrollAreaScrollbar {
            ScrollAreaThumb()
        }

        ScrollAreaScrollbar {
            orientation = "horizontal"
            ScrollAreaThumb()
        }

        ScrollAreaCorner()
    }
}

*/

// ------------------------------
// Root
// ------------------------------
external interface ScrollAreaProps : RadixProps {
    var type: String? // "auto", "always", "scroll", "hover"
    var scrollHideDelay: Int?
    var asChild: Boolean?
}

@JsName("Root")
external val ScrollArea: ComponentType<ScrollAreaProps>

// ------------------------------
// Viewport
// ------------------------------
external interface ScrollAreaViewportProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Viewport")
external val ScrollAreaViewport: ComponentType<ScrollAreaViewportProps>

// ------------------------------
// Scrollbar
// ------------------------------
external interface ScrollAreaScrollbarProps : RadixProps {
    var orientation: String? // "horizontal" | "vertical"
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("Scrollbar")
external val ScrollAreaScrollbar: ComponentType<ScrollAreaScrollbarProps>

// ------------------------------
// Thumb
// ------------------------------
external interface ScrollAreaThumbProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Thumb")
external val ScrollAreaThumb: ComponentType<ScrollAreaThumbProps>

// ------------------------------
// Corner (intersection of scrollbars)
// ------------------------------
external interface ScrollAreaCornerProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Corner")
external val ScrollAreaCorner: ComponentType<ScrollAreaCornerProps>
