package bop.ui

import lib.radix.*
import react.FC

val HoverCard = FC<HoverCardRootProps>("hoverCard") { props ->
   HoverCardPrimitiveRoot {
      +props
      dataSlot = "hover-card"
   }
}

val HoverCardTrigger = FC<HoverCardTriggerProps>("hoverCardTrigger") { props ->
   HoverCardPrimitiveTrigger {
      +props
      dataSlot = "hover-card-trigger"
   }
}

private val EXCLUDE_CONTENT_PROPS = setOf("align", "sideOffset", "className")

val HoverCardContent = FC<HoverCardContentProps>("hoverCardContent") { props ->
   HoverCardPrimitivePortal {
      dataSlot = "hover-card-portal"
      HoverCardPrimitiveContent {
         spread(props, EXCLUDE_CONTENT_PROPS)
         dataSlot = "hover-card-content"
         align = props.align ?: "center"
         sideOffset = props.sideOffset ?: 4
         className = cn(
            "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 w-64 origin-(--radix-hover-card-content-transform-origin) rounded-md border p-4 shadow-md outline-hidden",
            props.className
         )
      }
   }
}
