@file:JsModule("@radix-ui/react-tooltip")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

val MyTooltip = FC {
    TooltipProvider {
        TooltipRoot {
            TooltipTrigger {
                button {
                    +"Hover me"
                }
            }
            TooltipContent {
                className = ClassName("bg-gray-800 text-white px-2 py-1 rounded text-sm")
                +"Tooltip info"
                TooltipArrow {
                    className = ClassName("fill-gray-800")
                }
            }
        }
    }
}

*/

// ------------------------------
// Root
// ------------------------------
external interface TooltipRootProps : RadixProps {
    var delayDuration: Int? // milliseconds
    var skipDelayDuration: Int?
    var disableHoverableContent: Boolean?
    var onOpenChange: ((Boolean) -> Unit)?
    var defaultOpen: Boolean?
    var open: Boolean?
}

@JsName("Root")
external val TooltipRoot: ComponentType<TooltipRootProps>

// ------------------------------
// Trigger
// ------------------------------
external interface TooltipTriggerProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val TooltipTrigger: ComponentType<TooltipTriggerProps>

// ------------------------------
// Content
// ------------------------------
external interface TooltipContentProps : RadixProps {
    var side: String? // "top" | "right" | "bottom" | "left"
    var sideOffset: Int?
    var align: String? // "start" | "center" | "end"
    var alignOffset: Int?
    var avoidCollisions: Boolean?
    var collisionPadding: Int?
    var asChild: Boolean?
    var forceMount: Boolean?
    var onEscapeKeyDown: ((dynamic) -> Unit)?
    var onPointerDownOutside: ((dynamic) -> Unit)?
    var onFocusOutside: ((dynamic) -> Unit)?
    var onInteractOutside: ((dynamic) -> Unit)?
}

@JsName("Content")
external val TooltipContent: ComponentType<TooltipContentProps>

// ------------------------------
// Arrow
// ------------------------------
external interface TooltipArrowProps : RadixProps {
    var width: Int?
    var height: Int?
    var asChild: Boolean?
}

@JsName("Arrow")
external val TooltipArrow: ComponentType<TooltipArrowProps>

// ------------------------------
// Provider
// ------------------------------
external interface TooltipProviderProps : RadixProps {
    var delayDuration: Int?
    var skipDelayDuration: Int?
    var disableHoverableContent: Boolean?
}

@JsName("Provider")
external val TooltipProvider: ComponentType<TooltipProviderProps>
