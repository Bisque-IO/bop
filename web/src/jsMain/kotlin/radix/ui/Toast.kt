@file:JsModule("@radix-ui/react-toast")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyToastExample = FC {
    val (open, setOpen) = useState(false)

    ToastProvider {
        Toast {
            this.open = open
            onOpenChange = setOpen

            ToastTitle { +"Success!" }
            ToastDescription { +"Your settings were saved." }
            ToastAction {
                altText = "Undo"
                button { +"Undo" }
            }
            ToastClose {
                button { +"Close" }
            }
        }

        ToastViewport {
            className = "fixed bottom-0 right-0 p-4"
        }
    }

    button {
        onClick = { setOpen(true) }
        +"Show Toast"
    }
}

*/

// ------------------------------
// Provider
// ------------------------------
external interface ToastProviderProps : DefaultProps {
    var duration: Int?
    var swipeDirection: String? // "up", "down", "left", "right"
    var swipeThreshold: Int?
    var label: String?
}

@JsName("Provider")
external val ToastProvider: ComponentType<ToastProviderProps>

// ------------------------------
// Viewport
// ------------------------------
external interface ToastViewportProps : DefaultProps {
    var asChild: Boolean?
    var hotkey: Array<String>?
}

@JsName("Viewport")
external val ToastViewport: ComponentType<ToastViewportProps>

// ------------------------------
// Root (individual toast)
// ------------------------------
external interface ToastProps : DefaultProps {
    var type: String? // "foreground" | "background"
    var open: Boolean?
    var defaultOpen: Boolean?
    var onOpenChange: ((Boolean) -> Unit)?
    var duration: Int?
}

@JsName("Root")
external val Toast: ComponentType<ToastProps>

// ------------------------------
// Title
// ------------------------------
external interface ToastTitleProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Title")
external val ToastTitle: ComponentType<ToastTitleProps>

// ------------------------------
// Description
// ------------------------------
external interface ToastDescriptionProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Description")
external val ToastDescription: ComponentType<ToastDescriptionProps>

// ------------------------------
// Action
// ------------------------------
external interface ToastActionProps : DefaultProps {
    var altText: String
    var asChild: Boolean?
}

@JsName("Action")
external val ToastAction: ComponentType<ToastActionProps>

// ------------------------------
// Close
// ------------------------------
external interface ToastCloseProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Close")
external val ToastClose: ComponentType<ToastCloseProps>
