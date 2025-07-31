package bop.ui

import js.objects.unsafeJso
import lib.radix.*
import react.FC
import react.dom.html.ReactHTML.div

val AlertDialog = FC<DefaultProps>("AlertDialog") { props ->
   AlertDialogPrimitiveRoot {
      +props
      dataSlot = "alert-dialog"
   }
}

val AlertDialogTrigger = FC<DefaultProps>("AlertDialogTrigger") { props ->
   AlertDialogPrimitiveTrigger {
      +props
      dataSlot = "alert-dialog-trigger"
   }
}

val AlertDialogPortal = FC<AlertDialogPortalProps>("AlertDialogPortal") { props ->
   AlertDialogPrimitivePortal {
      +props
      dataSlot = "alert-dialog-portal"
   }
}

val AlertDialogOverlay = FC<AlertDialogOverlayProps>("AlertDialogOverlay") { props ->
   AlertDialogPrimitiveOverlay {
      +props
      dataSlot = "alert-dialog-overlay"
      className = cn(
         "data-[state=open]:animate-in data-[state=closed]:animate-out " + "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50 bg-black/50",
         props.className,
      )
   }
}

val AlertDialogContent = FC<AlertDialogContentProps>("AlertDialogContent") { props ->
   AlertDialogPrimitivePortal {
      AlertDialogOverlay {}
      AlertDialogPrimitiveContent {
         +props
         dataSlot = "alert-dialog-content"
         className = cn(
            "bg-background data-[state=open]:animate-in data-[state=closed]:animate-out " + "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 " + "data-[state=open]:zoom-in-95 fixed top-[50%] left-[50%] z-50 grid w-full " + "max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 " + "shadow-lg duration-200 sm:max-w-lg",
            props.className,
         )
      }
   }
}

val AlertDialogHeader = FC<DefaultProps>("AlertDialogHeader") { props ->
   div {
      +props
      dataSlot = "alert-dialog-header"
      className = cn("flex flex-col gap-2 text-center sm:text-left", props.className)
   }
}

val AlertDialogFooter = FC<DefaultProps>("AlertDialogFooter") { props ->
   div {
      +props
      dataSlot = "alert-dialog-footer"
      className = cn("flex flex-col-reverse gap-2 sm:flex-row sm:justify-end", props.className)
   }
}

val AlertDialogTitle = FC<AlertDialogTitleProps>("AlertDialogTitle") { props ->
   AlertDialogPrimitiveTitle {
      +props
      dataSlot = "alert-dialog-title"
      className = cn("text-lg font-semibold", props.className)
   }
}

val AlertDialogDescription = FC<AlertDialogDescriptionProps>("AlertDialogDescription") { props ->
   AlertDialogPrimitiveDescription {
      +props
      dataSlot = "alert-dialog-description"
      className = cn("text-muted-foreground text-sm", props.className)
   }
}

val AlertDialogAction = FC<AlertDialogActionProps>("AlertDialogAction") { props ->
   AlertDialogPrimitiveAction {
      +props
      className = cn(buttonVariants(), props.className)
   }
}

val AlertDialogCancel = FC<AlertDialogCancelProps>("AlertDialogCancel") { props ->
   AlertDialogPrimitiveCancel {
      +props
      className = cn(buttonVariants(unsafeJso { variant = "outline" }), props.className)
   }
}
