@file:JsModule("@radix-ui/react-accordion")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyAccordion = FC {
    AccordionRoot {
        type = "single"
        collapsible = true
        defaultValue = "item-1"

        AccordionItem {
            value = "item-1"
            AccordionTrigger {
                +"Item 1"
            }
            AccordionContent {
                div { +"This is the content of item 1." }
            }
        }

        AccordionItem {
            value = "item-2"
            AccordionTrigger {
                +"Item 2"
            }
            AccordionContent {
                div { +"This is the content of item 2." }
            }
        }
    }
}

*/

// ------------------------------
// Root
// ------------------------------
external interface AccordionRootProps : DefaultProps {
    var type: String // "single" or "multiple"
    var defaultValue: dynamic /* String or Array<String> */
    var value: dynamic /* String or Array<String> */
    var onValueChange: ((dynamic) -> Unit)?
    var collapsible: Boolean?
    var disabled: Boolean?
    var orientation: String? // "vertical" | "horizontal"
    var dir: String? // "ltr" | "rtl"
}

@JsName("Root")
external val AccordionRoot: ComponentType<AccordionRootProps>

// ------------------------------
// Item
// ------------------------------
external interface AccordionItemProps : DefaultProps {
    var value: String
    var disabled: Boolean?
}

@JsName("Item")
external val AccordionItem: ComponentType<AccordionItemProps>

// ------------------------------
// Trigger
// ------------------------------
external interface AccordionTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val AccordionTrigger: ComponentType<AccordionTriggerProps>

// ------------------------------
// Content
// ------------------------------
external interface AccordionContentProps : DefaultProps {
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("Content")
external val AccordionContent: ComponentType<AccordionContentProps>
