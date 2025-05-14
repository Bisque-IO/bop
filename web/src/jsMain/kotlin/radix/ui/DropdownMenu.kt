@file:JsModule("@radix-ui/react-dropdown-menu")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyDropdown = FC {
    DropdownMenu {
        DropdownMenuTrigger {
            button { +"Open Menu" }
        }

        DropdownMenuContent {
            DropdownMenuItem { +"Profile" }
            DropdownMenuCheckboxItem {
                checked = true
                +"Enable feature"
            }
            DropdownMenuSeparator()
            DropdownMenuSub {
                DropdownMenuSubTrigger { +"More Options" }
                DropdownMenuSubContent {
                    DropdownMenuItem { +"Sub item 1" }
                    DropdownMenuItem { +"Sub item 2" }
                }
            }
        }
    }
}

*/

// ------------------------------
// Root
@JsName("Root")
external val DropdownMenu: ComponentType<DefaultProps>

// ------------------------------
// Trigger
external interface DropdownMenuTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val DropdownMenuTrigger: ComponentType<DropdownMenuTriggerProps>

// ------------------------------
// Content
external interface DropdownMenuContentProps : DefaultProps {
    var side: String?
    var sideOffset: Int?
    var align: String?
    var alignOffset: Int?
    var avoidCollisions: Boolean?
    var collisionPadding: Int?
    var loop: Boolean?
    var onEscapeKeyDown: ((dynamic) -> Unit)?
    var onPointerDownOutside: ((dynamic) -> Unit)?
    var onFocusOutside: ((dynamic) -> Unit)?
    var onInteractOutside: ((dynamic) -> Unit)?
    var asChild: Boolean?
    var forceMount: Boolean?
}

@JsName("Content")
external val DropdownMenuContent: ComponentType<DropdownMenuContentProps>

// ------------------------------
// Item
external interface DropdownMenuItemProps : DefaultProps {
    var disabled: Boolean?
    var asChild: Boolean?
    var inset: Boolean?
    var onSelect: ((dynamic) -> Unit)?
}

@JsName("Item")
external val DropdownMenuItem: ComponentType<DropdownMenuItemProps>

// ------------------------------
// Checkbox Item
external interface DropdownMenuCheckboxItemProps : DefaultProps {
    var checked: Boolean?
    var onCheckedChange: ((Boolean) -> Unit)?
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("CheckboxItem")
external val DropdownMenuCheckboxItem: ComponentType<DropdownMenuCheckboxItemProps>

// ------------------------------
// Radio Group
@JsName("RadioGroup")
external val DropdownMenuRadioGroup: ComponentType<DefaultProps>

// ------------------------------
// Radio Item
external interface DropdownMenuRadioItemProps : DefaultProps {
    var value: String
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("RadioItem")
external val DropdownMenuRadioItem: ComponentType<DropdownMenuRadioItemProps>

// ------------------------------
// Label
external interface DropdownMenuLabelProps : DefaultProps {
    var asChild: Boolean?
    var inset: Boolean?
}

@JsName("Label")
external val DropdownMenuLabel: ComponentType<DropdownMenuLabelProps>

// ------------------------------
// Separator
external interface DropdownMenuSeparatorProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Separator")
external val DropdownMenuSeparator: ComponentType<DropdownMenuSeparatorProps>

// ------------------------------
// Sub
@JsName("Sub")
external val DropdownMenuSub: ComponentType<DefaultProps>

// ------------------------------
// Sub Trigger
external interface DropdownMenuSubTriggerProps : DefaultProps {
    var asChild: Boolean?
    var inset: Boolean?
}

@JsName("SubTrigger")
external val DropdownMenuSubTrigger: ComponentType<DropdownMenuSubTriggerProps>

// ------------------------------
// Sub Content
external interface DropdownMenuSubContentProps : DefaultProps {
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("SubContent")
external val DropdownMenuSubContent: ComponentType<DropdownMenuSubContentProps>

// ------------------------------
// Arrow
external interface DropdownMenuArrowProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Arrow")
external val DropdownMenuArrow: ComponentType<DropdownMenuArrowProps>

// ------------------------------
// Portal
external interface DropdownMenuPortalProps : DefaultProps {
    var forceMount: Boolean?
    var container: dynamic // HTMLElement or null
}

@JsName("Portal")
external val DropdownMenuPortal: ComponentType<DropdownMenuPortalProps>
