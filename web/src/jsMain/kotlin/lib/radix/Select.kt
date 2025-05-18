@file:JsModule("@radix-ui/react-select")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

val MySelect = FC {
    val (selected, setSelected) = useState("apple")

    Select {
        value = selected
        onValueChange = setSelected

        SelectTrigger {
            className = ClassName("inline-flex items-center justify-between border px-3 py-2 w-48")
            SelectValue { placeholder = "Select a fruit" }
        }

        SelectContent {
            SelectViewport {
                listOf("apple", "banana", "mango").forEach {
                    SelectItem {
                        value = it
                        +it.replaceFirstChar(Char::uppercase)
                    }
                }
            }
        }
    }
}

*/

// ------------------------------
// Root
external interface SelectProps : RadixProps {
    var value: String?
    var defaultValue: String?
    var onValueChange: ((String) -> Unit)?
    var open: Boolean?
    var defaultOpen: Boolean?
    var onOpenChange: ((Boolean) -> Unit)?
    var required: Boolean?
    var name: String?
    var autoComplete: String?
    var disabled: Boolean?
    var dir: String?
    var asChild: Boolean?
}

@JsName("Root")
external val Select: ComponentType<SelectProps>

// ------------------------------
// Trigger
external interface SelectTriggerProps : RadixProps {
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("Trigger")
external val SelectTrigger: ComponentType<SelectTriggerProps>

// ------------------------------
// Value
external interface SelectValueProps : RadixProps {
    var placeholder: String?
}

@JsName("Value")
external val SelectValue: ComponentType<SelectValueProps>

// ------------------------------
// Icon
external interface SelectIconProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Icon")
external val SelectIcon: ComponentType<SelectIconProps>

// ------------------------------
// Portal
external interface SelectPortalProps : RadixProps {
    var container: dynamic
    var forceMount: Boolean?
}

@JsName("Portal")
external val SelectPortal: ComponentType<SelectPortalProps>

// ------------------------------
// Content
external interface SelectContentProps : RadixProps {
    var position: String? // "popper" or "item-aligned"
    var side: String?
    var sideOffset: Int?
    var align: String?
    var alignOffset: Int?
    var avoidCollisions: Boolean?
    var collisionPadding: Int?
    var asChild: Boolean?
    var forceMount: Boolean?
}

@JsName("Content")
external val SelectContent: ComponentType<SelectContentProps>

// ------------------------------
// Viewport
external interface SelectViewportProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Viewport")
external val SelectViewport: ComponentType<SelectViewportProps>

// ------------------------------
// Group
external interface SelectGroupProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Group")
external val SelectGroup: ComponentType<SelectGroupProps>

// ------------------------------
// Label
external interface SelectLabelProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Label")
external val SelectLabel: ComponentType<SelectLabelProps>

// ------------------------------
// Item
external interface SelectItemProps : RadixProps {
    var value: String
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("Item")
external val SelectItem: ComponentType<SelectItemProps>

// ------------------------------
// ItemText
external interface SelectItemTextProps : RadixProps

@JsName("ItemText")
external val SelectItemText: ComponentType<SelectItemTextProps>

// ------------------------------
// ItemIndicator
external interface SelectItemIndicatorProps : RadixProps {
    var asChild: Boolean?
}

@JsName("ItemIndicator")
external val SelectItemIndicator: ComponentType<SelectItemIndicatorProps>

// ------------------------------
// Separator
external interface SelectSeparatorProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Separator")
external val SelectSeparator: ComponentType<SelectSeparatorProps>

// ------------------------------
// Scroll Up Button
external interface SelectScrollUpButtonProps : RadixProps {
    var asChild: Boolean?
}

@JsName("ScrollUpButton")
external val SelectScrollUpButton: ComponentType<SelectScrollUpButtonProps>

// ------------------------------
// Scroll Down Button
external interface SelectScrollDownButtonProps : RadixProps {
    var asChild: Boolean?
}

@JsName("ScrollDownButton")
external val SelectScrollDownButton: ComponentType<SelectScrollDownButtonProps>
