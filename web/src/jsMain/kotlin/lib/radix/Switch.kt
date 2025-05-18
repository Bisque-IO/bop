@file:JsModule("@radix-ui/react-switch")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

val MySwitch = FC {
    val (on, setOn) = useState(false)

    Switch {
        checked = on
        onCheckedChange = setOn

        SwitchThumb {
            // style or content here
        }
    }
}

*/

// ------------------------------
// Switch Root
// ------------------------------
external interface SwitchProps : RadixProps {
    var checked: Boolean?
    var defaultChecked: Boolean?
    var onCheckedChange: ((Boolean) -> Unit)?
    var disabled: Boolean?
    var required: Boolean?
    var name: String?
    var value: String?
    var id: String?
    var asChild: Boolean?
}

@JsName("Root")
external val Switch: ComponentType<SwitchProps>

// ------------------------------
// Switch Thumb
// ------------------------------
external interface SwitchThumbProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Thumb")
external val SwitchThumb: ComponentType<SwitchThumbProps>
