@file:JsModule("@radix-ui/react-checkbox")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyCheckbox = FC {
    val (checked, setChecked) = useState(false)

    Checkbox {
        this.checked = checked
        onCheckedChange = setChecked

        CheckboxIndicator {
            +"✓"
        }
    }
}

*/

// ------------------------------
// Root (Checkbox)
// ------------------------------
external interface CheckboxProps : DefaultProps {
    var checked: dynamic /* Boolean | "indeterminate" */
    var defaultChecked: Boolean?
    var onCheckedChange: ((dynamic) -> Unit)?
    var disabled: Boolean?
    var required: Boolean?
    var name: String?
    var value: String?
    var id: String?
    var asChild: Boolean?
}

@JsName("Root")
external val Checkbox: ComponentType<CheckboxProps>

// ------------------------------
// Indicator (Checkmark or custom icon)
// ------------------------------
external interface CheckboxIndicatorProps : DefaultProps {
    var forceMount: Boolean?
    var asChild: Boolean?
}

@JsName("Indicator")
external val CheckboxIndicator: ComponentType<CheckboxIndicatorProps>
