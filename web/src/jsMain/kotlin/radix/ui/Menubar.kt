@file:JsModule("@radix-ui/react-menubar")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyMenubar = FC {
    Menubar {
        MenubarMenu {
            MenubarTrigger { +"File" }

            MenubarContent {
                MenubarItem { +"New Tab" }
                MenubarItem { +"New Window" }
                MenubarSeparator()
                MenubarSub {
                    MenubarSubTrigger { +"Export" }
                    MenubarSubContent {
                        MenubarItem { +"PDF" }
                        MenubarItem { +"HTML" }
                    }
                }
            }
        }

        MenubarMenu {
            MenubarTrigger { +"Edit" }

            MenubarContent {
                MenubarCheckboxItem {
                    checked = true
                    +"Show Toolbar"
                }

                MenubarRadioGroup {
                    MenubarLabel { +"View Mode" }
                    MenubarRadioItem {
                        value = "light"
                        +"Light"
                    }
                    MenubarRadioItem {
                        value = "dark"
                        +"Dark"
                    }
                }
            }
        }
    }
}

*/

// ------------------------------
// Root
// ------------------------------
external interface MenubarProps : DefaultProps {
    var orientation: String? // "horizontal" | "vertical"
    var dir: String? // "ltr" | "rtl"
    var loop: Boolean?
}

@JsName("Root")
external val Menubar: ComponentType<MenubarProps>

// ------------------------------
// Menu (per top-level entry)
@JsName("Menu")
external val MenubarMenu: ComponentType<DefaultProps>

// ------------------------------
// Trigger
external interface MenubarTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val MenubarTrigger: ComponentType<MenubarTriggerProps>

// ------------------------------
// Content
external interface MenubarContentProps : DefaultProps {
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
external val MenubarContent: ComponentType<MenubarContentProps>

// ------------------------------
// Item
external interface MenubarItemProps : DefaultProps {
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("Item")
external val MenubarItem: ComponentType<MenubarItemProps>

// ------------------------------
// Checkbox Item
external interface MenubarCheckboxItemProps : DefaultProps {
    var checked: Boolean?
    var onCheckedChange: ((Boolean) -> Unit)?
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("CheckboxItem")
external val MenubarCheckboxItem: ComponentType<MenubarCheckboxItemProps>

// ------------------------------
// Radio Group
@JsName("RadioGroup")
external val MenubarRadioGroup: ComponentType<DefaultProps>

// ------------------------------
// Radio Item
external interface MenubarRadioItemProps : DefaultProps {
    var value: String
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("RadioItem")
external val MenubarRadioItem: ComponentType<MenubarRadioItemProps>

// ------------------------------
// Label
external interface MenubarLabelProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Label")
external val MenubarLabel: ComponentType<MenubarLabelProps>

// ------------------------------
// Separator
external interface MenubarSeparatorProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Separator")
external val MenubarSeparator: ComponentType<MenubarSeparatorProps>

// ------------------------------
// Submenu (group)
@JsName("Sub")
external val MenubarSub: ComponentType<DefaultProps>

// ------------------------------
// SubTrigger
external interface MenubarSubTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("SubTrigger")
external val MenubarSubTrigger: ComponentType<MenubarSubTriggerProps>

// ------------------------------
// SubContent
external interface MenubarSubContentProps : DefaultProps {
    var asChild: Boolean?
    var forceMount: Boolean?
}

@JsName("SubContent")
external val MenubarSubContent: ComponentType<MenubarSubContentProps>

// ------------------------------
// Arrow
external interface MenubarArrowProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Arrow")
external val MenubarArrow: ComponentType<MenubarArrowProps>
