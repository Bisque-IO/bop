@file:JsModule("@radix-ui/react-dialog")
@file:JsNonModule

package lib.radix

import react.ComponentType

/*

val MySheet = FC {
    val (open, setOpen) = useState(false)

    Sheet {
        this.open = open
        onOpenChange = setOpen

        SheetTrigger {
            button {
                +"Open Sheet"
            }
        }

        SheetContent {
            side = "right"
            className = ClassName("w-64 p-4 bg-white shadow-lg")

            SheetHeader {
                SheetTitle { +"Sheet Title" }
                SheetDescription { +"This is a description of the sheet." }
            }

            div {
                +"Sheet content goes here."
            }

            SheetFooter {
                SheetClose {
                    button {
                        +"Close"
                    }
                }
            }
        }
    }
}

*/

// ------------------------------
// Sheet Root
external interface SheetProps : RadixProps {
    var open: Boolean?
    var defaultOpen: Boolean?
    var onOpenChange: ((Boolean) -> Unit)?
    var modal: Boolean?
    var asChild: Boolean?
}

@JsName("Root")
external val Sheet: ComponentType<SheetProps>

// ------------------------------
// Sheet Trigger
external interface SheetTriggerProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Trigger")
external val SheetTrigger: ComponentType<SheetTriggerProps>

// ------------------------------
// Sheet Content
external interface SheetContentProps : RadixProps {
    var side: String? // "top" | "right" | "bottom" | "left"
    var asChild: Boolean?
}

@JsName("Content")
external val SheetContent: ComponentType<SheetContentProps>

// ------------------------------
// Sheet Header
external interface SheetHeaderProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Header")
external val SheetHeader: ComponentType<SheetHeaderProps>

// ------------------------------
// Sheet Title
external interface SheetTitleProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Title")
external val SheetTitle: ComponentType<SheetTitleProps>

// ------------------------------
// Sheet Description
external interface SheetDescriptionProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Description")
external val SheetDescription: ComponentType<SheetDescriptionProps>

// ------------------------------
// Sheet Footer
external interface SheetFooterProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Footer")
external val SheetFooter: ComponentType<SheetFooterProps>

// ------------------------------
// Sheet Close
external interface SheetCloseProps : RadixProps {
    var asChild: Boolean?
}

@JsName("Close")
external val SheetClose: ComponentType<SheetCloseProps>
