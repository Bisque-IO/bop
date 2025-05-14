package bop.ui

import bop.ui.utils.CVAConfig
import bop.ui.utils.cva

import bop.ui.utils.cn
import bop.ui.utils.spread
import bop.ui.utils.set

import radix.ui.Slot
import react.FC
import react.dom.html.HTMLAttributes
import react.dom.html.ReactHTML.button
import web.html.HTMLButtonElement

val buttonVariants = cva(
    base = "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg:not([class*='size-'])]:size-4 shrink-0 [&_svg]:shrink-0 outline-none focus-visible:border-ring focus-visible:ring-ring/50 focus-visible:ring-[3px] aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
    config = js("""{
    "variants": {
        "variant": {
            "default": "bg-primary text-primary-foreground shadow-xs hover:bg-primary/90",
            "destructive": "bg-destructive text-white shadow-xs hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40 dark:bg-destructive/60",
            "outline": "border bg-background shadow-xs hover:bg-accent hover:text-accent-foreground dark:bg-input/30 dark:border-input dark:hover:bg-input/50",
            "secondary": "bg-secondary text-secondary-foreground shadow-xs hover:bg-secondary/80",
            "ghost": "hover:bg-accent hover:text-accent-foreground dark:hover:bg-accent/50",
            "link": "text-primary underline-offset-4 hover:underline"
        }, "size": {
            "default": "h-9 px-4 py-2 has-[>svg]:px-3",
            "sm": "h-8 rounded-md gap-1.5 px-3 has-[>svg]:px-2.5",
            "lg": "h-10 rounded-md px-6 has-[>svg]:px-4",
            "icon": "size-9"
        },
    }, "defaultVariants": {
        "variant": "default", "size": "default"
    }
}""").unsafeCast<CVAConfig>()
)

external interface ButtonProps : HTMLAttributes<HTMLButtonElement> {
    var variant: String?
    var size: String?
    var asChild: Boolean?
}

val exclude = setOf("variant", "size", "asChild", "className")

val Button = FC<ButtonProps> { props ->
    val component = if (props.asChild == true) Slot else button

    val variants = js("{}")
    variants["variant"] = props.variant ?: "default"
    variants["size"] = props.size ?: "default"
    variants["className"] = props.className ?: "default"

    component {
        this["data-slot"] = "button"
        className = cn(buttonVariants(variants))
        spread(props, exclude)
    }
}.apply { displayName = "Button" }
