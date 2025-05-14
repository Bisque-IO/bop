@file:JsModule("@radix-ui/react-label")
@file:JsNonModule

package radix.ui

import react.*

/*

val MyLabelExample = FC {
    Label {
        htmlFor = "email"
        className = "block mb-1 font-semibold"
        +"Email Address"
    }

    input {
        id = "email"
        type = "email"
        className = "border p-2 w-full"
    }
}

*/

// ------------------------------
// Label
// ------------------------------
external interface LabelProps : DefaultProps {
    var htmlFor: String?
    var asChild: Boolean?
}

@JsName("Root")
external val Label: ComponentType<LabelProps>
