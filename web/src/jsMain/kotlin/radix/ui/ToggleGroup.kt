@file:JsModule("@radix-ui/react-toggle-group")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyToggleGroup = FC {
    val (value, setValue) = useState("bold")

    ToggleGroup {
        type = "single"
        this.value = value
        onValueChange = setValue

        ToggleGroupItem {
            value = "bold"
            +"B"
        }

        ToggleGroupItem {
            value = "italic"
            +"I"
        }

        ToggleGroupItem {
            value = "underline"
            +"U"
        }
    }
}

*/

// ------------------------------
// ToggleGroup Root
external interface ToggleGroupProps : DefaultProps {
    var type: String // "single" | "multiple"
    var value: dynamic
    var defaultValue: dynamic
    var onValueChange: ((dynamic) -> Unit)?
    var rovingFocus: Boolean?
    var orientation: String? // "horizontal" | "vertical"
    var dir: String? // "ltr" | "rtl"
    var loop: Boolean?
    var disabled: Boolean?
    var asChild: Boolean?
}

@JsName("Root")
external val ToggleGroup: ComponentType<ToggleGroupProps>

// ------------------------------
// ToggleGroup Item
external interface ToggleGroupItemProps : DefaultProps {
    var value: String
    var disabled: Boolean?
    var pressed: Boolean?
    var defaultPressed: Boolean?
    var onPressedChange: ((Boolean) -> Unit)?
    var asChild: Boolean?
}

@JsName("Item")
external val ToggleGroupItem: ComponentType<ToggleGroupItemProps>
