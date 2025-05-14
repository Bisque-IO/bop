package bop.ui

import radix.ui.AlertDialogOverlayProps
import react.*

import bop.ui.utils.cn
import bop.ui.utils.spread
import bop.ui.utils.set
import radix.ui.AlertDialogActionProps
import radix.ui.AlertDialogCancelProps

import radix.ui.AlertDialogContentProps
import radix.ui.AlertDialogDescriptionProps
import radix.ui.AlertDialogTitleProps
import radix.ui.DefaultProps
import react.dom.html.ReactHTML.div


val AlertDialogOverlay = FC<AlertDialogOverlayProps> { props ->
    radix.ui.AlertDialogOverlay {
        this["data-slot"] = "alert-dialog-overlay"
        className = cn(
            "data-[state=open]:animate-in data-[state=closed]:animate-out " +
                    "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 fixed inset-0 z-50 bg-black/50", props.className)
        spread(props, "className")
    }
}

val AlertDialogContent = FC<AlertDialogContentProps> { props ->
    radix.ui.AlertDialogPortal {
        AlertDialogOverlay {}
        radix.ui.AlertDialogContent {
            this["data-slot"] = "alert-dialog-content"
            className = cn(
                "bg-background data-[state=open]:animate-in data-[state=closed]:animate-out " +
                        "data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 " +
                        "data-[state=open]:zoom-in-95 fixed top-[50%] left-[50%] z-50 grid w-full " +
                        "max-w-[calc(100%-2rem)] translate-x-[-50%] translate-y-[-50%] gap-4 rounded-lg border p-6 " +
                        "shadow-lg duration-200 sm:max-w-lg",
                props.className
            )
            spread(props, "className")
        }
    }
}

val AlertDialogHeader = FC<DefaultProps> { props ->
    div {
        this["data-slot"] = "alert-dialog-header"
        className = cn("flex flex-col gap-2 text-center sm:text-left", props.className)
        spread(props, "className")
    }
}

val AlertDialogFooter = FC<DefaultProps> { props ->
    div {
        this["data-slot"] = "alert-dialog-footer"
        className = cn(
            "flex flex-col-reverse gap-2 sm:flex-row sm:justify-end",
            props.className
        )
        spread(props, "className")
//        +props.children
    }
}

val AlertDialogTitle = FC<AlertDialogTitleProps> { props ->
    radix.ui.AlertDialogTitle {
        this["data-slot"] = "alert-dialog-title"
        className = cn("text-lg font-semibold", props.className)
        spread(props, "className")
    }
}

val AlertDialogDescription = FC<AlertDialogDescriptionProps> { props ->
    radix.ui.AlertDialogDescription {
        this["data-slot"] = "alert-dialog-description"
        className = cn("text-muted-foreground text-sm", props.className)
        spread(props, "className")
    }
}

val AlertDialogAction = FC<AlertDialogActionProps> { props ->
    radix.ui.AlertDialogAction {
        className = cn(buttonVariants(), props.className)
        spread(props, "className")
    }
}

val AlertDialogCancel = FC<AlertDialogCancelProps> { props ->
    radix.ui.AlertDialogCancel {
        className = cn(buttonVariants(mapOf("variant" to "outline")), props.className)
        spread(props, "className")
    }
}
