@file:JsModule("@radix-ui/react-toggle")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

val MyToggle = FC {
    val (pressed, setPressed) = useState(false)

    Toggle {
        this.pressed = pressed
        onPressedChange = setPressed
        className = ClassName("px-3 py-1 rounded border bg-white data-[state=on]:bg-blue-500 text-sm")
        +"Toggle Me"
    }
}

*/

// ------------------------------
// Toggle
external interface ToggleProps : RadixProps {
    var pressed: Boolean?
    var defaultPressed: Boolean?
    var onPressedChange: ((Boolean) -> Unit)?
    var disabled: Boolean?
    var required: Boolean?
    var name: String?
    var value: String?
    var asChild: Boolean?
}

@JsName("Root")
external val Toggle: ComponentType<ToggleProps>
