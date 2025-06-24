package bop.ui

import lib.radix.*
import react.FC

val ScrollArea = FC<ScrollAreaRootProps>("ScrollArea") { props ->
   ScrollAreaPrimitiveRoot {
      +props
      dataSlot = "scroll-area"
      className = cn("relative", props.className)

      ScrollAreaPrimitiveViewport {
         dataSlot = "scroll-area-viewport"
         className =
            cn("focus-visible:ring-ring/50 size-full rounded-[inherit] transition-[color,box-shadow] outline-none focus-visible:ring-[3px] focus-visible:outline-1")

         +props.children
      }

      ScrollBar {}
      ScrollAreaPrimitiveCorner {}
   }
}

val ScrollBar = FC<ScrollAreaScrollbarProps>("ScrollBar") { props ->
   ScrollAreaPrimitiveScrollbar {
      +props
      dataSlot = "scroll-area-scrollbar"
      orientation = props.orientation ?: "vertical"
      className = cn(
         "flex touch-none p-px transition-colors select-none",
         when (orientation) {
            "vertical" -> "h-full w-2.5 border-l border-l-transparent"
            "horizontal" -> "h-2.5 flex-col border-t border-t-transparent"
            else -> ""
         },
         props.className
      )

      ScrollAreaPrimitiveThumb {
         dataSlot = "scroll-area-thumb"
         className = cn("bg-border relative flex-1 rounded-full")
      }
   }
}