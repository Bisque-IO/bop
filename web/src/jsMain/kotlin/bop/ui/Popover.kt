package bop.ui

import lib.radix.*
import react.FC

val Popover = FC<PopoverRootProps>("Popover") { props ->
   PopoverPrimitiveRoot {
      spread(props)
      dataSlot = "popover"
   }
}

val PopoverTrigger = FC<PopoverTriggerProps>("PopoverTrigger") { props ->
   PopoverPrimitiveTrigger {
      spread(props)
      dataSlot = "popover-trigger"
   }
}

val PopoverContent = FC<PopoverContentProps>("PopoverContent") { props ->
   PopoverPrimitivePortal {
      PopoverPrimitiveContent {
         spread(props, "className")
         dataSlot = "popover-content"
         align = props.align ?: "center"
         sideOffset = props.sideOffset ?: 4
         className = cn(
            "bg-popover text-popover-foreground data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 z-50 w-72 origin-(--radix-popover-content-transform-origin) rounded-md border p-4 shadow-md outline-hidden",
            props.className
         )
      }
   }
}

val PopoverAnchor = FC<PopoverAnchorProps>("PopoverAnchor") { props ->
   PopoverPrimitiveAnchor {
      spread(props)
      dataSlot = "popover-anchor"
   }
}