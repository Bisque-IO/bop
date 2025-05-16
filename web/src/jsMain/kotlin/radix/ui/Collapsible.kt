@file:JsModule("@radix-ui/react-collapsible")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyCollapsible = FC {
    val (open, setOpen) = useState(false)

    Collapsible {
        this.open = open
        onOpenChange = setOpen

        CollapsibleTrigger {
            button {
                +"Toggle"
            }
        }

        CollapsibleContent {
            div {
                className = "mt-2"
                +"This content can collapse."
            }
        }
    }
}

*/

// ------------------------------
// Collapsible Root
external interface CollapsibleProps : DefaultProps {
    var open: Boolean?
    var defaultOpen: Boolean?
    var onOpenChange: ((Boolean) -> Unit)?
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("Root")
external val Collapsible: ComponentType<CollapsibleProps>

// ------------------------------
// Trigger
external interface CollapsibleTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val CollapsibleTrigger: ComponentType<CollapsibleTriggerProps>

// ------------------------------
// Content
external interface CollapsibleContentProps : DefaultProps {
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("Content")
external val CollapsibleContent: ComponentType<CollapsibleContentProps>
