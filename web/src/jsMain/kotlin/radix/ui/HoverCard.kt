@file:JsModule("@radix-ui/react-hover-card")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyHoverCard = FC {
    HoverCard {
        HoverCardTrigger {
            button { +"Hover me" }
        }

        HoverCardContent {
            className = ClassName("p-4 bg-white rounded shadow")
            +"Additional info appears here."
            HoverCardArrow {
                className = ClassName("fill-white")
            }
        }
    }
}

*/

// ------------------------------
// Root
@JsName("Root")
external val HoverCard: ComponentType<DefaultProps>

// ------------------------------
// Trigger
external interface HoverCardTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val HoverCardTrigger: ComponentType<HoverCardTriggerProps>

// ------------------------------
// Content
external interface HoverCardContentProps : DefaultProps {
    var side: String?
    var sideOffset: Int?
    var align: String?
    var alignOffset: Int?
    var avoidCollisions: Boolean?
    var collisionPadding: Int?
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("Content")
external val HoverCardContent: ComponentType<HoverCardContentProps>

// ------------------------------
// Arrow
external interface HoverCardArrowProps : DefaultProps {
    var width: Int?
    var height: Int?
    var asChild: Boolean?
}

@JsName("Arrow")
external val HoverCardArrow: ComponentType<HoverCardArrowProps>

// ------------------------------
// Portal
external interface HoverCardPortalProps : DefaultProps {
    var forceMount: Boolean?
    var container: dynamic // HTMLElement or null
}

@JsName("Portal")
external val HoverCardPortal: ComponentType<HoverCardPortalProps>
