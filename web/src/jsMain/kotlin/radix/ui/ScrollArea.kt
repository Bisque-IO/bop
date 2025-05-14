@file:JsModule("@radix-ui/react-scroll-area")
@file:JsNonModule

package radix.ui

import react.*

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
external interface ScrollAreaProps : DefaultProps {
    var type: String? // "auto", "always", "scroll", "hover"
    var scrollHideDelay: Int?
    var asChild: Boolean?
}

@JsName("Root")
external val ScrollArea: ComponentType<ScrollAreaProps>

// ------------------------------
// Viewport
// ------------------------------
external interface ScrollAreaViewportProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Viewport")
external val ScrollAreaViewport: ComponentType<ScrollAreaViewportProps>

// ------------------------------
// Scrollbar
// ------------------------------
external interface ScrollAreaScrollbarProps : DefaultProps {
    var orientation: String? // "horizontal" | "vertical"
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("Scrollbar")
external val ScrollAreaScrollbar: ComponentType<ScrollAreaScrollbarProps>

// ------------------------------
// Thumb
// ------------------------------
external interface ScrollAreaThumbProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Thumb")
external val ScrollAreaThumb: ComponentType<ScrollAreaThumbProps>

// ------------------------------
// Corner (intersection of scrollbars)
// ------------------------------
external interface ScrollAreaCornerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Corner")
external val ScrollAreaCorner: ComponentType<ScrollAreaCornerProps>
