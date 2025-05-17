package bop.ui

import lucide.XIcon
import radix.ui.*
import react.FC
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.span
import web.cssom.ClassName

/**
 * @see radix.ui.DialogRoot
 */
val Dialog = FC<DialogRootProps>("Dialog") { props ->
   radix.ui.DialogRoot {
      dataSlot = "dialog"
      spread(props)
   }
}

/**
 * @see radix.ui.DialogTrigger
 */
val DialogTrigger = FC<DialogTriggerProps>("DialogTrigger") { props ->
   radix.ui.DialogTrigger {
      dataSlot = "dialog-trigger"
      spread(props)
   }
}

/**
 * @see radix.ui.DialogPortal
 */
val DialogPortal = FC<DialogPortalProps>("DialogPortal") { props ->
   radix.ui.DialogPortal {
      dataSlot = "dialog-portal"
      spread(props)
   }
}

/**
 * @see radix.ui.DialogClose
 */
val DialogClose = FC<DialogCloseProps>("DialogClose") { props ->
   radix.ui.DialogClose {
      dataSlot = "dialog-close"
      spread(props)
   }
}

/**
 * @see radix.ui.DialogOverlay
 */
val DialogOverlay = FC<DialogOverlayProps>("DialogOverlay") { props ->
   radix.ui.DialogOverlay {
      dataSlot = "dialog-overlay"
      className = cn(
         "data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50 bg-black/80",
         props.className
      )
      spread(props, "className")
   }
}

/**
 * @see radix.ui.DialogContent
 */
val DialogContent = FC<DialogContentProps>("DialogContent") { props ->
   DialogPortal {
      DialogOverlay {
         radix.ui.DialogContent {
            dataSlot = "dialog-content"
            className = cn(
               "bg-background data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 fixed top-[50%] left-[50%] z-50 grid w-full max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 shadow-lg duration-200 sm:max-w-lg",
               props.className
            )
            spread(props, "className", "children")

            +props.children

            radix.ui.DialogClose {
               className =
                  ClassName("ring-offset-background focus:ring-ring data-[state=open]:bg-accent data-[state=open]:text-muted-foreground absolute top-4 right-4 rounded-xs opacity-70 transition-opacity hover:opacity-100 focus:ring-2 focus:ring-offset-2 focus:outline-hidden disabled:pointer-events-none [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*='size-'])]:size-4")

               XIcon {}
               span {
                  className = ClassName("sr-only")
                  +"Close"
               }
            }
         }
      }
   }
}

val DialogHeader = FC<DefaultProps>("DialogHeader") { props ->
   div {
      dataSlot = "dialog-header"
      className = cn("flex flex-col gap-2 text-center sm:text-left", props.className)
      spread(props, "className")
   }
}

val DialogFooter = FC<DefaultProps>("DialogFooter") { props ->
   div {
      dataSlot = "dialog-footer"
      className = cn("flex flex-col-reverse gap-2 sm:flex-row sm:justify-end", props.className)
      spread(props, "className")
   }
}

/**
 * @see radix.ui.DialogTitle
 */
val DialogTitle = FC<DialogTitleProps>("DialogTitle") { props ->
   radix.ui.DialogTitle {
      dataSlot = "dialog-title"
      className = cn("text-lg leading-none font-semibold", props.className)
      spread(props, "className")
   }
}

/**
 * @see radix.ui.DialogDescription
 */
val DialogDescription = FC<DialogDescriptionProps>("DialogDescription") { props ->
   radix.ui.DialogDescription {
      dataSlot = "dialog-description"
      className = cn("text-muted-foreground text-sm", props.className)
      spread(props, "className")
   }
}
