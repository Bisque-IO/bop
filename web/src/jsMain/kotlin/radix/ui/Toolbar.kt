@file:JsModule("@radix-ui/react-toolbar")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyToolbar = FC {
    Toolbar {
        className = ClassName("flex gap-2 p-2 bg-gray-100")

        ToolbarButton {
            className = ClassName("px-2 py-1 border rounded")
            +"Button"
        }

        ToolbarSeparator {
            orientation = "vertical"
            className = ClassName("w-px bg-gray-300")
        }

        ToolbarToggleGroup {
            type = "single"
            defaultValue = "bold"

            ToolbarToggleItem {
                value = "bold"
                +"B"
            }

            ToolbarToggleItem {
                value = "italic"
                +"I"
            }
        }
    }
}

*/

// ------------------------------
// Toolbar Root
external interface ToolbarProps : DefaultProps {
    var orientation: String? // "horizontal" | "vertical"
    var dir: String? // "ltr" | "rtl"
    var loop: Boolean?
    var asChild: Boolean?
}

@JsName("Root")
external val Toolbar: ComponentType<ToolbarProps>

// ------------------------------
// Toolbar Button
external interface ToolbarButtonProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Button")
external val ToolbarButton: ComponentType<ToolbarButtonProps>

// ------------------------------
// Toolbar Link
external interface ToolbarLinkProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Link")
external val ToolbarLink: ComponentType<ToolbarLinkProps>

// ------------------------------
// Toolbar Separator
external interface ToolbarSeparatorProps : DefaultProps {
    var orientation: String? // "horizontal" | "vertical"
    var decorative: Boolean?
    var asChild: Boolean?
}

@JsName("Separator")
external val ToolbarSeparator: ComponentType<ToolbarSeparatorProps>

// ------------------------------
// Toolbar Toggle Group
external interface ToolbarToggleGroupProps : DefaultProps {
    var type: String // "single" | "multiple"
    var value: dynamic
    var defaultValue: dynamic
    var onValueChange: ((dynamic) -> Unit)?
    var rovingFocus: Boolean?
    var orientation: String?
    var dir: String?
    var loop: Boolean?
    var asChild: Boolean?
}

@JsName("ToggleGroup")
external val ToolbarToggleGroup: ComponentType<ToolbarToggleGroupProps>

// ------------------------------
// Toolbar Toggle Item
external interface ToolbarToggleItemProps : DefaultProps {
    var value: String
    var pressed: Boolean?
    var defaultPressed: Boolean?
    var onPressedChange: ((Boolean) -> Unit)?
    var asChild: Boolean?
}

@JsName("ToggleItem")
external val ToolbarToggleItem: ComponentType<ToolbarToggleItemProps>
