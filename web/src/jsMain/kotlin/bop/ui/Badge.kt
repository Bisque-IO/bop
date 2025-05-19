package bop.ui

import js.objects.unsafeJso
import lib.cva.cva
import lib.radix.Slot
import react.FC
import react.dom.html.ReactHTML.span

val badgeVariants = cva(
   "",
   unsafeJso {
      variants = unsafeJso {
         variant = unsafeJso {
            default = "border-transparent bg-primary text-primary-foreground [a&]:hover:bg-primary/90"
            secondary = "border-transparent bg-secondary text-secondary-foreground [a&]:hover:bg-secondary/90"
            destructive =
               "border-transparent bg-destructive text-white [a&]:hover:bg-destructive/90 focus-visible:ring-destructive/20 dark:focus-visible:ring-destructive/40"
            outline = "text-foreground [a&]:hover:bg-accent [a&]:hover:text-accent-foreground"
         }
      }
      defaultVariants = unsafeJso { variant = "default" }
   },
)

external interface BadgeProps : DefaultProps {
   var variant: String?
   var asChild: Boolean?
}

val Badge = FC<BadgeProps>("Badge") { props ->
   val component = if (props.asChild == true) Slot else span
   component {
      +props
      dataSlot = "badge"
      className = cn(badgeVariants(unsafeJso { variant = props.variant }, props.className))
   }
}
