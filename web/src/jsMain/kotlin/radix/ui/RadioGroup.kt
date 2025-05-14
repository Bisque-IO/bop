@file:JsModule("@radix-ui/react-radio-group")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyRadioGroup = FC {
    val (selected, setSelected) = useState("option1")

    RadioGroup {
        value = selected
        onValueChange = setSelected

        RadioGroupItem {
            value = "option1"
            + "Option 1"
            RadioGroupIndicator {
                +"•"
            }
        }

        RadioGroupItem {
            value = "option2"
            + "Option 2"
            RadioGroupIndicator {
                +"•"
            }
        }
    }
}

*/

// ------------------------------
// Root
// ------------------------------
external interface RadioGroupProps : DefaultProps {
    var value: String?
    var defaultValue: String?
    var onValueChange: ((String) -> Unit)?
    var name: String?
    var required: Boolean?
    var disabled: Boolean?
    var orientation: String? // "horizontal" | "vertical"
    var dir: String?         // "ltr" | "rtl"
    var asChild: Boolean?
}

@JsName("Root")
external val RadioGroup: ComponentType<RadioGroupProps>

// ------------------------------
// Item (Radio Button)
// ------------------------------
external interface RadioGroupItemProps : DefaultProps {
    var value: String
    var disabled: Boolean?
    var required: Boolean?
    var id: String?
    var asChild: Boolean?
}

@JsName("Item")
external val RadioGroupItem: ComponentType<RadioGroupItemProps>

// ------------------------------
// Indicator (the inner dot)
// ------------------------------
external interface RadioGroupIndicatorProps : DefaultProps {
    var asChild: Boolean?
    var forceMount: Boolean?
}

@JsName("Indicator")
external val RadioGroupIndicator: ComponentType<RadioGroupIndicatorProps>
