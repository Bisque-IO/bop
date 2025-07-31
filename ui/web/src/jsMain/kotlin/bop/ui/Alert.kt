package bop.ui

import js.objects.unsafeJso
import lib.cva.cva
import react.FC
import react.PropsWithChildren
import react.PropsWithClassName
import react.dom.aria.AriaRole
import react.dom.html.ReactHTML.div

val alertVariants = cva(
   "relative w-full rounded-lg border px-4 py-3 text-sm grid has-[>svg]:grid-cols-[calc(var(--spacing)*4)_1fr] grid-cols-[0_1fr] has-[>svg]:gap-x-3 gap-y-0.5 items-start [&>svg]:size-4 [&>svg]:translate-y-0.5 [&>svg]:text-current",
   unsafeJso {
      variants = unsafeJso {
         variant = unsafeJso {
            default = "bg-background text-foreground"
            destructive =
               "text-destructive-foreground [&>svg]:text-current *:data-[slot=alert-description]:text-destructive-foreground/80"
         }
      }
      defaultVariants = unsafeJso { variant = "default" }
   },
)

external interface AlertProps : DefaultProps, PropsWithChildren, PropsWithClassName {
   var variant: String?
}

val Alert = FC<AlertProps>("Alert") { props ->
   div {
      +props
      dataSlot = "alert"
      role = AriaRole.alert
      className = cn(alertVariants(unsafeJso { variant = props.variant ?: "default" }), props.className)
   }
}

val AlertTitle = FC<DefaultProps>("AlertTitle") { props ->
   div {
      +props
      dataSlot = "alert-title"
      className = cn("col-start-2 line-clamp-1 min-h-4 font-medium tracking-tight", props.className)
   }
}

val AlertDescription = FC<DefaultProps>("AlertDescription") { props ->
   div {
      +props
      dataSlot = "alert-description"
      className = cn(
         "text-muted-foreground col-start-2 grid justify-items-start gap-1 text-sm [&_p]:leading-relaxed",
         props.className,
      )
   }
}
