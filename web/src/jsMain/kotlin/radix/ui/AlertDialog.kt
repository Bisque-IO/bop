@file:JsModule("@radix-ui/react-alert-dialog")
@file:JsNonModule

package radix.ui

import react.ComponentType

/*

val MyAlertDialog = FC {
    AlertDialog {
        AlertDialogTrigger {
            button { +"Delete Item" }
        }

        AlertDialogPortal {
            AlertDialogOverlay {
                className = ClassName("fixed inset-0 bg-black/40")
            }

            AlertDialogContent {
                className = ClassName("fixed top-1/2 left-1/2 bg-white p-6 rounded shadow transform -translate-x-1/2 -translate-y-1/2 w-[90%] max-w-md")

                AlertDialogTitle { +"Are you sure?" }
                AlertDialogDescription {
                    +"This action cannot be undone."
                }

                div {
                    className = ClassName("flex justify-end gap-2 mt-4")

                    AlertDialogCancel {
                        button {
                            className = ClassName("px-4 py-2 border rounded")
                            +"Cancel"
                        }
                    }

                    AlertDialogAction {
                        button {
                            className = ClassName("px-4 py-2 bg-red-600 text-white rounded")
                            +"Delete"
                        }
                    }
                }
            }
        }
    }
}

*/

// ------------------------------
// Root
@JsName("Root")
external val AlertDialog: ComponentType<DefaultProps>

// ------------------------------
// Trigger
external interface AlertDialogTriggerProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val AlertDialogTrigger: ComponentType<AlertDialogTriggerProps>

// ------------------------------
// Portal
external interface AlertDialogPortalProps : DefaultProps {
    var container: dynamic
    var forceMount: Boolean?
}

@JsName("Portal")
external val AlertDialogPortal: ComponentType<AlertDialogPortalProps>

// ------------------------------
// Overlay
external interface AlertDialogOverlayProps : DefaultProps {
    var asChild: Boolean?
    var forceMount: Boolean?
}

@JsName("Overlay")
external val AlertDialogOverlay: ComponentType<AlertDialogOverlayProps>

// ------------------------------
// Content
external interface AlertDialogContentProps : DefaultProps {
    var asChild: Boolean?
    var forceMount: Boolean?
    var onEscapeKeyDown: ((dynamic) -> Unit)?
    var onPointerDownOutside: ((dynamic) -> Unit)?
    var onFocusOutside: ((dynamic) -> Unit)?
    var onInteractOutside: ((dynamic) -> Unit)?
}

@JsName("Content")
external val AlertDialogContent: ComponentType<AlertDialogContentProps>

// ------------------------------
// Title
external interface AlertDialogTitleProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Title")
external val AlertDialogTitle: ComponentType<AlertDialogTitleProps>

// ------------------------------
// Description
external interface AlertDialogDescriptionProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Description")
external val AlertDialogDescription: ComponentType<AlertDialogDescriptionProps>

// ------------------------------
// Cancel
external interface AlertDialogCancelProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Cancel")
external val AlertDialogCancel: ComponentType<AlertDialogCancelProps>

// ------------------------------
// Action
external interface AlertDialogActionProps : DefaultProps {
    var asChild: Boolean?
}

@JsName("Action")
external val AlertDialogAction: ComponentType<AlertDialogActionProps>
