@file:JsModule("@radix-ui/react-popover")
@file:JsNonModule

package radix.ui

import react.*
import kotlin.js.*

/*

val MyPopover = FC {
    val (isOpen, setOpen) = useState(false)

    PopoverRoot {
        open = isOpen
        onOpenChange = setOpen

        PopoverTrigger {
            button {
                +"Open Popover"
            }
        }

        PopoverContent {
            className = ClassName("bg-white p-4 rounded shadow-lg")
            div { +"This is the popover content." }

            PopoverClose {
                button {
                    className = ClassName("mt-2 text-sm text-red-500")
                    +"Close"
                }
            }

            PopoverArrow {
                className = ClassName("fill-white")
            }
        }
    }
}

*/

external interface PopoverRootProps : DefaultProps {
    var open: Boolean?
    var defaultOpen: Boolean?
    var onOpenChange: ((Boolean) -> Unit)?
    var modal: Boolean?
}

@JsName("Root")
external val PopoverRoot: ComponentType<PopoverRootProps>

// ------------------------------
// Trigger
// ------------------------------
external interface PopoverTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val PopoverTrigger: ComponentType<PopoverTriggerProps>

// ------------------------------
// Content
// ------------------------------
external interface PopoverContentProps : Props {
    var side: String? // "top", "right", "bottom", "left"
    var sideOffset: Int?
    var align: String? // "start", "center", "end"
    var alignOffset: Int?
    var avoidCollisions: Boolean?
    var collisionPadding: Int?
    var onEscapeKeyDown: ((dynamic) -> Unit)?
    var onPointerDownOutside: ((dynamic) -> Unit)?
    var onFocusOutside: ((dynamic) -> Unit)?
    var onInteractOutside: ((dynamic) -> Unit)?
    var asChild: Boolean?
}

@JsName("Content")
external val PopoverContent: ComponentType<PopoverContentProps>

// ------------------------------
// Arrow
// ------------------------------
external interface PopoverArrowProps : DefaultProps {
    var width: Int?
    var height: Int?
    var asChild: Boolean?
}

@JsName("Arrow")
external val PopoverArrow: ComponentType<PopoverArrowProps>

// ------------------------------
// Close
// ------------------------------
external interface PopoverCloseProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Close")
external val PopoverClose: ComponentType<PopoverCloseProps>

// ------------------------------
// Anchor
// ------------------------------
external interface PopoverAnchorProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Anchor")
external val PopoverAnchor: ComponentType<PopoverAnchorProps>
