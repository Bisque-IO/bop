@file:JsModule("@radix-ui/react-dialog")
@file:JsNonModule

package radix.ui

import react.ComponentType
import web.events.Event

/*

val ControlledDialog = FC {
    val (isOpen, setOpen) = useState(false)

    DialogRoot {
        open = isOpen
        onOpenChange = { setOpen(it) }

        DialogTrigger {
            button {
                +"Open Dialog"
            }
        }

        DialogPortal {
            DialogOverlay {
                className = ClassName("fixed inset-0 bg-black bg-opacity-50")
            }

            DialogContent {
                className = ClassName("fixed top-1/2 left-1/2 bg-white p-6 rounded-md transform -translate-x-1/2 -translate-y-1/2 w-[300px]")

                DialogTitle {
                    className = ClassName("text-lg font-bold mb-2")
                    +"Controlled Dialog"
                }

                DialogDescription {
                    className = ClassName("mb-4")
                    +"Dialog is open: $isOpen"
                }

                DialogClose {
                    button {
                        className = ClassName("mt-2 bg-blue-500 text-white px-4 py-2 rounded")
                        +"Close"
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
external interface DialogRootProps : DefaultProps {
    var open: Boolean?
    var defaultOpen: Boolean?
    var onOpenChange: ((Boolean) -> Unit)?
    var modal: Boolean? // Optional in Radix v1+
}

// Root Dialog component
@JsName("Root")
external val DialogRoot: ComponentType<DialogRootProps>

// ------------------------------
// Trigger
// ------------------------------
external interface DialogTriggerProps : DefaultProps {
    var asChild: Boolean?
}

// Trigger (e.g., button to open)
@JsName("Trigger")
external val DialogTrigger: ComponentType<DialogTriggerProps>

// ------------------------------
// Portal
// ------------------------------
external interface DialogPortalProps : DefaultProps {
    var forceMount: Boolean?
    var container: dynamic // HTMLElement or null
}

// Portal (optional wrapper for rendering in body)
@JsName("Portal")
external val DialogPortal: ComponentType<DialogPortalProps>

// ------------------------------
// Overlay
// ------------------------------
external interface DialogOverlayProps : DefaultProps {
    var asChild: Boolean?
    var forceMount: Boolean?
}

// Overlay (background)
@JsName("Overlay")
external val DialogOverlay: ComponentType<DialogOverlayProps>

// ------------------------------
// Content
// ------------------------------
external interface DialogContentProps : DefaultProps {
    var asChild: Boolean?
    var forceMount: Boolean?
    var onEscapeKeyDown: ((Event) -> Unit)?
    var onPointerDownOutside: ((Event) -> Unit)?
    var onFocusOutside: ((Event) -> Unit)?
    var onInteractOutside: ((Event) -> Unit)?
}

// Content (the dialog itself)
@JsName("Content")
external val DialogContent: ComponentType<DialogContentProps>

// ------------------------------
// Title
// ------------------------------
external interface DialogTitleProps : DefaultProps {
    var asChild: Boolean?
}

// Title and Description
@JsName("Title")
external val DialogTitle: ComponentType<DialogTitleProps>

// ------------------------------
// Description
// ------------------------------
external interface DialogDescriptionProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Description")
external val DialogDescription: ComponentType<DialogDescriptionProps>

// ------------------------------
// Close
// ------------------------------
external interface DialogCloseProps : DefaultProps {
    var asChild: Boolean?
}

// Close button
@JsName("Close")
external val DialogClose: ComponentType<DialogCloseProps>
