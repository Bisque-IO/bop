package bop.ui

import lib.lucide.GripVerticalIcon
import react.FC
import react.createElement
import react.dom.html.ReactHTML.div

external interface ResizablePanelGroupProps : lib.resizable.PanelGroupProps

val ResizablePanelGroup = FC<ResizablePanelGroupProps> { props ->
   lib.resizable.PanelGroup {
      +props
      dataSlot = "resizable-panel-group"
      className = cn("flex h-full w-full data-[panel-group-direction=vertical]:flex-col", props.className)
   }
}

external interface ResizablePanelProps : lib.resizable.PanelProps

val ResizablePanel = FC<ResizablePanelProps> { props ->
   lib.resizable.Panel {
      +props
      dataSlot = "resizable-panel"
   }
}

external interface ResizableHandleProps : lib.resizable.PanelResizeHandleProps {
   var withHandle: Boolean?
}

val ResizableHandle = FC<ResizableHandleProps> { props ->
   lib.resizable.PanelResizeHandle {
      +props
      dataSlot = "resizable-handle"
      className = cn("bg-border focus-visible:ring-ring relative flex w-px items-center justify-center after:absolute after:inset-y-0 after:left-1/2 after:w-1 after:-translate-x-1/2 focus-visible:ring-1 focus-visible:ring-offset-1 focus-visible:outline-hidden data-[panel-group-direction=vertical]:h-px data-[panel-group-direction=vertical]:w-full data-[panel-group-direction=vertical]:after:left-0 data-[panel-group-direction=vertical]:after:h-1 data-[panel-group-direction=vertical]:after:w-full data-[panel-group-direction=vertical]:after:-translate-y-1/2 data-[panel-group-direction=vertical]:after:translate-x-0 [&[data-panel-group-direction=vertical]>div]:rotate-90", props.className)

      if (props.withHandle == true) {
         div {
            className = cn("bg-border z-10 flex h-4 w-3 items-center justify-center rounded-xs border")
            GripVerticalIcon { className = cn("size-2.5") }
         }
      }
   }
}
