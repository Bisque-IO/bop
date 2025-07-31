package bop.demo

import bop.ui.Button
import bop.ui.Input
import bop.ui.Label
import bop.ui.Popover
import bop.ui.PopoverContent
import bop.ui.PopoverTrigger
import bop.ui.cn
import react.FC
import react.dom.html.ReactHTML.div
import react.dom.html.ReactHTML.h4
import react.dom.html.ReactHTML.p
import web.dom.ElementId

val PopoverDemo = FC {
   Popover {
      PopoverTrigger {
         asChild = true
         Button {
            variant = "outline"
            +"Open popover"
         }
      }
      PopoverContent {
         className = cn("w-80")
         align = "start"
         div {
            className = cn("grid gap-4")
            div {
               className = cn("grid gap-1.5")
               h4 {
                  className = cn("leading-none font-medium")
                  +"Dimensions"
               }
               p {
                  className = cn("text-muted-foreground text-sm")
                  +"Set the dimensions for the layer."
               }
            }
            div {
               className = cn("grid gap-2")
               div {
                  className = cn("grid grid-cols-3 items-center gap-4")
                  Label {
                     htmlFor = "width"
                     +"Width"
                  }
                  Input {
                     id = ElementId("width")
                     defaultValue = "100%"
                     className = cn("col-span-2 h-8")
                  }
               }
               div {
                  className = cn("grid grid-cols-3 items-center gap-4")
                  Label {
                     htmlFor = "maxWidth"
                     +"Max Width"
                  }
                  Input {
                     id = ElementId("maxWidth")
                     defaultValue = "300px"
                     className = cn("col-span-2 h-8")
                  }
               }
               div {
                  className = cn("grid grid-cols-3 items-center gap-4")
                  Label {
                     htmlFor = "height"
                     +"Height"
                  }
                  Input {
                     id = ElementId("height")
                     defaultValue = "25px"
                     className = cn("col-span-2 h-8")
                  }
               }
               div {
                  className = cn("grid grid-cols-3 items-center gap-4")
                  Label {
                     htmlFor = "maxHeight"
                     +"Max Height"
                  }
                  Input {
                     id = ElementId("maxHeight")
                     defaultValue = "none"
                     className = cn("col-span-2 h-8")
                  }
               }
            }
         }
      }
   }
}